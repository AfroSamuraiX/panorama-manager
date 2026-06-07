//! `adb` CLI wrapper for talking to the device's Android filesystem.
//!
//! Spawns the system `adb` binary via `std::process::Command`. Arguments are
//! passed directly as `argv[]`, so no shell escaping is needed.
//!
//! Discovery identifies the Panorama among other connected adb devices by
//! the `product:cm01` field reported in `adb devices -l` — `cm01` is the
//! Rockchip SoC model code TRYX uses for the Panorama display board.

use std::process::Command;
use std::sync::Mutex;

const MEDIA_PATH: &str = "/sdcard/pcMedia";
const PANORAMA_PRODUCT_TAG: &str = "product:cm01";
const PANORAMA_KEYWORDS: &[&str] = &["cm01", "tryx", "panorama-mgr"];

/// Outcome of [`Adb::validate_device`] — distinguishes "no device" from
/// "wrong device" so callers can produce useful error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceValidation {
    /// Device confirmed as a Panorama; the variant tags which evidence proved it.
    Confirmed(ConfirmationKind),
    /// adb is reachable but no device is in `device` state.
    NotConnected,
    /// adb sees a connected device, but it doesn't look like a Panorama.
    NotPanorama { detected: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationKind {
    /// `/sdcard/pcMedia/` exists on the device's filesystem.
    MediaPathPresent,
    /// `ro.product.{model,manufacturer,device}` contains a Panorama keyword.
    ProductInfoMatched,
}

/// Outcome of [`Adb::diagnose`] — `doctor`-grade detail about the adb side so
/// the caller can produce a targeted suggestion instead of unconditionally
/// blaming udev. Covers every state `adb devices -l` is observed to report
/// plus the `adb`-binary-itself failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbDiagnosis {
    /// `adb` is not in `PATH`.
    NotInstalled,
    /// `adb devices` invocation failed (binary present but the call errored,
    /// or the server reported failure). Carries the stderr / OS error.
    ServerUnreachable(String),
    /// adb is reachable but lists no devices at all.
    NoDevicesListed,
    /// adb sees devices, but none of them are a `cm01` Panorama.
    NonPanoramaOnly { detected: String },
    /// `cm01` is present but in `offline` state — almost always a stale server.
    /// Any non-`device`, non-`unauthorized` state (e.g. `no permissions`) is
    /// also routed here since the user-facing fix is the same.
    PanoramaOffline { state: String },
    /// `cm01` is present in `unauthorized` state. Rare for a cooler (no UI
    /// to authorize on-device), usually also a stale-server symptom.
    PanoramaUnauthorized,
    /// `cm01` is present and in `device` state — the happy path. Carries the
    /// serial so callers can log / display it.
    PanoramaReady { serial: String },
}

pub struct Adb {
    cached_serial: Mutex<String>,
}

struct AdbOutput {
    success: bool,
    text: String,
}

impl Adb {
    pub fn new() -> Self {
        Self {
            cached_serial: Mutex::new(String::new()),
        }
    }

    pub fn is_device_connected(&self) -> bool {
        find_panorama_serial_via_adb().is_some()
    }

