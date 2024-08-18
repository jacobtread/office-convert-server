use anyhow::Context;
use axum::{
    body::Body, extract::DefaultBodyLimit, http::Response, routing::get, Extension, Json, Router,
};
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use bytes::Bytes;
use error::DynHttpError;
use libreofficesdk::{urls, CallbackType, JSDialog, Office};
use rand::{distributions::Alphanumeric, Rng};
use serde::Serialize;
use std::{
    env::temp_dir,
    ffi::{CStr, CString},
};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;
use tracing_subscriber::EnvFilter;

mod error;

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

    let office_path =
        std::env::var("LIBREOFFICE_SDK_PATH").context("missing LIBREOFFICE_SDK_PATH")?;
    let server_address = std::env::var("SERVER_ADDRESS").context("missing SERVER_ADDRESS")?;

    // Create office access
    let office_handle = create_office_runner(office_path);

    // Create the router
    let app = Router::new()
        .route("/", get(is_busy).post(convert))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))
        .layer(Extension(office_handle));

    // Create a TCP listener
    let listener = tokio::net::TcpListener::bind(&server_address)
        .await
        .context("failed to bind http server")?;

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

    std::thread::spawn(|| office_runner(path, rx));

    OfficeHandle(tx)
}

/// Main event loop for an office runner
fn office_runner(path: String, mut rx: mpsc::Receiver<OfficeMsg>) -> anyhow::Result<()> {
    // Create office instance
    let mut office = Office::new(&path).context("failed to create office instance")?;

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

    let mut o2 = office.clone();

    office
        .register_callback(move |ty, payload| {
            let value = &*payload.to_string_lossy();
            debug!(?ty, %value, "callback invoked");

            if let CallbackType::JSDialog = ty {
                if value.contains("is corrupt and therefore cannot be opened") {
                    let value: serde_json::Value = serde_json::from_str(value).unwrap();
                    let dialog = JSDialog(value);
                    let id = dialog.get_id();

                    if let Some(id) = id {
                        let a = CString::new("{\"id\": \"no\", \"response\": 3}").unwrap();
                        debug!(%id, "sending dialog event");
                        o2.send_dialog_event(id, std::ptr::null()).unwrap();
                    }
                }
            }
        })
        .context("failed to register office callback")?;

    // Get next message
    while let Some(msg) = rx.blocking_recv() {
        let (input, output) = match msg {
            OfficeMsg::Convert { bytes, tx } => (bytes, tx),
            // Busy checks are ignored
            OfficeMsg::BusyCheck => continue,
        };

        // Convert document
        let result = convert_document(&mut office, temp_in_path, temp_out_path, input);

        // Send response
        _ = output.send(result);
    }

    Ok(())
}

/// Converts the provided document bytes into PDF format returning
/// the converted bytes
fn convert_document(
    office: &mut Office,
    temp_in_path: &str,
    temp_out_path: &str,
    input: Bytes,
) -> anyhow::Result<Bytes> {
    // Write to temp file
    std::fs::write(temp_in_path, input).context("failed to write temp input")?;

    let output_url = urls::local_as_abs(temp_out_path).context("failed to create output url")?;

    // Load document
    let input_url = urls::local_into_abs(temp_in_path).context("failed to create input url")?;
    let mut doc = office
        .document_load(input_url)
        .context("failed to load document")?;

    debug!("document loaded");

    // Convert document
    let result = doc.save_as(output_url, "pdf", None);

    if result {
        debug!("conversion finished");
    } else {
        debug!("failed to convert")
    }

    // Read document context
    let bytes = std::fs::read(temp_out_path).context("failed to read temp out file")?;

    Ok(Bytes::from(bytes))
}

/// Request to convert a file
#[derive(TryFromMultipart)]
struct UploadAssetRequest {
    /// The file to convert
    #[form_data(limit = "unlimited")]
    file: FieldData<Bytes>,
}

/// POST /
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
        .body(Body::from(converted))
        .context("failed to create response")?;

    Ok(response)
}

/// Result from checking the server busy state
#[derive(Serialize)]
struct BusyResult {
    /// Whether the server is busy
    is_busy: bool,
}

/// GET /
///
/// Checks if the converter is currently busy
async fn is_busy(Extension(office): Extension<OfficeHandle>) -> Json<BusyResult> {
    let is_locked = office.0.try_send(OfficeMsg::BusyCheck).is_err();
    Json(BusyResult { is_busy: is_locked })
}
