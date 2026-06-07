use std::env;

use base64::Engine;
use panorama_core::device::{parse_fan_status, DeviceInfo};
use panorama_core::ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use panorama_core::protocol::Response;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewStatus {
    daemon_available: bool,
    device: Option<DeviceInfo>,
    display: CurrentDisplay,
    available_storage_bytes: Option<u64>,
    fan_lcd_rpm: Option<u32>,
    turbo_pump_rpm: Option<u32>,
    warnings: Vec<DeviceWarning>,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoolingStatus {
    daemon_available: bool,
    available_storage_bytes: Option<u64>,
    fan_lcd_rpm: Option<u32>,
    turbo_pump_rpm: Option<u32>,
    warnings: Vec<DeviceWarning>,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceWarning {
    kind: String,
    description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaList {
    files: Vec<String>,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaPreview {
    filename: String,
    preview_src: Option<String>,
    message: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentDisplay {
    saved: bool,
    screen_mode: String,
    ratio: String,
    pane1_media: Vec<String>,
    pane2_media: Vec<String>,
    pane1_preview_src: Option<String>,
    pane2_preview_src: Option<String>,
    pane1_metrics: Vec<String>,
    pane2_metrics: Vec<String>,
    pane1_badges: Vec<String>,
    pane2_badges: Vec<String>,
    metrics_color: String,
    metrics_align: String,
    metrics_position: String,
    display_filter: Option<String>,
    brightness: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyDisplayResult {
    display: CurrentDisplay,
    message: String,
}

#[derive(Deserialize)]
struct IpcMessagePayload {
    message: String,
}

#[tauri::command]
fn overview_status(app: tauri::AppHandle) -> Result<OverviewStatus, String> {
    let display = current_display(&app);

    let Some(device_response) = send_ipc(IpcRequest::new(IpcCommand::DeviceInfo))? else {
        return Ok(OverviewStatus {
            daemon_available: false,
            device: None,
            display,
            available_storage_bytes: None,
            fan_lcd_rpm: None,
            turbo_pump_rpm: None,
            warnings: Vec::new(),
            message: Some("Daemon IPC socket is not available".to_string()),
        });
    };

    let device = ipc_payload::<DeviceInfo>(device_response, "device info")?;

    let cooling = cooling_status()?;

    Ok(OverviewStatus {
        daemon_available: true,
        device: Some(device),
        display,
        available_storage_bytes: cooling.available_storage_bytes,
        fan_lcd_rpm: cooling.fan_lcd_rpm,
        turbo_pump_rpm: cooling.turbo_pump_rpm,
        warnings: cooling.warnings,
        message: None,
    })
}

fn current_display(app: &tauri::AppHandle) -> CurrentDisplay {
    let Some(state) = panorama_core::config::load_state() else {
        let default = panorama_core::config::DisplayState::default();
        return CurrentDisplay {
            saved: false,
            screen_mode: default.screen_mode,
            ratio: default.ratio,
            pane1_media: default.media,
            pane2_media: default.media2,
            pane1_preview_src: None,
            pane2_preview_src: None,
            pane1_metrics: default.sysinfo_display,
            pane2_metrics: default.sysinfo_display2,
            pane1_badges: default.badges,
            pane2_badges: default.badges2,
            metrics_color: default.metrics_color,
            metrics_align: default.metrics_align,
            metrics_position: default.metrics_position,
            display_filter: None,
            brightness: default.brightness,
        };
    };

    let pane1_preview_src = state
        .media
        .first()
        .and_then(|filename| cache_preview_media(app, filename));
    let pane2_preview_src = state
        .media2
        .first()
        .and_then(|filename| cache_preview_media(app, filename));

    CurrentDisplay {
        saved: true,
        screen_mode: state.screen_mode,
        ratio: state.ratio,
        pane1_media: state.media,
        pane2_media: state.media2,
        pane1_preview_src,
        pane2_preview_src,
        pane1_metrics: state.sysinfo_display,
        pane2_metrics: state.sysinfo_display2,
        pane1_badges: state.badges,
        pane2_badges: state.badges2,
        metrics_color: state.metrics_color,
        metrics_align: state.metrics_align,
        metrics_position: state.metrics_position,
        display_filter: (!state.display_filter.trim().is_empty()).then_some(state.display_filter),
        brightness: state.brightness,
    }
}

fn cache_preview_media(app: &tauri::AppHandle, filename: &str) -> Option<String> {
    if !panorama_core::adb::is_safe_media_filename(filename) {
        return None;
    }

    let preview_dir = app.path().app_cache_dir().ok()?.join("preview");
    std::fs::create_dir_all(&preview_dir).ok()?;
    let preview_path = preview_dir.join(filename);
    if !preview_path.exists() {
        let adb = panorama_core::adb::Adb::new();
        let local_path = preview_path.to_string_lossy().into_owned();
        if !adb.pull(filename, &local_path) {
            let _ = std::fs::remove_file(&preview_path);
            return None;
        }
    }

    media_data_url(&preview_path, filename)
}

fn media_data_url(path: &std::path::Path, filename: &str) -> Option<String> {
    const MAX_PREVIEW_BYTES: u64 = 32 * 1024 * 1024;

    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_PREVIEW_BYTES {
        return None;
    }

    let mime = preview_mime(filename)?;
    let bytes = std::fs::read(path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

fn preview_mime(filename: &str) -> Option<&'static str> {
    match panorama_core::media::normalize_ext(filename).as_str() {
        ".mp4" => Some("video/mp4"),
        ".webm" => Some("video/webm"),
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".bmp" => Some("image/bmp"),
        ".webp" => Some("image/webp"),
        _ => None,
    }
}

#[tauri::command]
fn cooling_status() -> Result<CoolingStatus, String> {
    let Some(state_response) = send_ipc(IpcRequest::new(IpcCommand::StateAll))? else {
        return Ok(CoolingStatus {
            daemon_available: false,
            available_storage_bytes: None,
            fan_lcd_rpm: None,
            turbo_pump_rpm: None,
            warnings: Vec::new(),
            message: Some("Daemon IPC socket is not available".to_string()),
        });
    };
    let state = ipc_payload::<Response>(state_response, "device status")?;
    Ok(cooling_from_response(&state))
}

#[tauri::command]
fn media_list() -> Result<MediaList, String> {
    let adb = panorama_core::adb::Adb::new();
    if !adb.is_device_connected() {
        return Ok(MediaList {
            files: Vec::new(),
            message: Some("No Panorama device detected by adb".to_string()),
        });
    }

    match adb.list_media() {
        Some(files) => Ok(MediaList {
            files,
            message: None,
        }),
        None => Ok(MediaList {
            files: Vec::new(),
            message: Some("Could not list media from /sdcard/pcMedia".to_string()),
        }),
    }
}

#[tauri::command]
fn media_preview(app: tauri::AppHandle, filename: String) -> Result<MediaPreview, String> {
    if !panorama_core::adb::is_safe_media_filename(&filename) {
        return Ok(MediaPreview {
            filename,
            preview_src: None,
            message: Some("Invalid AIO media filename".to_string()),
        });
    }

    let preview_src = cache_preview_media(&app, &filename);
    let message = if preview_src.is_some() {
        None
    } else {
        Some("Could not load preview for selected media".to_string())
    };

    Ok(MediaPreview {
        filename,
        preview_src,
        message,
    })
}

#[tauri::command]
fn apply_display(
    app: tauri::AppHandle,
    draft: CurrentDisplay,
) -> Result<ApplyDisplayResult, String> {
    let mut state = panorama_core::config::load_state().unwrap_or_default();
    let split = draft.screen_mode == "Screen Splitting";

    validate_selected_media(&draft.pane1_media)?;
    validate_selected_media(&draft.pane2_media)?;

    let plan = panorama_core::display::plan_display(
        panorama_core::display::DisplayPlanInput {
            file: draft.pane1_media.first().cloned(),
            split,
            media2: if split {
                Some(draft.pane2_media.first().cloned())
            } else {
                None
            },
            metrics: Some(metric_labels_to_tokens(&draft.pane1_metrics)?),
            metrics2: if split {
                Some(metric_labels_to_tokens(&draft.pane2_metrics)?)
            } else {
                None
            },
            badges: Some(draft.pane1_badges.clone()),
            badges2: if split {
                Some(draft.pane2_badges.clone())
            } else {
                None
            },
            metrics_color: Some(draft.metrics_color.clone()),
            metrics_align: Some(draft.metrics_align.clone()),
            metrics_position: Some(draft.metrics_position.clone()),
            filter: Some(draft.display_filter.clone().unwrap_or_default()),
            ratio: Some(draft.ratio.clone()),
        },
        state.clone(),
    )
    .map_err(|e| e.to_string())?;

    let request = IpcRequest::with_payload(IpcCommand::SetScreenConfig, &plan.screen)
        .map_err(|e| format!("could not build display IPC request: {e}"))?;
    let Some(response) = send_ipc(request)? else {
        return Err("Daemon IPC socket is not available".to_string());
    };
    ipc_expect_ok(response)?;

    state = plan.next_state;
    if !panorama_core::config::save_state(&state) {
        return Err("Display applied, but saving display state failed".to_string());
    }

    Ok(ApplyDisplayResult {
        display: current_display(&app),
        message: "Display applied".to_string(),
    })
}

fn validate_selected_media(files: &[String]) -> Result<(), String> {
    let Some(filename) = files.first() else {
        return Ok(());
    };
    if !panorama_core::adb::is_safe_media_filename(filename) {
        return Err(format!("Invalid AIO media filename: {filename}"));
    }
    let adb = panorama_core::adb::Adb::new();
    if !matches!(
        adb.validate_device(),
        panorama_core::adb::DeviceValidation::Confirmed(_)
    ) {
        return Err("Panorama media storage is not available over adb".to_string());
    }
    if !adb.file_exists(filename) {
        return Err(format!("Media not found on AIO: {filename}"));
    }
    Ok(())
}

fn metric_labels_to_tokens(labels: &[String]) -> Result<Vec<String>, String> {
    labels
        .iter()
        .map(|label| {
            panorama_core::display::metric_label_to_token(label)
                .map(str::to_string)
                .ok_or_else(|| format!("Unknown metric label: {label}"))
        })
        .collect()
}

fn cooling_from_response(response: &Response) -> CoolingStatus {
    let fan_status = parse_fan_status(response);
    CoolingStatus {
        daemon_available: true,
        available_storage_bytes: available_storage_bytes(response),
        fan_lcd_rpm: fan_status.fan_lcd_rpm,
        turbo_pump_rpm: fan_status.turbo_pump_rpm,
        warnings: parse_warnings(response),
        message: None,
    }
}

fn send_ipc(request: IpcRequest) -> Result<Option<IpcResponse>, String> {
    panorama_core::ipc::send_request(request).map_err(|e| e.to_string())
}

fn ipc_payload<T: serde::de::DeserializeOwned>(
    response: IpcResponse,
    context: &str,
) -> Result<T, String> {
    match response.status {
        IpcStatus::Ok => response
            .payload_as::<T>()
            .map_err(|e| format!("invalid {context} IPC payload: {e}"))?
            .ok_or_else(|| format!("daemon returned no {context} payload")),
        _ => Err(format!(
            "daemon {}: {}",
            ipc_status_name(response.status),
            ipc_response_message(&response)
        )),
    }
}

fn ipc_status_name(status: IpcStatus) -> &'static str {
    match status {
        IpcStatus::Ok => "ok",
        IpcStatus::BadRequest => "bad request",
        IpcStatus::Unsupported => "unsupported",
        IpcStatus::DeviceNotConnected => "device not connected",
        IpcStatus::DeviceError => "device error",
        IpcStatus::InternalError => "internal error",
    }
}

fn ipc_response_message(response: &IpcResponse) -> String {
    response
        .payload_as::<IpcMessagePayload>()
        .ok()
        .flatten()
        .map(|payload| payload.message)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| ipc_status_name(response.status).to_string())
}

fn ipc_expect_ok(response: IpcResponse) -> Result<(), String> {
    match response.status {
        IpcStatus::Ok => Ok(()),
        _ => Err(format!(
            "daemon {}: {}",
            ipc_status_name(response.status),
            ipc_response_message(&response)
        )),
    }
}

fn available_storage_bytes(response: &Response) -> Option<u64> {
    response
        .json
        .as_ref()
        .and_then(|json| json.get("availableStorage"))
        .and_then(|value| value.as_u64())
}

fn parse_warnings(response: &Response) -> Vec<DeviceWarning> {
    let Some(warnings) = response
        .json
        .as_ref()
        .and_then(|json| json.get("warning"))
        .and_then(|value| value.as_str())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
    else {
        return Vec::new();
    };

    warnings
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| {
            let description = warning
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if description.eq_ignore_ascii_case("No ERROR") {
                return None;
            }
            Some(DeviceWarning {
                kind: warning
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                description: if description.is_empty() {
                    "Unknown warning".to_string()
                } else {
                    description.to_string()
                },
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn configure_linux_webkit_environment() {
    let is_wayland_session = env::var_os("WAYLAND_DISPLAY").is_some()
        || matches!(env::var("XDG_SESSION_TYPE").as_deref(), Ok("wayland"));

    // Match the dev launcher workaround for WebKit on Wayland without
    // overriding an explicit user setting.
    if is_wayland_session && env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_webkit_environment() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_linux_webkit_environment();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            overview_status,
            cooling_status,
            media_list,
            media_preview,
            apply_display
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