    /// Run `adb devices -l` and classify the result for `pctl doctor`.
    ///
    /// Distinguishes "adb is broken", "server lists nothing", "wrong device
    /// connected", and the three cm01 states (`device`, `offline`,
    /// `unauthorized`). Callers use this to give targeted advice — the bare
    /// [`Self::is_device_connected`] boolean is fine for non-diagnostic paths.
    pub fn diagnose(&self) -> AdbDiagnosis {
        let output = match Command::new("adb").args(["devices", "-l"]).output() {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return AdbDiagnosis::NotInstalled;
            }
            Err(e) => return AdbDiagnosis::ServerUnreachable(e.to_string()),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("adb devices exited with status {}", output.status)
            } else {
                stderr
            };
            return AdbDiagnosis::ServerUnreachable(msg);
        }
        let mut combined = output.stdout;
        combined.extend_from_slice(&output.stderr);
        diagnose_from_devices_output(&String::from_utf8_lossy(&combined))
    }

    /// Confirm the connected device is actually a Panorama.
    ///
    /// Runs two checks in order, returning [`DeviceValidation::Confirmed`] on
    /// the first one that passes:
    ///
    /// 1. Filesystem evidence — `/sdcard/pcMedia/` exists. Strongest signal,
    ///    but absent on fresh devices that have never had media pushed.
    /// 2. Product info — `ro.product.{model,manufacturer,device}` contains a
    ///    Panorama keyword. Works on fresh devices since these properties are
    ///    baked into the firmware.
    pub fn validate_device(&self) -> DeviceValidation {
        if !self.is_device_connected() {
            return DeviceValidation::NotConnected;
        }

        if self.media_path_exists() {
            return DeviceValidation::Confirmed(ConfirmationKind::MediaPathPresent);
        }

        let info = self.read_product_info();
        if product_info_matches_panorama(&info) {
            return DeviceValidation::Confirmed(ConfirmationKind::ProductInfoMatched);
        }

        DeviceValidation::NotPanorama { detected: info }
    }

    fn media_path_exists(&self) -> bool {
        self.run(&["shell", "test", "-d", MEDIA_PATH])
            .map(|o| o.success)
            .unwrap_or(false)
    }

    fn read_product_info(&self) -> String {
        let model = self.getprop("ro.product.model").unwrap_or_default();
        let manufacturer = self.getprop("ro.product.manufacturer").unwrap_or_default();
        let device = self.getprop("ro.product.device").unwrap_or_default();
        format!("{model} {manufacturer} {device}")
    }

    fn getprop(&self, key: &str) -> Option<String> {
        let out = self.run(&["shell", "getprop", key])?;
        if !out.success {
            return None;
        }
        Some(out.text.trim().to_string())
    }

    pub fn push(&self, local_path: &str, remote_name: &str) -> bool {
        if !is_safe_media_filename(remote_name) {
            return false;
        }
        // `adb push` does not run a shell — it passes the remote path as an
        // argv token to the on-device adb daemon — so we do not need shell
        // quoting here, only the filename-safety check above.
        let dest = media_path(remote_name);
        self.run(&["push", local_path, &dest])
            .map(|o| o.success)
            .unwrap_or(false)
    }

    pub fn pull(&self, remote_name: &str, local_path: &str) -> bool {
        if !is_safe_media_filename(remote_name) {
            return false;
        }
        let source = media_path(remote_name);
        self.run(&["pull", &source, local_path])
            .map(|o| o.success)
            .unwrap_or(false)
    }

    pub fn list_media(&self) -> Option<Vec<String>> {
        let out = self.run(&["shell", "ls", "-1", MEDIA_PATH])?;
        if !out.success {
            return None;
        }
        Some(parse_ls_output(&out.text))
    }

    pub fn file_exists(&self, filename: &str) -> bool {
        let Some(target) = quoted_media_path(filename) else {
            return false;
        };
        self.run(&["shell", "ls", &target])
            .map(|o| o.success)
            .unwrap_or(false)
    }

    pub fn remove(&self, filename: &str) -> bool {
        let Some(target) = quoted_media_path(filename) else {
            return false;
        };
        self.run(&["shell", "rm", &target])
            .map(|o| o.success)
            .unwrap_or(false)
    }

    fn run(&self, args: &[&str]) -> Option<AdbOutput> {
        let serial = self.ensure_serial();
        let mut cmd = Command::new("adb");
        let skip_serial = args.first().map(|a| *a == "devices").unwrap_or(true);
        if !serial.is_empty() && !skip_serial {
            cmd.arg("-s").arg(&serial);
        }
        cmd.args(args);

        let output = cmd.output().ok()?;
        let success = output.status.success();
        let mut combined = output.stdout;
        combined.extend_from_slice(&output.stderr);
        Some(AdbOutput {
            success,
            text: String::from_utf8_lossy(&combined).into_owned(),
        })
    }

    fn ensure_serial(&self) -> String {
        let mut guard = self.cached_serial.lock().expect("mutex poisoned");
        if guard.is_empty() {
            if let Some(serial) = find_panorama_serial_via_adb() {
                *guard = serial;
            }
        }
        guard.clone()
    }
}

impl Default for Adb {
    fn default() -> Self {
        Self::new()
    }
}

fn find_panorama_serial_via_adb() -> Option<String> {
    let output = Command::new("adb").args(["devices", "-l"]).output().ok()?;
    let mut text = output.stdout;
    text.extend_from_slice(&output.stderr);
    find_panorama_serial(&String::from_utf8_lossy(&text))
}

