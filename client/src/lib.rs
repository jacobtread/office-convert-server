use async_trait::async_trait;
use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use thiserror::Error;

pub mod load;

pub use load::{LoadBalanceError, OfficeConvertLoadBalancer};

/// Trait implement by entities that can convert office files into
/// PDF files.
#[async_trait]
pub trait ConvertOffice {
    /// Converts the provided office file format bytes into a
    /// PDF returning the PDF file bytes
    ///
    /// ## Arguments
    /// * `file` - The file bytes to convert
    async fn convert(&self, file: Vec<u8>) -> Result<Bytes, RequestError>;
}

#[derive(Clone)]
pub struct OfficeConvertClient {
    /// HTTP client to connect to the server with
    http: reqwest::Client,
    /// Host the office convert server is running on
    host: Arc<str>,
}

/// Errors that can occur during setup
#[derive(Debug, Error)]
pub enum CreateError {
    /// Builder failed to create HTTP client
    #[error(transparent)]
    Builder(reqwest::Error),
}

/// Errors that can occur during a request
#[derive(Debug, Error)]
pub enum RequestError {
    /// Failed to request the server
    #[error(transparent)]
    RequestFailed(reqwest::Error),

    /// Response from the server was invalid
    #[error(transparent)]
    InvalidResponse(reqwest::Error),

    /// Reached timeout when trying to connect
    #[error("server connection timed out")]
    ServerConnectTimeout,

    /// Error message from the convert server reply
    #[error("{reason}")]
    ErrorResponse {
        reason: String,
        backtrace: Option<String>,
    },
}

#[derive(Deserialize)]
pub struct StatusResponse {
    pub is_busy: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    /// Server reason for the error
    reason: String,
    /// Server backtrace if available
    backtrace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// Connection timeout used when checking the status of the server
    pub connect_timeout: Option<Duration>,

    /// Timeout when reading responses from the server
    pub read_timeout: Option<Duration>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            // Allow the connection to fail if not established in 700ms
            connect_timeout: Some(Duration::from_millis(700)),
            read_timeout: None,
        }
    }
}

impl OfficeConvertClient {
    /// Creates a new office convert client using the default options
    ///
    /// ## Arguments
    /// * `host` - The host where the server is located
    pub fn new<T>(host: T) -> Result<Self, CreateError>
    where
        T: Into<Arc<str>>,
    {
        Self::new_with_options(host, ClientOptions::default())
    }

    /// Creates a new office convert client using the provided options
    ///
    /// ## Arguments
    /// * `host` - The host where the server is located
    /// * `options` - The configuration options for the client
    pub fn new_with_options<T>(host: T, options: ClientOptions) -> Result<Self, CreateError>
    where
        T: Into<Arc<str>>,
    {
        let mut builder = reqwest::Client::builder();

        if let Some(connect_timeout) = options.connect_timeout {
            builder = builder.connect_timeout(connect_timeout);
        }

        if let Some(connect_timeout) = options.read_timeout {
            builder = builder.read_timeout(connect_timeout);
        }

        let client = builder.build().map_err(CreateError::Builder)?;
        Self::from_client(host, client)
    }

    /// Create an office convert client from an existing [reqwest::Client] if
    /// your setup is more advanced than the default configuration
    ///
    /// ## Arguments
    /// * `host` - The host where the server is located
    /// * `client` - The request HTTP client to use
    pub fn from_client<T>(host: T, client: reqwest::Client) -> Result<Self, CreateError>
    where
        T: Into<Arc<str>>,
    {
        Ok(Self {
            http: client,
            host: host.into(),
        })
    }

    /// Obtains the current status of the converter server
    pub async fn get_status(&self) -> Result<StatusResponse, RequestError> {
        let route = format!("{}/status", self.host);
        let response = self
            .http
            .get(route)
            .send()
            .await
            .map_err(RequestError::RequestFailed)?;

        let status = response.status();

        // Handle error responses
        if status.is_client_error() || status.is_server_error() {
            let body: ErrorResponse = response
                .json()
                .await
                .map_err(RequestError::InvalidResponse)?;

            return Err(RequestError::ErrorResponse {
                reason: body.reason,
                backtrace: body.backtrace,
            });
        }

        // Extract the response message
        let response: StatusResponse = response
            .json()
            .await
            .map_err(RequestError::InvalidResponse)?;

        Ok(response)
    }

    /// Gets the current busy status of the convert server
    pub async fn is_busy(&self) -> Result<bool, RequestError> {
        let status = self.get_status().await?;
        Ok(status.is_busy)
    }

    /// Tells the converter server to collect garbage
    pub async fn collect_garbage(&self) -> Result<(), RequestError> {
        let route = format!("{}/collect-garbage", self.host);
        let response = self
            .http
            .post(route)
            .send()
            .await
            .map_err(RequestError::RequestFailed)?;

        let status = response.status();

        // Handle error responses
        if status.is_client_error() || status.is_server_error() {
            let body: ErrorResponse = response
                .json()
                .await
                .map_err(RequestError::InvalidResponse)?;

            return Err(RequestError::ErrorResponse {
                reason: body.reason,
                backtrace: body.backtrace,
            });
        }

        Ok(())
    }
}

#[async_trait]
impl ConvertOffice for OfficeConvertClient {
    async fn convert(&self, file: Vec<u8>) -> Result<Bytes, RequestError> {
        let route = format!("{}/convert", self.host);
        let form = Form::new().part("file", Part::bytes(file));
        let response = self
            .http
            .post(route)
            .multipart(form)
            .send()
            .await
            .map_err(RequestError::RequestFailed)?;

        let status = response.status();

        // Handle error responses
        if status.is_client_error() || status.is_server_error() {
            let body: ErrorResponse = response
                .json()
                .await
                .map_err(RequestError::InvalidResponse)?;

            return Err(RequestError::ErrorResponse {
                reason: body.reason,
                backtrace: body.backtrace,
            });
        }

        let response = response
            .bytes()
            .await
            .map_err(RequestError::InvalidResponse)?;

        Ok(response)
    }
}
