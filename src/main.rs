use anyhow::{anyhow, Context};
use axum::{
    body::Body,
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Response, StatusCode},
    routing::{get, post},
    Extension, Json, Router,
};
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use bytes::Bytes;
use clap::Parser;
use error::DynHttpError;
use libreofficekit::{DocUrl, Office, OfficeError, OfficeOptionalFeatures};
use rand::{distributions::Alphanumeric, Rng};
use serde::Serialize;
use std::env::temp_dir;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

mod error;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the office installation (Omit to determine automatically)
    #[arg(long)]
    office_path: Option<String>,

    /// Port to bind the server to, defaults to 8080
    #[arg(long)]
    port: Option<u16>,

    /// Host to bind the server to, defaults to 0.0.0.0
    #[arg(long)]
    host: Option<String>,

    /// Logging level to use
    #[arg(long, default_value_t = LevelFilter::INFO)]
    logging: LevelFilter,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    _ = dotenvy::dotenv();

    // Start configuring a `fmt` subscriber
    let subscriber = tracing_subscriber::fmt()
        // Use the logging options from env variables
        .with_env_filter(EnvFilter::from_default_env())
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
        // Build the subscriber
        .finish();

    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Args::parse();

    let mut office_path: Option<String> = args.office_path;

    // Try loading office path from environment variables
    if office_path.is_none() {
        office_path = std::env::var("LIBREOFFICE_SDK_PATH").ok()
    }

    // Try determine default office path
    if office_path.is_none() {
        office_path = Office::find_install_path().map(|value| value.to_string());
    }

    // Check a path was provided
    let office_path = match office_path {
        Some(value) => value,
        None => {
            error!("no office install path provided, cannot start server");
            panic!();
        }
    };

    debug!("using libreoffice install from: {office_path}");

    // Determine the address to run the server on
    let server_address = if args.host.is_some() || args.port.is_some() {
        let host = args.host.unwrap_or_else(|| "0.0.0.0".to_string());
        let port = args.port.unwrap_or(8080);

        format!("{host}:{port}")
    } else {
        std::env::var("SERVER_ADDRESS").context("missing SERVER_ADDRESS")?
    };

    // Create office access
    let office_handle = create_office_runner(office_path);

    // Create the router
    let app = Router::new()
        .route("/status", get(status))
        .route("/convert", post(convert))
        .route("/collect-garbage", post(collect_garbage))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))
        .layer(Extension(office_handle));

    // Create a TCP listener
    let listener = tokio::net::TcpListener::bind(&server_address)
        .await
        .context("failed to bind http server")?;

    debug!("server started on: {server_address}");

    // Serve the app from the listener
    axum::serve(listener, app)
        .await
        .context("failed to serve")?;

    Ok(())
}

/// Messages the office runner can process
pub enum OfficeMsg {
    /// Message to convert a file
    Convert {
        /// The file bytes to convert
        bytes: Bytes,

        /// The return channel for sending back the result
        tx: oneshot::Sender<anyhow::Result<Bytes>>,
    },

    /// Tells office to clean up and trim its memory usage
    CollectGarbage,

    /// Message to check if the server is busy, ignored
    BusyCheck,
}

/// Handle to send messages to the office runner
#[derive(Clone)]
pub struct OfficeHandle(mpsc::Sender<OfficeMsg>);

/// Creates a new office runner on its own thread providing
/// a handle to access it via messages
fn create_office_runner(path: String) -> OfficeHandle {
    let (tx, rx) = mpsc::channel(1);

    std::thread::spawn(|| {
        if let Err(cause) = office_runner(path, rx) {
            error!(%cause, "failed to start office runner")
        }
    });

    OfficeHandle(tx)
}