/// Parse `adb devices -l` output and return the serial of a connected
/// Panorama (product `cm01`, state `device`), if present.
///
/// `adb devices -l` formats output as space-padded columns, not tab-separated
/// (unlike plain `adb devices`). Whitespace splitting handles both shapes.
fn find_panorama_serial(text: &str) -> Option<String> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        let state = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        if state == "device" && line.contains(PANORAMA_PRODUCT_TAG) {
            return Some(serial.to_string());
        }
    }
    None
}

/// Classify `adb devices -l` output. Pure-string; unit-testable without
/// spawning a subprocess. The header line, blank lines, and adb's `* daemon`
/// status messages are skipped; only well-formed `<serial> <state> ...` lines
/// are counted as devices.
fn diagnose_from_devices_output(text: &str) -> AdbDiagnosis {
    let mut panorama: Option<(String, String)> = None; // (serial, state)
    let mut non_panorama: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with('*') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        let state_first = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        // `adb devices -l` reports most states as a single token (device,
        // offline, unauthorized, ...) but Linux's "no permissions" is two
        // tokens. Rejoin that one case so the user-facing message reads
        // naturally.
        let state = if state_first == "no" && parts.clone().next() == Some("permissions") {
            "no permissions".to_string()
        } else {
            state_first.to_string()
        };
        if line.contains(PANORAMA_PRODUCT_TAG) {
            panorama = Some((serial.to_string(), state));
        } else {
            non_panorama.push(format!("{serial} ({state})"));
        }
    }

    match panorama {
        Some((serial, state)) => match state.as_str() {
            "device" => AdbDiagnosis::PanoramaReady { serial },
            "unauthorized" => AdbDiagnosis::PanoramaUnauthorized,
            // `offline`, `no`/`permissions`, `bootloader`, anything else —
            // user-facing fix (kill-server + retry) is the same.
            other => AdbDiagnosis::PanoramaOffline {
                state: other.to_string(),
            },
        },
        None if non_panorama.is_empty() => AdbDiagnosis::NoDevicesListed,
        None => AdbDiagnosis::NonPanoramaOnly {
            detected: non_panorama.join(", "),
        },
    }
}

