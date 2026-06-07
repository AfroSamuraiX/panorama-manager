//! High-level command surface for the TRYX Panorama display.
//!
//! Owns a [`SerialTransport`] and a sequence counter. Each method builds the
//! request JSON, frames it via [`crate::protocol`], and dispatches through
//! the transport. JSON construction lives in pure helper functions below so
//! payload shapes are unit-testable without hardware.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::protocol::{build_frame, parse_response, Response};
use crate::transport::{SerialTransport, TransportError};

const RECEIVE_BUFFER_BYTES: usize = 4096;
const SCREEN_CONFIG_RESEND_DELAY: Duration = Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("device not connected — call connect() first")]
    NotConnected,

    #[error("transport: {0}")]
    Transport(#[from] TransportError),

    #[error("no response received from device")]
    NoResponse,

    #[error("response malformed: could not parse frame")]
    MalformedResponse,

    #[error(
        "device handshake succeeded but attributes do not match Panorama signature — \
         found product '{product_id}' with attributes {attributes:?}"
    )]
    NotPanorama {
        product_id: String,
        attributes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub product_id: String,
    pub os: String,
    pub serial: String,
    pub app_version: String,
    pub firmware: String,
    pub hardware: String,
    pub attributes: Vec<String>,
}

impl DeviceInfo {
    fn unknown() -> Self {
        let u = || "unknown".to_string();
        Self {
            product_id: u(),
            os: u(),
            serial: u(),
            app_version: u(),
            firmware: u(),
            hardware: u(),
            attributes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub position: String,
    pub color: String,
    pub align: String,
    pub badges: Vec<String>,
    pub filter_value: String,
    pub filter_opacity: i32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            position: "Top".to_string(),
            color: "#FFFFFF".to_string(),
            align: "Left".to_string(),
            badges: Vec::new(),
            filter_value: String::new(),
            filter_opacity: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenConfig {
    /// e.g. "Pre-set 1: Cooling delivery". Empty for custom mode.
    pub preset_id: String,
    pub media: Vec<String>,
    pub screen_mode: String,
    pub ratio: String,
    pub play_mode: String,
    /// Max 3 labels.
    pub sysinfo_display: Vec<String>,
    pub settings: DisplaySettings,
    /// Used only when `screen_mode == "Screen Splitting"`.
    pub settings2: DisplaySettings,
    pub sysinfo_display2: Vec<String>,
    pub waterfall_mode: bool,
    /// Whether the cooler keeps its screen lit through its idle timeout.
    /// `true` keeps the display on; `false` lets the screen sleep when idle.
    /// Only carried by the full `POST config` payload.
    pub display_in_sleep: bool,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            preset_id: String::new(),
            media: Vec::new(),
            screen_mode: "Full Screen".to_string(),
            ratio: "2:1".to_string(),
            play_mode: "Single".to_string(),
            sysinfo_display: Vec::new(),
            settings: DisplaySettings::default(),
            settings2: DisplaySettings::default(),
            sysinfo_display2: Vec::new(),
            waterfall_mode: false,
            display_in_sleep: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SysinfoData {
    pub label: String,
    pub value: String,
    pub unit: String,
}

/// Fan and pump telemetry the device reports back in every `STATE all`
/// response. Both fields are `None` when the firmware omits them, so callers
/// can distinguish missing from present-but-zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanStatus {
    pub fan_lcd_rpm: Option<u32>,
    pub turbo_pump_rpm: Option<u32>,
}

pub struct Device {
    transport: Option<SerialTransport>,
    seq_number: u64,
}

impl Device {
    pub fn new() -> Self {
        Self {
            transport: None,
            seq_number: 0,
        }
    }

    /// Open the serial port without handshaking. Most callers want
    /// [`Device::connect`]; this granular form exists for `doctor`, which
    /// reports port-open and handshake status as separate diagnostics.
    pub fn connect_port(&mut self) -> Result<(), DeviceError> {
        let transport = SerialTransport::open()?;
        self.transport = Some(transport);
        Ok(())
    }

    /// Open the serial port and complete the `POST conn` handshake. The device
    /// rejects every other command until it has been handshaked, so the two
    /// always go together — this folds them into one call and returns the
    /// [`DeviceInfo`] the handshake reports.
    pub fn connect(&mut self) -> Result<DeviceInfo, DeviceError> {
        self.connect_port()?;
        self.handshake()
    }

    pub fn disconnect(&mut self) {
        self.transport = None;
    }

    pub fn is_connected(&self) -> bool {
        self.transport.is_some()
    }

    pub fn handshake(&mut self) -> Result<DeviceInfo, DeviceError> {
        let response = self
            .send_command("POST", "conn", "", true)?
            .ok_or(DeviceError::NoResponse)?;
        let json = response.json.ok_or(DeviceError::MalformedResponse)?;
        let info = parse_device_info(&json);
        validate_panorama_attributes(&info)?;
        Ok(info)
    }

    pub fn set_brightness(&mut self, value: i32) -> Result<Option<Response>, DeviceError> {
        let content = json!({ "value": value }).to_string();
        self.send_command("POST", "brightness", &content, true)
    }

    pub fn set_fan_lcd(&mut self, percent: i32) -> Result<Option<Response>, DeviceError> {
        let content = build_fan_lcd_payload(percent).to_string();
        self.send_command("POST", "fanLCDSet", &content, true)
    }

    pub fn reboot(&mut self) -> Result<Option<Response>, DeviceError> {
        self.send_command("POST", "reboot", "", true)
    }

    pub fn set_waterfall_mode(&mut self, enable: bool) -> Result<Option<Response>, DeviceError> {
        let content = json!({ "enable": enable }).to_string();
        self.send_command("POST", "waterfallMode", &content, true)
    }

    /// `unit` is `"Celsius"` or `"Fahrenheit"`.
    pub fn set_temperature_unit(&mut self, unit: &str) -> Result<Option<Response>, DeviceError> {
        let content = json!({ "value": unit }).to_string();
        self.send_command("POST", "temperature", &content, true)
    }

    pub fn delete_media(&mut self, files: &[String]) -> Result<Option<Response>, DeviceError> {
        let content = json!({ "include": files }).to_string();
        self.send_command("POST", "mediaDelete", &content, true)
    }

    /// Apply a screen config. Sends the same payload twice with a 200 ms gap —
    /// the device firmware sometimes drops the first one. After both sends,
    /// applies the waterfall-mode flag.
    pub fn set_screen_config(
        &mut self,
        config: &ScreenConfig,
    ) -> Result<Option<Response>, DeviceError> {
        let content = build_screen_config_payload(config).to_string();
        let _ = self.send_command("POST", "waterBlockScreenId", &content, true)?;
        std::thread::sleep(SCREEN_CONFIG_RESEND_DELAY);
        let result = self.send_command("POST", "waterBlockScreenId", &content, true)?;
        std::thread::sleep(SCREEN_CONFIG_RESEND_DELAY);
        self.set_waterfall_mode(config.waterfall_mode)?;
        Ok(result)
    }

    pub fn set_sysinfo_display(
        &mut self,
        config: &ScreenConfig,
    ) -> Result<Option<Response>, DeviceError> {
        let content = json!({ "items": &config.sysinfo_display }).to_string();
        self.send_command("POST", "sysinfoDisplay", &content, false)
    }

    /// Send hardware spec (CPU/GPU names + temperature unit). Used for side
    /// effects only — the device's response carries no useful data here.
    pub fn send_config(
        &mut self,
        cpu_name: &str,
        gpu_name: &str,
        temp_unit: &str,
    ) -> Result<(), DeviceError> {
        let spec = json!({ "cpu": cpu_name, "gpu": gpu_name }).to_string();
        self.send_command("POST", "spec", &spec, true)?;
        self.set_temperature_unit(temp_unit)?;
        Ok(())
    }

    /// Send the combined Kanali-format config payload (screen + spec +
    /// temperature unit) in one command.
    pub fn send_full_config(
        &mut self,
        config: &ScreenConfig,
        cpu_name: &str,
        gpu_name: &str,
        brightness: i32,
        temp_unit: &str,
    ) -> Result<Option<Response>, DeviceError> {
        let content = build_full_config_payload(config, cpu_name, gpu_name, brightness, temp_unit)
            .to_string();
        self.send_command("POST", "config", &content, true)
    }

    pub fn send_sysinfo(&mut self, data: &[SysinfoData]) -> Result<Option<Response>, DeviceError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let local_ms = now_ms + local_utc_offset_ms();
        let content = build_sysinfo_payload(data, local_ms).to_string();
        self.send_command("STATE", "all", &content, true)
    }

    /// Query the device for current fan and pump RPM. Sends an empty-metric
    /// `STATE all` payload — the firmware reports `status.fanLCD` and
    /// `status.turboPump` independently of the metric values the host pushed.
    pub fn read_fan_status(&mut self) -> Result<FanStatus, DeviceError> {
        let response = self.send_sysinfo(&[])?.ok_or(DeviceError::NoResponse)?;
        Ok(parse_fan_status(&response))
    }

    pub fn send_command(
        &mut self,
        request_state: &str,
        cmd_type: &str,
        content: &str,
        wait_response: bool,
    ) -> Result<Option<Response>, DeviceError> {
        let transport = self.transport.as_mut().ok_or(DeviceError::NotConnected)?;
        self.seq_number += 1;

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let local_ms = now_ms + local_utc_offset_ms();

        let frame = build_frame(
            request_state,
            cmd_type,
            content,
            "1",
            self.seq_number,
            local_ms,
        );
        // Drop any stale or unsolicited bytes so the response we read back
        // is the one for this command, not a leftover from an earlier one.
        transport.clear_input();
        transport.send(&frame)?;

        if !wait_response {
            return Ok(None);
        }

        let raw = transport.receive(RECEIVE_BUFFER_BYTES)?;
        if raw.is_empty() {
            return Err(DeviceError::NoResponse);
        }

        parse_response(&raw)
            .map(Some)
            .ok_or(DeviceError::MalformedResponse)
    }
}

impl Default for Device {
    fn default() -> Self {
        Self::new()
    }
}

// --- Pure helpers (unit-testable) ---

/// Return the host's local UTC offset in milliseconds (positive = east of UTC).
/// The device firmware renders the `Date=` header as wall-clock time without
/// applying any timezone, so callers must add this to UTC epoch ms before
/// sending so the device displays local time.
pub fn local_utc_offset_ms() -> i64 {
    chrono::Local::now().offset().local_minus_utc() as i64 * 1000
}

/// Validates that device attributes match Panorama cooler signatures.
///
/// The Panorama family reports distinctive attributes that are not present on
/// generic Android devices. Requires at least two of the known Panorama
/// attributes to be present for positive identification.
///
/// Known attributes from hardware verification:
/// - "Status"
/// - "Water Block Screen"
/// - "Fan LCD|rw"
/// - "Turbo Pump"
fn validate_panorama_attributes(info: &DeviceInfo) -> Result<(), DeviceError> {
    const PANORAMA_ATTRIBUTES: &[&str] =
        &["Status", "Water Block Screen", "Fan LCD|rw", "Turbo Pump"];

    let matches = info
        .attributes
        .iter()
        .filter(|attr| PANORAMA_ATTRIBUTES.contains(&attr.as_str()))
        .count();

    if matches >= 2 {
        Ok(())
    } else {
        Err(DeviceError::NotPanorama {
            product_id: info.product_id.clone(),
            attributes: info.attributes.clone(),
        })
    }
}

fn parse_device_info(json: &Value) -> DeviceInfo {
    let mut info = DeviceInfo::unknown();
    if let Some(s) = json.get("productId").and_then(Value::as_str) {
        info.product_id = s.to_string();
    }
    if let Some(s) = json.get("OS").and_then(Value::as_str) {
        info.os = s.to_string();
    }
    if let Some(s) = json.get("sn").and_then(Value::as_str) {
        info.serial = s.to_string();
    }
    if let Some(v) = json.get("version") {
        if let Some(s) = v.get("app").and_then(Value::as_str) {
            info.app_version = s.to_string();
        }
        if let Some(s) = v.get("firmware").and_then(Value::as_str) {
            info.firmware = s.to_string();
        }
        if let Some(s) = v.get("hardware").and_then(Value::as_str) {
            info.hardware = s.to_string();
        }
    }
    if let Some(arr) = json.get("attribute").and_then(Value::as_array) {
        info.attributes = arr
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
    }
    info
}

fn build_display_settings(ds: &DisplaySettings) -> Value {
    json!({
        "position": ds.position,
        "color": ds.color,
        "align": ds.align,
        "filter": {
            "value": ds.filter_value,
            "opacity": ds.filter_opacity,
        },
        "badges": ds.badges,
    })
}

fn build_screen_config_payload(config: &ScreenConfig) -> Value {
    let mut cfg = serde_json::Map::new();

    if config.preset_id.is_empty() {
        cfg.insert("Type".into(), json!("Custom"));
        cfg.insert("id".into(), json!("Customization"));
        cfg.insert("media".into(), json!(config.media));
    } else {
        cfg.insert("Type".into(), json!("Pre-set"));
        cfg.insert("id".into(), json!(config.preset_id));
    }
    cfg.insert("screenMode".into(), json!(config.screen_mode));
    cfg.insert("ratio".into(), json!(config.ratio));
    cfg.insert("playMode".into(), json!(config.play_mode));

    if config.screen_mode == "Screen Splitting" {
        cfg.insert(
            "settings".into(),
            json!([
                build_display_settings(&config.settings),
                build_display_settings(&config.settings2),
            ]),
        );
        cfg.insert(
            "sysinfoDisplay".into(),
            json!([config.sysinfo_display, config.sysinfo_display2]),
        );
    } else {
        cfg.insert("settings".into(), build_display_settings(&config.settings));
        cfg.insert("sysinfoDisplay".into(), json!(config.sysinfo_display));
    }

    Value::Object(cfg)
}

fn build_full_config_payload(
    config: &ScreenConfig,
    cpu_name: &str,
    gpu_name: &str,
    brightness: i32,
    temp_unit: &str,
) -> Value {
    let mut screen_cfg = serde_json::Map::new();
    screen_cfg.insert(
        "Type".into(),
        json!(if config.preset_id.is_empty() {
            "Custom"
        } else {
            "Pre-set"
        }),
    );
    screen_cfg.insert(
        "id".into(),
        json!(if config.preset_id.is_empty() {
            "Customization"
        } else {
            config.preset_id.as_str()
        }),
    );
    screen_cfg.insert("screenMode".into(), json!(config.screen_mode));
    screen_cfg.insert("ratio".into(), json!(config.ratio));
    screen_cfg.insert("playMode".into(), json!(config.play_mode));
    screen_cfg.insert("media".into(), json!(config.media));

    if config.screen_mode == "Screen Splitting" {
        screen_cfg.insert(
            "settings".into(),
            json!([
                build_display_settings(&config.settings),
                build_display_settings(&config.settings2),
            ]),
        );
        screen_cfg.insert(
            "sysinfoDisplay".into(),
            json!([config.sysinfo_display, config.sysinfo_display2]),
        );
    } else {
        screen_cfg.insert("settings".into(), build_display_settings(&config.settings));
        screen_cfg.insert("sysinfoDisplay".into(), json!(config.sysinfo_display));
    }

    json!({
        "temperature": temp_unit,
        "waterBlockScreen": {
            "enable": true,
            "displayInSleep": config.display_in_sleep,
            "brightness": brightness,
            "waterfallMode": config.waterfall_mode,
            "id": Value::Object(screen_cfg),
        },
        "spec": { "cpu": cpu_name, "gpu": gpu_name },
    })
}

fn build_fan_lcd_payload(percent: i32) -> Value {
    let clamped = percent.clamp(0, 100);
    // Default smart-mode curve captured from Kanali — 8 [temp, fan%] points.
    let smart_mode = json!([
        [0, 10],
        [28, 10],
        [48, 10],
        [61, 10],
        [75, 10],
        [77, 68],
        [79, 100],
        [100, 100],
    ]);
    json!({
        "mode": "Fixed Mode",
        "smartMode": smart_mode,
        "fixedMode": clamped,
    })
}

fn build_sysinfo_payload(data: &[SysinfoData], timestamp_ms: i64) -> Value {
    let mut cpu = json!({
        "load": 0.0, "temperature": 0.0, "speedAverage": 0.0,
        "voltage": 0.0, "power": 0.0, "fanAverage": 0.0,
    });
    let mut gpu = json!({
        "load": 0.0, "temperature": 0.0, "speed": 0.0,
        "voltage": 0.0, "power": 0.0, "fan": 0.0,
    });
    let mut memory = json!({
        "load": 0.0, "speed": 0.0, "temperature": 0.0,
        "total": 0.0, "used": 0.0,
    });
    let mut motherboard = json!({ "temperature": 0.0 });
    let mut disk = json!({
        "load": 0.0, "used": 0.0, "total": 0.0, "temperature": 0.0,
        "activity": 0.0, "readSpeed": 0.0, "writeSpeed": 0.0,
    });
    let network = json!({ "download": 0.0, "upload": 0.0 });

    for item in data {
        let parsed: f64 = item.value.parse().unwrap_or(0.0);
        let rounded = parsed.round();
        apply_sysinfo_metric(
            &item.label,
            rounded,
            &mut cpu,
            &mut gpu,
            &mut memory,
            &mut motherboard,
            &mut disk,
        );
    }

    json!({
        "cpu": cpu,
        "gpu": gpu,
        "memory": memory,
        "motherboard": motherboard,
        "disk": disk,
        "network": network,
        "fans": [],
        "timestamp": timestamp_ms,
    })
}

fn apply_sysinfo_metric(
    label: &str,
    value: f64,
    cpu: &mut Value,
    gpu: &mut Value,
    memory: &mut Value,
    motherboard: &mut Value,
    disk: &mut Value,
) {
    let v = json!(value);
    match label {
        "CPU Temperature" => cpu["temperature"] = v,
        "CPU Frequency" => cpu["speedAverage"] = v,
        "CPU Usage" => cpu["load"] = v,
        "CPU Voltage" => cpu["voltage"] = v,
        "GPU Temperature" => gpu["temperature"] = v,
        "GPU Frequency" => gpu["speed"] = v,
        "GPU Usage" => gpu["load"] = v,
        "GPU Voltage" => gpu["voltage"] = v,
        "Hard Disk Temperature" => disk["temperature"] = v,
        "Motherboard Temperature" => motherboard["temperature"] = v,
        "Memory Frequency" => memory["speed"] = v,
        "Memory Utilization" => memory["load"] = v,
        _ => {} // unknown label, silently ignored
    }
}

/// Coerce a JSON value into a non-negative RPM reading. Accepts unsigned
/// integers, finite non-negative floats (rounded), and numeric strings.
/// Anything else returns `None`. The protocol docs don't pin the wire type
/// for this field, so we accept both number and string.
fn parse_rpm_field(value: &Value) -> Option<u32> {
    if let Some(n) = value.as_u64() {
        return u32::try_from(n).ok();
    }
    if let Some(n) = value.as_f64() {
        if n.is_finite() && n >= 0.0 {
            return Some(n.round() as u32);
        }
    }
    if let Some(s) = value.as_str() {
        return s.trim().parse::<u32>().ok();
    }
    None
}

/// Extract fan and pump RPM from a `STATE all` response. Missing fields
/// (or a missing `status` object) yield `None` rather than an error —
/// firmware variants across the Panorama family may omit one or both.
pub fn parse_fan_status(response: &Response) -> FanStatus {
    let Some(json) = response.json.as_ref() else {
        return FanStatus::default();
    };
    let Some(status) = json.get("status") else {
        return FanStatus::default();
    };
    FanStatus {
        fan_lcd_rpm: status.get("fanLCD").and_then(parse_rpm_field),
        turbo_pump_rpm: status.get("turboPump").and_then(parse_rpm_field),
    }
}

/// Poll [`Device::read_fan_status`] every `interval`, delivering each reading
/// to `on_status` until `running` clears. The caller picks the thread — the
/// CLI runs this on the foreground thread under a `ctrlc` handler; the
/// future GUI is expected to call it from a worker thread that owns the
/// [`Device`], with `on_status` forwarding into a channel the UI drains.
///
/// Errors from [`Device::read_fan_status`] abort the loop, except when
/// `running` was cleared during the failing call — those are suppressed so
/// shutdown is quiet. The loop wakes within ~250 ms of `running` clearing.
pub fn poll_fan_status<F>(
    device: &mut Device,
    interval: Duration,
    running: &AtomicBool,
    mut on_status: F,
) -> Result<(), DeviceError>
where
    F: FnMut(FanStatus),
{
    while running.load(Ordering::SeqCst) {
        match device.read_fan_status() {
            Ok(status) => on_status(status),
            Err(e) => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                return Err(e);
            }
        }
        poll_sleep(interval, running);
    }
    Ok(())
}

/// Sleep up to `total`, waking within ~250 ms once `running` clears. Mirrors
/// the daemon's `interruptible_sleep` but kept private here so polling
/// doesn't take a hidden dependency on `panorama-ctl`.
fn poll_sleep(total: Duration, running: &AtomicBool) {
    let slice = Duration::from_millis(250);
    let mut elapsed = Duration::ZERO;
    while elapsed < total && running.load(Ordering::SeqCst) {
        let nap = slice.min(total - elapsed);
        std::thread::sleep(nap);
        elapsed += nap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_utc_offset_is_within_plausible_range() {
        // Real timezones span roughly UTC-12 to UTC+14.
        let offset = local_utc_offset_ms();
        let twelve_hours_ms = 12 * 3600 * 1000;
        let fourteen_hours_ms = 14 * 3600 * 1000;
        assert!(
            offset >= -twelve_hours_ms && offset <= fourteen_hours_ms,
            "offset {} ms is outside plausible timezone range",
            offset
        );
    }

    #[test]
    fn parse_device_info_uses_unknown_for_missing_fields() {
        let info = parse_device_info(&json!({}));
        assert_eq!(info.product_id, "unknown");
        assert_eq!(info.serial, "unknown");
        assert_eq!(info.firmware, "unknown");
        assert!(info.attributes.is_empty());
    }

    #[test]
    fn parse_device_info_extracts_all_fields() {
        let info = parse_device_info(&json!({
            "productId": "Panorama 360",
            "OS": "rk3568",
            "sn": "REDACTED_SERIAL",
            "version": {
                "app": "1.2.3",
                "firmware": "4.5.6",
                "hardware": "rev_a",
            },
            "attribute": ["audio", "adb"],
        }));
        assert_eq!(info.product_id, "Panorama 360");
        assert_eq!(info.os, "rk3568");
        assert_eq!(info.serial, "REDACTED_SERIAL");
        assert_eq!(info.app_version, "1.2.3");
        assert_eq!(info.firmware, "4.5.6");
        assert_eq!(info.hardware, "rev_a");
        assert_eq!(info.attributes, vec!["audio", "adb"]);
    }

    #[test]
    fn parse_device_info_ignores_non_string_attribute_entries() {
        let info = parse_device_info(&json!({
            "attribute": ["audio", 42, true, "adb"],
        }));
        assert_eq!(info.attributes, vec!["audio", "adb"]);
    }

    #[test]
    fn validate_panorama_attributes_accepts_genuine_device() {
        let info = DeviceInfo {
            product_id: "cm01".into(),
            os: "Android".into(),
            serial: "TEST_SERIAL".into(),
            app_version: "1.4".into(),
            firmware: "V1.0.11".into(),
            hardware: "V1.1".into(),
            attributes: vec![
                "Status".into(),
                "Water Block Screen".into(),
                "Fan LCD|rw".into(),
                "Turbo Pump".into(),
            ],
        };
        assert!(validate_panorama_attributes(&info).is_ok());
    }

    #[test]
    fn validate_panorama_attributes_rejects_generic_android_device() {
        let info = DeviceInfo {
            product_id: "cm01".into(),
            os: "Android".into(),
            serial: "GENERIC_DEVICE".into(),
            app_version: "1.0".into(),
            firmware: "1.0".into(),
            hardware: "1.0".into(),
            attributes: vec!["audio".into(), "adb".into()],
        };
        let result = validate_panorama_attributes(&info);
        assert!(result.is_err());
        assert!(matches!(result, Err(DeviceError::NotPanorama { .. })));
    }

    #[test]
    fn validate_panorama_attributes_requires_at_least_two_matches() {
        // Only one Panorama attribute — should fail
        let info = DeviceInfo {
            product_id: "cm01".into(),
            os: "Android".into(),
            serial: "PARTIAL_MATCH".into(),
            app_version: "1.0".into(),
            firmware: "1.0".into(),
            hardware: "1.0".into(),
            attributes: vec!["Status".into(), "audio".into(), "adb".into()],
        };
        assert!(validate_panorama_attributes(&info).is_err());

        // Two Panorama attributes — should pass
        let info = DeviceInfo {
            product_id: "cm01".into(),
            os: "Android".into(),
            serial: "VALID_MATCH".into(),
            app_version: "1.0".into(),
            firmware: "1.0".into(),
            hardware: "1.0".into(),
            attributes: vec!["Status".into(), "Turbo Pump".into(), "other".into()],
        };
        assert!(validate_panorama_attributes(&info).is_ok());
    }

    #[test]
    fn display_settings_payload_has_expected_keys() {
        let ds = DisplaySettings {
            position: "Top".into(),
            color: "#FFFFFF".into(),
            align: "Center".into(),
            badges: vec!["CPU Badge".into()],
            filter_value: "Smoke".into(),
            filter_opacity: 50,
        };
        let v = build_display_settings(&ds);
        assert_eq!(v["position"], "Top");
        assert_eq!(v["color"], "#FFFFFF");
        assert_eq!(v["align"], "Center");
        assert_eq!(v["filter"]["value"], "Smoke");
        assert_eq!(v["filter"]["opacity"], 50);
        assert_eq!(v["badges"][0], "CPU Badge");
    }

    #[test]
    fn screen_config_custom_mode_includes_media_array() {
        let cfg = ScreenConfig {
            preset_id: String::new(),
            media: vec!["clip.mp4".into()],
            ..Default::default()
        };
        let v = build_screen_config_payload(&cfg);
        assert_eq!(v["Type"], "Custom");
        assert_eq!(v["id"], "Customization");
        assert_eq!(v["media"][0], "clip.mp4");
    }

    #[test]
    fn screen_config_preset_mode_omits_media_array() {
        let cfg = ScreenConfig {
            preset_id: "Pre-set 1: Cooling delivery".into(),
            ..Default::default()
        };
        let v = build_screen_config_payload(&cfg);
        assert_eq!(v["Type"], "Pre-set");
        assert_eq!(v["id"], "Pre-set 1: Cooling delivery");
        assert!(
            v.get("media").is_none(),
            "preset mode shouldn't carry media"
        );
    }

    #[test]
    fn screen_config_split_mode_uses_paired_settings_and_sysinfo() {
        let cfg = ScreenConfig {
            screen_mode: "Screen Splitting".into(),
            settings: DisplaySettings {
                position: "Top".into(),
                filter_value: "Smoke".into(),
                filter_opacity: 80,
                ..Default::default()
            },
            settings2: DisplaySettings {
                position: "Bottom".into(),
                filter_value: "Rain".into(),
                filter_opacity: 80,
                ..Default::default()
            },
            sysinfo_display: vec!["CPU Temperature".into()],
            sysinfo_display2: vec!["GPU Temperature".into()],
            ..Default::default()
        };
        let v = build_screen_config_payload(&cfg);
        assert_eq!(v["settings"][0]["position"], "Top");
        assert_eq!(v["settings"][1]["position"], "Bottom");
        assert_eq!(v["settings"][0]["filter"]["value"], "Smoke");
        assert_eq!(v["settings"][1]["filter"]["value"], "Rain");
        assert_eq!(v["settings"][0]["filter"]["opacity"], 80);
        assert_eq!(v["settings"][1]["filter"]["opacity"], 80);
        assert_eq!(v["sysinfoDisplay"][0][0], "CPU Temperature");
        assert_eq!(v["sysinfoDisplay"][1][0], "GPU Temperature");
    }

    #[test]
    fn full_config_nests_screen_under_water_block_screen_id() {
        let cfg = ScreenConfig {
            preset_id: "Pre-set 1: Cooling delivery".into(),
            ..Default::default()
        };
        let v = build_full_config_payload(&cfg, "Ryzen 9", "RTX 4090", 80, "Celsius");
        assert_eq!(v["temperature"], "Celsius");
        assert_eq!(v["waterBlockScreen"]["brightness"], 80);
        assert_eq!(v["waterBlockScreen"]["enable"], true);
        assert_eq!(v["waterBlockScreen"]["displayInSleep"], false);
        assert_eq!(v["waterBlockScreen"]["id"]["Type"], "Pre-set");
        assert_eq!(v["spec"]["cpu"], "Ryzen 9");
        assert_eq!(v["spec"]["gpu"], "RTX 4090");
    }

    #[test]
    fn full_config_carries_display_in_sleep() {
        let cfg = ScreenConfig {
            display_in_sleep: true,
            ..Default::default()
        };
        let v = build_full_config_payload(&cfg, "cpu", "gpu", 80, "Celsius");
        assert_eq!(v["waterBlockScreen"]["displayInSleep"], true);
    }

    #[test]
    fn fan_lcd_payload_clamps_to_0_100() {
        let low = build_fan_lcd_payload(-50);
        assert_eq!(low["fixedMode"], 0);

        let high = build_fan_lcd_payload(150);
        assert_eq!(high["fixedMode"], 100);

        let mid = build_fan_lcd_payload(42);
        assert_eq!(mid["fixedMode"], 42);
    }

    #[test]
    fn fan_lcd_payload_includes_default_smart_mode_curve() {
        let v = build_fan_lcd_payload(50);
        assert_eq!(v["mode"], "Fixed Mode");
        let curve = v["smartMode"].as_array().expect("smartMode is an array");
        assert_eq!(curve.len(), 8, "default curve has 8 points");
        assert_eq!(curve[0][0], 0);
        assert_eq!(curve[7][0], 100);
    }

    #[test]
    fn sysinfo_payload_applies_known_labels_to_correct_fields() {
        let data = vec![
            SysinfoData {
                label: "CPU Temperature".into(),
                value: "65.4".into(),
                unit: "°C".into(),
            },
            SysinfoData {
                label: "GPU Usage".into(),
                value: "78".into(),
                unit: "%".into(),
            },
            SysinfoData {
                label: "Memory Utilization".into(),
                value: "42".into(),
                unit: "%".into(),
            },
        ];
        let v = build_sysinfo_payload(&data, 1_700_000_000_000);
        // 65.4 rounds to 65 (half-away-from-zero).
        assert_eq!(v["cpu"]["temperature"], 65.0);
        assert_eq!(v["gpu"]["load"], 78.0);
        assert_eq!(v["memory"]["load"], 42.0);
        assert_eq!(v["timestamp"], 1_700_000_000_000i64);
        // Untouched defaults remain zeroed.
        assert_eq!(v["cpu"]["load"], 0.0);
        assert_eq!(v["fans"], json!([]));
    }

    #[test]
    fn sysinfo_payload_silently_skips_unknown_labels() {
        let data = vec![SysinfoData {
            label: "Made-Up Sensor".into(),
            value: "999".into(),
            unit: "x".into(),
        }];
        let v = build_sysinfo_payload(&data, 0);
        assert_eq!(v["cpu"]["temperature"], 0.0);
    }

    #[test]
    fn sysinfo_payload_treats_invalid_numeric_strings_as_zero() {
        let data = vec![SysinfoData {
            label: "CPU Temperature".into(),
            value: "not a number".into(),
            unit: "°C".into(),
        }];
        let v = build_sysinfo_payload(&data, 0);
        assert_eq!(v["cpu"]["temperature"], 0.0);
    }

    fn response_with_json(value: Value) -> Response {
        Response {
            json: Some(value),
            ..Response::default()
        }
    }

    #[test]
    fn parse_fan_status_extracts_both_numeric_fields() {
        let resp = response_with_json(json!({
            "status": { "fanLCD": 1450, "turboPump": 2700 }
        }));
        let fs = parse_fan_status(&resp);
        assert_eq!(fs.fan_lcd_rpm, Some(1450));
        assert_eq!(fs.turbo_pump_rpm, Some(2700));
    }

    #[test]
    fn parse_fan_status_accepts_numeric_strings() {
        let resp = response_with_json(json!({
            "status": { "fanLCD": "1450", "turboPump": "2700" }
        }));
        let fs = parse_fan_status(&resp);
        assert_eq!(fs.fan_lcd_rpm, Some(1450));
        assert_eq!(fs.turbo_pump_rpm, Some(2700));
    }

    #[test]
    fn parse_fan_status_missing_fields_become_none() {
        let resp = response_with_json(json!({ "status": { "fanLCD": 1450 } }));
        let fs = parse_fan_status(&resp);
        assert_eq!(fs.fan_lcd_rpm, Some(1450));
        assert_eq!(fs.turbo_pump_rpm, None);
    }

    #[test]
    fn parse_fan_status_missing_status_object_yields_defaults() {
        let resp = response_with_json(json!({ "other": "value" }));
        assert_eq!(parse_fan_status(&resp), FanStatus::default());
    }

    #[test]
    fn parse_fan_status_no_json_yields_defaults() {
        let resp = Response::default();
        assert_eq!(parse_fan_status(&resp), FanStatus::default());
    }

    #[test]
    fn parse_fan_status_drops_malformed_and_negative_values() {
        let resp = response_with_json(json!({
            "status": { "fanLCD": "garbage", "turboPump": -5 }
        }));
        let fs = parse_fan_status(&resp);
        assert_eq!(fs.fan_lcd_rpm, None);
        assert_eq!(fs.turbo_pump_rpm, None);
    }

    #[test]
    fn parse_fan_status_rounds_float_values() {
        let resp = response_with_json(json!({
            "status": { "fanLCD": 1449.6, "turboPump": 2700.4 }
        }));
        let fs = parse_fan_status(&resp);
        assert_eq!(fs.fan_lcd_rpm, Some(1450));
        assert_eq!(fs.turbo_pump_rpm, Some(2700));
    }

    #[test]
    fn poll_fan_status_returns_immediately_when_running_already_false() {
        let mut device = Device::new();
        let running = AtomicBool::new(false);
        let mut received: Vec<FanStatus> = Vec::new();
        let result = poll_fan_status(&mut device, Duration::from_secs(60), &running, |s| {
            received.push(s)
        });
        assert!(result.is_ok());
        assert!(
            received.is_empty(),
            "no read should have happened with running=false on entry"
        );
    }
}