/// Main event loop for an office runner
fn office_runner(path: String, mut rx: mpsc::Receiver<OfficeMsg>) -> anyhow::Result<()> {
    // Create office instance
    let office = Office::new(&path).context("failed to create office instance")?;

    let tmp_dir = temp_dir();

    // Generate random ID for the path name
    let random_id = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(|value| value as char)
        .collect::<String>();

    // Create input and output paths
    let temp_in = tmp_dir.join(format!("lo_native_input_{random_id}"));
    let temp_out = tmp_dir.join(format!("lo_native_output_{random_id}.pdf"));

    // Convert paths to strings
    let temp_in_path = temp_in.to_str().context("failed to create temp in path")?;
    let temp_out_path = temp_out
        .to_str()
        .context("failed to create temp out path")?;

    // Create office type safe paths
    let input_url = DocUrl::local_as_abs(temp_in_path).context("failed to create input url")?;
    let output_url = DocUrl::local_as_abs(temp_out_path).context("failed to create output url")?;

    // Allow prompting for passwords
    office
        .set_optional_features(
            OfficeOptionalFeatures::DOCUMENT_PASSWORD
                | OfficeOptionalFeatures::DOCUMENT_PASSWORD_TO_MODIFY,
        )
        .context("failed to set optional features")?;

    office
        .register_callback(|ty, _payload| {
            debug!(?ty, "callback invoked");
        })
        .context("failed to register office callback")?;

    // Get next message
    while let Some(msg) = rx.blocking_recv() {
        let (input, output) = match msg {
            OfficeMsg::Convert { bytes, tx } => (bytes, tx),

            OfficeMsg::CollectGarbage => {
                if let Err(cause) = office.trim_memory(2000) {
                    error!(%cause, "failed to collect garbage")
                }
                continue;
            }
            // Busy checks are ignored
            OfficeMsg::BusyCheck => continue,
        };

        // Convert document
        let result = convert_document(
            &office,
            temp_in_path,
            temp_out_path,
            &input_url,
            &output_url,
            input,
        );

        // Send response
        _ = output.send(result);
    }

    Ok(())
}

/// Converts the provided document bytes into PDF format returning
/// the converted bytes
fn convert_document(
    office: &Office,

    temp_in_str: &str,
    temp_out_str: &str,

    temp_in_path: &DocUrl,
    temp_out_path: &DocUrl,
    input: Bytes,
) -> anyhow::Result<Bytes> {
    // Write to temp file
    std::fs::write(temp_in_str, input).context("failed to write temp input")?;

    // Load document
    let mut doc = match office.document_load_with_options(temp_in_path, "Batch=1") {
        Ok(value) => value,
        Err(err) => match err {
            OfficeError::OfficeError(err) => {
                error!(%err, "failed to load document");

                if err.contains("loadComponentFromURL returned an empty reference") {
                    return Err(anyhow!("file is corrupted"));
                }

                if err.contains("Unsupported URL") {
                    return Err(anyhow!("file is encrypted"));
                }

                return Err(OfficeError::OfficeError(err).into());
            }
            err => return Err(err.into()),
        },
    };

    debug!("document loaded");

    // Convert document
    let result = doc.save_as(temp_out_path, "pdf", None)?;

    // Attempt to free up some memory
    _ = office.trim_memory(1000);

    if !result {
        return Err(anyhow!("failed to convert file"));
    }

    // Read document context
    let bytes = std::fs::read(temp_out_str).context("failed to read temp out file")?;

    Ok(Bytes::from(bytes))
}

/// Request to convert a file
#[derive(TryFromMultipart)]
struct UploadAssetRequest {
    /// The file to convert
    #[form_data(limit = "unlimited")]
    file: FieldData<Bytes>,
}

/// POST /convert
///
/// Converts the provided file to PDF format responding with the PDF file
async fn convert(
    Extension(office): Extension<OfficeHandle>,
    TypedMultipart(UploadAssetRequest { file }): TypedMultipart<UploadAssetRequest>,
) -> Result<Response<Body>, DynHttpError> {
    let (tx, rx) = oneshot::channel();

    // Convert the file
    office
        .0
        .send(OfficeMsg::Convert {
            bytes: file.contents,
            tx,
        })
        .await
        .context("failed to send convert request")?;

    // Wait for the response
    let converted = rx.await.context("failed to get convert response")??;

    // Build the response
    let response = Response::builder()
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/pdf"),
        )
        .body(Body::from(converted))
        .context("failed to create response")?;

    Ok(response)
}

/// Result from checking the server busy state
#[derive(Serialize)]
struct StatusResponse {
    /// Whether the server is busy
    is_busy: bool,
}

/// GET /status
///
/// Checks if the converter is currently busy
async fn status(Extension(office): Extension<OfficeHandle>) -> Json<StatusResponse> {
    let is_locked = office.0.try_send(OfficeMsg::BusyCheck).is_err();
    Json(StatusResponse { is_busy: is_locked })
}

/// POST /collect-garbage
///
/// Collects garbage from the office converter
async fn collect_garbage(Extension(office): Extension<OfficeHandle>) -> StatusCode {
    _ = office.0.send(OfficeMsg::CollectGarbage).await;
    StatusCode::OK
}