fn product_info_matches_panorama(info: &str) -> bool {
    let lower = info.to_lowercase();
    PANORAMA_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Validate a media filename for use as a single component under the device
/// media directory. Returns false for anything that could escape the media
/// directory or otherwise misbehave when joined. Backed by
/// [`std::path::Path`]'s component classification rather than substring
/// matching: a safe filename parses to exactly one [`std::path::Component::Normal`]
/// — so `..`, `.`, absolute paths, embedded separators (`a/b`), and the
/// empty string all fail by construction. NUL bytes (also illegal in any
/// real Unix path) are rejected explicitly.
pub fn is_safe_media_filename(filename: &str) -> bool {
    if filename.is_empty() || filename.contains('\0') {
        return false;
    }
    let path = std::path::Path::new(filename);
    let mut components = path.components();
    let Some(first) = components.next() else {
        return false;
    };
    if components.next().is_some() {
        return false; // multi-component → contains separator
    }
    matches!(first, std::path::Component::Normal(_))
}

fn media_path(filename: &str) -> String {
    std::path::Path::new(MEDIA_PATH)
        .join(filename)
        .to_string_lossy()
        .into_owned()
}

/// Build a validated device media path and shell-quote the result for safe embedding in an `adb shell <cmd>`
/// argv. Returns `None` if the filename fails the safety check or shlex
/// refuses to quote the value (e.g. embedded NUL). Callers that get
/// `None` should treat it as "filename rejected, nothing happens".
fn quoted_media_path(filename: &str) -> Option<String> {
    if !is_safe_media_filename(filename) {
        return None;
    }
    let target = media_path(filename);
    shlex::try_quote(&target).ok().map(|s| s.into_owned())
}

fn parse_ls_output(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches([' ', '\t', '\r', '\n']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures below mirror the real `adb devices -l` output verified on
    // hardware (2026-05-14): space-padded columns, not tab-separated.

    #[test]
    fn find_panorama_locates_cm01_device() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01     device usb:1-9 product:cm01 model:cm01 device:cm01 transport_id:1\n";
        assert_eq!(find_panorama_serial(text), Some("PANORAMA_TEST_01".into()));
    }

    #[test]
    fn find_panorama_handles_tab_separator_too() {
        // Legacy `adb devices` (no -l) uses tabs; whitespace parsing means
        // we still cope even though we now always pass -l.
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01\tdevice usb:1-9 product:cm01 model:cm01 device:cm01 transport_id:1\n";
        assert_eq!(find_panorama_serial(text), Some("PANORAMA_TEST_01".into()));
    }

    #[test]
    fn find_panorama_returns_none_when_no_cm01_device() {
        let text = "List of devices attached\n\
                    A1B2C3D4       device usb:1-1 product:Pixel_7 model:Pixel_7 device:cheetah transport_id:1\n";
        assert_eq!(find_panorama_serial(text), None);
    }

    #[test]
    fn find_panorama_returns_none_when_cm01_is_unauthorized() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01       unauthorized usb:1-9 product:cm01 model:cm01\n";
        assert_eq!(find_panorama_serial(text), None);
    }

    #[test]
    fn find_panorama_returns_none_when_cm01_is_offline() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01       offline usb:1-9 product:cm01 model:cm01\n";
        assert_eq!(find_panorama_serial(text), None);
    }

    #[test]
    fn find_panorama_picks_cm01_among_multiple_devices() {
        let text = "List of devices attached\n\
                    PixelPhone     device usb:1-1 product:Pixel_7 model:Pixel_7 device:cheetah\n\
                    PANORAMA_TEST_02         device usb:1-9 product:cm01 model:cm01 device:cm01\n";
        assert_eq!(find_panorama_serial(text), Some("PANORAMA_TEST_02".into()));
    }

    #[test]
    fn find_panorama_returns_none_for_empty_device_list() {
        let text = "List of devices attached\n\n";
        assert_eq!(find_panorama_serial(text), None);
    }

    #[test]
    fn product_info_matches_on_cm01() {
        // Matches the live values JR reported: "cm01 rockchip cm01".
        assert!(product_info_matches_panorama("cm01 rockchip cm01"));
    }

    #[test]
    fn product_info_matches_case_insensitively() {
        assert!(product_info_matches_panorama("CM01 ROCKCHIP CM01"));
        assert!(product_info_matches_panorama("TRYX Panorama 360"));
        assert!(product_info_matches_panorama("tryx-panorama-mgr-se"));
    }

    #[test]
    fn product_info_rejects_random_android_device() {
        assert!(!product_info_matches_panorama("Pixel_7 Google cheetah"));
        assert!(!product_info_matches_panorama("SM-G998U samsung q2q"));
    }

    #[test]
    fn product_info_rejects_empty_string() {
        assert!(!product_info_matches_panorama(""));
        assert!(!product_info_matches_panorama("   "));
    }

    #[test]
    fn ls_parses_one_filename_per_line() {
        let text = "video1.mp4\nimage.png\n";
        assert_eq!(parse_ls_output(text), vec!["video1.mp4", "image.png"]);
    }

    #[test]
    fn ls_trims_trailing_whitespace_and_carriage_returns() {
        let text = "video1.mp4 \r\nimage.png\t\n\n";
        assert_eq!(parse_ls_output(text), vec!["video1.mp4", "image.png"]);
    }

    #[test]
    fn ls_returns_empty_for_blank_output() {
        assert!(parse_ls_output("").is_empty());
        assert!(parse_ls_output("\n\n").is_empty());
    }

    // --- diagnose_from_devices_output ---------------------------------------

    #[test]
    fn diagnose_ready_when_cm01_in_device_state() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01     device usb:1-9 product:cm01 model:cm01 device:cm01 transport_id:1\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::PanoramaReady {
                serial: "PANORAMA_TEST_01".into()
            }
        );
    }

    #[test]
    fn diagnose_offline_when_cm01_in_offline_state() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01     offline usb:1-9 product:cm01 model:cm01\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::PanoramaOffline {
                state: "offline".into()
            }
        );
    }

    #[test]
    fn diagnose_unauthorized_when_cm01_unauthorized() {
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01     unauthorized usb:1-9 product:cm01 model:cm01\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::PanoramaUnauthorized
        );
    }

    #[test]
    fn diagnose_unknown_state_falls_through_to_offline() {
        // `no permissions` shows up on Linux when udev lets adb see the
        // device but not open it. The user-facing fix is the same as offline,
        // and the parser rejoins the two tokens so the message reads cleanly.
        let text = "List of devices attached\n\
                    PANORAMA_TEST_01     no permissions usb:1-9 product:cm01 model:cm01\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::PanoramaOffline {
                state: "no permissions".into()
            }
        );
    }

    #[test]
    fn diagnose_no_devices_listed_for_empty_output() {
        let text = "List of devices attached\n\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::NoDevicesListed
        );
    }

    #[test]
    fn diagnose_no_devices_listed_ignores_daemon_status_lines() {
        let text = "* daemon not running; starting now at tcp:5037\n\
                    * daemon started successfully\n\
                    List of devices attached\n\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::NoDevicesListed
        );
    }

    #[test]
    fn diagnose_non_panorama_only_when_other_devices_present() {
        let text = "List of devices attached\n\
                    PixelPhone     device usb:1-1 product:Pixel_7 model:Pixel_7 device:cheetah transport_id:1\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::NonPanoramaOnly {
                detected: "PixelPhone (device)".into()
            }
        );
    }

    #[test]
    fn diagnose_picks_cm01_among_multiple_devices() {
        let text = "List of devices attached\n\
                    PixelPhone     device usb:1-1 product:Pixel_7 model:Pixel_7 device:cheetah\n\
                    PANORAMA_TEST_02       device usb:1-9 product:cm01 model:cm01 device:cm01\n";
        assert_eq!(
            diagnose_from_devices_output(text),
            AdbDiagnosis::PanoramaReady {
                serial: "PANORAMA_TEST_02".into()
            }
        );
    }

    // --- is_safe_media_filename ---------------------------------------------

    #[test]
    fn safe_filename_accepts_plain_name() {
        assert!(is_safe_media_filename("foo.mp4"));
    }

    #[test]
    fn safe_filename_accepts_spaces_and_punctuation() {
        // The original bug — "Catch Me If You Can.mp4" was splitting into
        // five argv tokens at the remote shell.
        assert!(is_safe_media_filename("Catch Me If You Can.mp4"));
        assert!(is_safe_media_filename("clip (final-v2).webm"));
        assert!(is_safe_media_filename("file'with'quotes.png"));
    }

    #[test]
    fn safe_filename_rejects_parent_traversal() {
        assert!(!is_safe_media_filename(".."));
        assert!(!is_safe_media_filename("../etc/passwd"));
        assert!(!is_safe_media_filename("../../shadow"));
    }

    #[test]
    fn safe_filename_rejects_current_dir() {
        // `.` alone parses as CurDir, not Normal — rejected.
        assert!(!is_safe_media_filename("."));
    }

    #[test]
    fn safe_filename_rejects_path_separators() {
        assert!(!is_safe_media_filename("sub/clip.mp4"));
        assert!(!is_safe_media_filename("a/b/c"));
    }

    #[test]
    fn safe_filename_rejects_absolute_paths() {
        assert!(!is_safe_media_filename("/etc/passwd"));
        assert!(!is_safe_media_filename("/sdcard/pcMedia/foo.mp4"));
    }

    #[test]
    fn safe_filename_rejects_empty_and_nul() {
        assert!(!is_safe_media_filename(""));
        assert!(!is_safe_media_filename("\0"));
        assert!(!is_safe_media_filename("foo\0bar"));
    }

    // --- quoted_media_path --------------------------------------------------

    #[test]
    fn quoted_media_path_wraps_plain_filename_with_prefix() {
        // shlex's quoting rule: simple alnum + `.` needs no quoting at all;
        // shlex returns the input unchanged. The important contract is that
        // the result is safe to drop into an argv slot for `adb shell`.
        let q = quoted_media_path("foo.mp4").unwrap();
        assert_eq!(q, "/sdcard/pcMedia/foo.mp4");
    }

    #[test]
    fn quoted_media_path_quotes_spaces() {
        let q = quoted_media_path("Catch Me.mp4").unwrap();
        // shlex chooses the quoting style; both single-quoted and escaped
        // forms are acceptable. We just need to assert the result is a
        // single shell token — verify by re-parsing.
        let parsed = shlex::split(&q).unwrap();
        assert_eq!(parsed, vec!["/sdcard/pcMedia/Catch Me.mp4"]);
    }

    #[test]
    fn quoted_media_path_returns_none_for_unsafe_input() {
        assert!(quoted_media_path("..").is_none());
        assert!(quoted_media_path("a/b").is_none());
        assert!(quoted_media_path("/etc/passwd").is_none());
        assert!(quoted_media_path("").is_none());
        assert!(quoted_media_path("foo\0bar").is_none());
    }
}
