use crate::{ConvertOffice, OfficeConvertClient, RequestError};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
    sync::{Mutex, Notify},
    time::Instant,
};
use tracing::{debug, error};

/// Round robbin load balancer, will pass convert jobs
/// around to the next available client, connections
/// will wait until there is an available client
#[derive(Clone)]
pub struct OfficeConvertLoadBalancer {
    /// Inner portion of the load balancer
    inner: Arc<OfficeConvertLoadBalancerInner>,
}

impl OfficeConvertLoadBalancer {
    /// Creates a load balancer from the provided collection of clients
    ///
    /// ## Arguments
    /// * `clients` - The clients to load balance amongst
    pub fn new<I>(clients: I) -> Self
    where
        I: IntoIterator<Item = OfficeConvertClient>,
    {
        let clients = clients
            .into_iter()
            .map(|client| {
                Mutex::new(LoadBalancedClient {
                    client,
                    busy_externally_at: None,
                })
            })
            .collect::<Vec<_>>();

        let inner = OfficeConvertLoadBalancerInner {
            clients,
            free_notify: Notify::new(),
        };

        Self {
            inner: Arc::new(inner),
        }
    }
}

struct OfficeConvertLoadBalancerInner {
    // Available clients the load balancer can use
    clients: Vec<Mutex<LoadBalancedClient>>,

    /// Notifier for connections that are no longer busy
    free_notify: Notify,
}

struct LoadBalancedClient {
    /// The actual client
    client: OfficeConvertClient,

    /// Last time the server reported as busy externally
    busy_externally_at: Option<Instant>,
}

#[derive(Debug, Error)]
pub enum LoadBalanceError {
    #[error("no servers available for load balancing")]
    NoServers,
}

/// Time in-between external busy checks
const RETRY_BUSY_CHECK_AFTER: Duration = Duration::from_secs(5);

#[async_trait]
impl ConvertOffice for OfficeConvertLoadBalancer {
    async fn convert(&self, file: Vec<u8>) -> Result<bytes::Bytes, RequestError> {
        let inner = &*self.inner;

        loop {
            for (index, client) in inner.clients.iter().enumerate() {
                let mut client = match client.try_lock() {
                    Ok(value) => value,
                    // Server is already in use
                    Err(_) => continue,
                };

                let client = &mut *client;

                let now = Instant::now();

                if let Some(busy_externally_at) = client.busy_externally_at {
                    let since_check = now.duration_since(busy_externally_at);

                    // Don't check this server if the busy check timeout hasn't passed
                    if since_check < RETRY_BUSY_CHECK_AFTER {
                        continue;
                    }
                }

                // Check if the server is busy externally (Busy outside of our control)
                let externally_busy = match client.client.is_busy().await {
                    Ok(value) => value,
                    Err(err) => {
                        error!("failed to perform server busy check at {index}: {err}");

                        // Mark erroneous servers as busy
                        true
                    }
                };

                // Store the busy state if busy
                if externally_busy {
                    debug!("server at {index} is busy externally");

                    client.busy_externally_at = Some(now);
                    continue;
                }

                debug!("obtained available server {index} for convert");

                let response = client.client.convert(file).await;

                // Notify waiters that this server is now free
                inner.free_notify.notify_waiters();

                return response;
            }

            debug!("no available servers, waiting until one is available");

            // All servers are in use, wait for the free notifier
            inner.free_notify.notified().await;
        }
    }
}
