use crate::core::vnc::{self, VncInputEvent, VncSessionManager};
use crate::error::AppResult;
use std::sync::Arc;
use tauri::Emitter;
use tauri::ipc::{Channel, InvokeResponseBody};

#[tauri::command]
pub async fn create_vnc_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<VncSessionManager>>,
    connection_id: String,
    _create_request_id: Option<String>,
) -> AppResult<String> {
    let config = vnc::load_saved_vnc_config(&app, &connection_id)?;
    let session_id = state.create_session(app.clone(), config).await?;
    if let Err(error) = crate::storage::mark_connection_used(&connection_id) {
        tracing::warn!(connection_id, %error, "Failed to mark VNC connection as recently used");
    } else {
        let _ = app.emit("connections-changed", ());
    }
    Ok(session_id)
}

#[tauri::command]
pub async fn vnc_attach_frame_channel(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<VncSessionManager>>,
    session_id: String,
    frame_channel: Channel<InvokeResponseBody>,
) -> AppResult<()> {
    state
        .attach_frame_channel(&app, &session_id, frame_channel)
        .await
}

#[tauri::command]
pub async fn vnc_input_batch(
    state: tauri::State<'_, Arc<VncSessionManager>>,
    session_id: String,
    events: Vec<VncInputEvent>,
) -> AppResult<()> {
    state.send_input(&session_id, events).await
}

#[tauri::command]
pub async fn vnc_set_clipboard_text(
    state: tauri::State<'_, Arc<VncSessionManager>>,
    session_id: String,
    text: String,
) -> AppResult<()> {
    state.set_clipboard_text(&session_id, text).await
}

#[tauri::command]
pub async fn vnc_reconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<VncSessionManager>>,
    session_id: String,
) -> AppResult<()> {
    state.inner().clone().reconnect(app, &session_id).await
}

#[tauri::command]
pub async fn respond_vnc_server_key(
    state: tauri::State<'_, Arc<VncSessionManager>>,
    request_id: String,
    accepted: bool,
) -> AppResult<()> {
    state.respond_server_key(&request_id, accepted).await
}

#[tauri::command]
pub async fn close_vnc_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<VncSessionManager>>,
    session_id: String,
) -> AppResult<()> {
    state.close(&app, &session_id).await
}
