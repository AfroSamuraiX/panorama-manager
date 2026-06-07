use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "pctl")]
#[command(about = "Control utility for TRYX Panorama AIO coolers", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show device information
    Info,
    /// Validate device connectivity and filesystem structure
    Doctor,
    /// Install the udev rule that grants non-root access to the cooler.
    /// Manual/cargo installs only; package-managed installs should use the
    /// package-provided udev rule and user unit instead.
    Setup,
    /// Set screen brightness (0-100)
    Brightness { value: u8 },
    /// Set fan LCD cooling speed (0-100)
    FanLcd { value: u8 },
    /// Read current fan and pump RPM from the device
    FanRpm {
        /// Poll every N seconds instead of reading once (Ctrl-C to stop)
        #[arg(long, value_name = "SECONDS")]
        watch: Option<u64>,
    },
    /// Reboot the device
    Reboot,
    /// List media files on device
    List,
    /// Delete media file(s) from device
    Delete {
        /// Filenames or glob patterns to delete
        files: Vec<String>,
        /// Delete every media file on the device
        #[arg(long)]
        all: bool,
        /// Skip the confirmation prompt (only affects --all)
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Upload a media file to the device (converts to MP4 if needed)
    Upload {
        /// Local media file to upload
        #[arg(value_name = "LOCAL_FILE")]
        file: String,
    },
    /// Upload every media file in a directory to the device
    LibraryImport { dir: String },
    /// Show already-uploaded media on the cooler's LCD and/or set the on-screen metrics overlay
    Display {
        /// Media filename already on the cooler (optional if only changing the overlay)
        #[arg(value_name = "FILENAME")]
        file: Option<String>,
        /// Render in split-screen mode (two panes side-by-side).
        /// --media2 / --metrics2 may only be used with --split.
        #[arg(long)]
        split: bool,
        /// Pane-2 media filename already on the cooler (split mode only).
        /// Pass --media2 with no value
        /// to clear pane 2 (dark); omit it to keep whatever is saved.
        #[arg(long, num_args = 0..=1, value_name = "FILENAME")]
        media2: Option<Option<String>>,
        /// Metrics overlay: comma-separated, 0-3 (e.g. cpu-temp,gpu-temp).
        /// Pass --metrics with no value to remove the overlay; omit it
        /// entirely to leave the overlay unchanged.
        #[arg(long, value_delimiter = ',', num_args = 0..=1, value_name = "METRICS")]
        metrics: Option<Vec<String>>,
        /// Pane-2 metrics overlay (split mode only). Same syntax as --metrics.
        #[arg(long, value_delimiter = ',', num_args = 0..=1, value_name = "METRICS")]
        metrics2: Option<Vec<String>>,
        /// Overlay text color: hex (#RRGGBB) or an R,G,B triple (e.g. 255,0,0)
        #[arg(long, value_name = "COLOR")]
        metrics_color: Option<String>,
        /// Overlay horizontal alignment
        #[arg(long, value_name = "ALIGN")]
        metrics_align: Option<MetricsAlign>,
        /// Overlay vertical position
        #[arg(long, value_name = "POSITION")]
        metrics_position: Option<MetricsPosition>,
        /// Firmware display filter effect: none, smoke, or rain
        #[arg(long, value_name = "FILTER")]
        filter: Option<DisplayFilter>,
        /// Screen ratio: 2:1 (full-screen default) or 1:1 (typical for split)
        #[arg(long, value_name = "RATIO")]
        ratio: Option<ScreenRatio>,
    },
    /// Set screen sleep mode — whether the screen sleeps when the cooler is idle
    Sleep {
        /// `on` lets the screen sleep after the idle timeout; `off` keeps it on
        state: SleepState,
    },
    /// Show or update configuration values
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Run the foreground keepalive + metrics loop (intended for systemd)
    Daemon,
}

/// Subcommands for `panorama config`.
#[derive(Subcommand)]
enum ConfigAction {
    /// Show the current configuration
    Show,
    /// Update a configuration value
    Set {
        /// Config key: port, brightness, keepalive-interval, fan-lcd-percent
        key: String,
        /// New value for the key
        value: String,
    },
}

/// Screen sleep state for `panorama sleep`.
#[derive(Clone, Copy, clap::ValueEnum)]
enum SleepState {
    On,
    Off,
}

/// Horizontal alignment for the metrics overlay.
#[derive(Clone, Copy, clap::ValueEnum)]
enum MetricsAlign {
    Left,
    Center,
    Right,
}

impl MetricsAlign {
    fn as_device_str(self) -> &'static str {
        match self {
            MetricsAlign::Left => "Left",
            MetricsAlign::Center => "Center",
            MetricsAlign::Right => "Right",
        }
    }
}

/// Vertical position for the metrics overlay.
#[derive(Clone, Copy, clap::ValueEnum)]
enum MetricsPosition {
    Top,
    Center,
    Bottom,
}

/// Firmware-rendered display filter effect.
#[derive(Clone, Copy, clap::ValueEnum)]
enum DisplayFilter {
    None,
    Smoke,
    Rain,
}

impl DisplayFilter {
    fn as_device_str(self) -> &'static str {
        match self {
            DisplayFilter::None => "",
            DisplayFilter::Smoke => "Smoke",
            DisplayFilter::Rain => "Rain",
        }
    }

    fn as_user_str(self) -> &'static str {
        match self {
            DisplayFilter::None => "none",
            DisplayFilter::Smoke => "smoke",
            DisplayFilter::Rain => "rain",
        }
    }
}

impl MetricsPosition {
    fn as_device_str(self) -> &'static str {
        match self {
            MetricsPosition::Top => "Top",
            MetricsPosition::Center => "Center",
            MetricsPosition::Bottom => "Bottom",
        }
    }
}

/// Screen aspect ratio for `pctl display --ratio`. The cooler only accepts
/// `2:1` (the screen's native ratio, default for full-screen) and `1:1`
/// (typical for split, halving the 2:1 screen into two squares).
#[derive(Clone, Copy, clap::ValueEnum)]
enum ScreenRatio {
    #[value(name = "2:1")]
    TwoToOne,
    #[value(name = "1:1")]
    OneToOne,
}

impl ScreenRatio {
    fn as_device_str(self) -> &'static str {
        match self {
            ScreenRatio::TwoToOne => "2:1",
            ScreenRatio::OneToOne => "1:1",
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Info => cmd_info(),
        Commands::Doctor => cmd_doctor(),
        Commands::Setup => cmd_setup(),
        Commands::Brightness { value } => cmd_brightness(value),
        Commands::FanLcd { value } => cmd_fan_lcd(value),
        Commands::FanRpm { watch } => cmd_fan_rpm(watch),
        Commands::Reboot => cmd_reboot(),
        Commands::List => cmd_list(),
        Commands::Delete { files, all, yes } => cmd_delete(files, all, yes),
        Commands::Upload { file } => cmd_upload(file),
        Commands::LibraryImport { dir } => cmd_library_import(dir),
        Commands::Display {
            file,
            split,
            media2,
            metrics,
            metrics2,
            metrics_color,
            metrics_align,
            metrics_position,
            filter,
            ratio,
        } => cmd_display(DisplayArgs {
            file,
            split,
            media2,
            metrics,
            metrics2,
            metrics_color,
            metrics_align,
            metrics_position,
            filter,
            ratio,
        }),
        Commands::Sleep { state } => cmd_sleep(state),
        Commands::Config { action } => cmd_config(action),
        Commands::Daemon => cmd_daemon(),
    }
}

fn cmd_info() -> Result<()> {
    use panorama_core::device::Device;

    let info = if let Some(info) = try_ipc_device_info()? {
        info
    } else {
        // Connect to device (connect() also performs the handshake)
        let mut device = Device::new();
        device.connect()?
    };

    // Display device info
    println!("Device Information:");
    println!("  Product ID:  {}", info.product_id);
    println!("  OS:          {}", info.os);
    println!("  Serial:      {}", info.serial);
    println!("  App Version: {}", info.app_version);
    println!("  Firmware:    {}", info.firmware);
    println!("  Hardware:    {}", info.hardware);
    if !info.attributes.is_empty() {
        println!("  Attributes:  {}", info.attributes.join(", "));
    }

    Ok(())
}

fn cmd_doctor() -> Result<()> {
    use panorama_core::adb::{Adb, AdbDiagnosis, ConfirmationKind, DeviceValidation};
    use panorama_core::device::{Device, DeviceError};
    use panorama_core::transport::TransportError;

    println!("Running diagnostics...\n");

    // Check the serial transport (device-protocol channel)
    print!("Serial connection: ");
    let mut device = Device::new();
    let mut daemon_routed = false;
    let serial_ok = if let Some(info) = try_ipc_device_info()? {
        daemon_routed = true;
        println!("✓ Connected via daemon IPC");
        println!("  Firmware: {}", info.firmware);
        println!("  Hardware: {}", info.hardware);
        true
    } else {
        match device.connect_port() {
            Ok(_) => {
                println!("✓ Connected");
                match device.handshake() {
                    Ok(info) => {
                        println!("  Firmware: {}", info.firmware);
                        println!("  Hardware: {}", info.hardware);
                    }
                    Err(e) => {
                        println!("  ⚠ Handshake failed: {}", e);
                    }
                }
                true
            }
            Err(e) => {
                println!("✗ Failed");
                println!("  Error: {}", e);
                if matches!(&e, DeviceError::Transport(TransportError::PortBusy { .. })) {
                    println!("  Hint: the daemon likely owns the serial port, but its IPC socket was unavailable.");
                    println!("  Check the user service status or stop the daemon and retry.");
                }
                false
            }
        }
    };

    // Dump the device's STATE all response so the doctor output includes
    // live telemetry (fan/pump RPM, available storage, warnings).
    if serial_ok {
        println!("\nDevice status:");
        if daemon_routed {
            match try_ipc_state_all()? {
                Some(response) => print_device_status(&response),
                None => println!("  ⚠ Daemon IPC became unavailable before STATE all"),
            }
        } else {
            match device.send_sysinfo(&[]) {
                Ok(Some(response)) => print_device_status(&response),
                Ok(None) => println!("  ⚠ Device returned no response"),
                Err(e) => println!("  ⚠ STATE all request failed: {}", e),
            }
        }
    }

    // Check ADB (used for media push)
    print!("\nADB connection: ");
    let adb = Adb::new();
    match adb.diagnose() {
        AdbDiagnosis::PanoramaReady { serial } => {
            println!("✓ Device visible to adb (serial: {})", serial);
        }
        AdbDiagnosis::PanoramaOffline { state } => {
            println!("✗ Cooler detected by adb but in '{}' state", state);
            println!("  This is almost always a stale adb server. Run:");
            println!("    adb kill-server");
            println!("  then re-run `pctl doctor`.");
            return Ok(());
        }
        AdbDiagnosis::PanoramaUnauthorized => {
            println!("✗ Cooler detected by adb but in 'unauthorized' state");
            println!("  Unusual for the cooler (no on-device UI to authorize).");
            println!("  Try restarting the adb server:");
            println!("    adb kill-server");
            println!("  then re-run `pctl doctor`.");
            return Ok(());
        }
        AdbDiagnosis::NoDevicesListed => {
            println!("✗ adb sees no devices");
            if serial_ok {
                // Serial works, so the cooler IS plugged in. Two suspects:
                // a stale adb server, or missing udev permissions.
                println!("  The cooler responded over serial, so it IS connected —");
                println!("  this is either a stale adb server or a missing udev rule.");
                println!();
                println!("  First, try restarting the adb server:");
                println!("    adb kill-server");
                println!("  then re-run `pctl doctor`. If adb still sees nothing,");
                println!("  install the udev rule:");
                println!("    sudo pctl setup");
            } else {
                println!("  Check the USB cable and that the cooler is powered on.");
            }
            return Ok(());
        }
        AdbDiagnosis::NonPanoramaOnly { detected } => {
            println!("✗ adb sees other devices but no Panorama");
            println!("  Detected: {}", detected);
            if serial_ok {
                println!("  The cooler answered over serial — likely a stale adb server.");
                println!("  Try: adb kill-server, then re-run `pctl doctor`.");
            }
            return Ok(());
        }
        AdbDiagnosis::NotInstalled => {
            println!("✗ `adb` is not installed or not in PATH");
            println!("  Install it (e.g. `pacman -S android-tools` on Arch,");
            println!("  `apt install adb` on Debian/Ubuntu) and re-run.");
            return Ok(());
        }
        AdbDiagnosis::ServerUnreachable(err) => {
            println!("✗ `adb devices` failed: {}", err);
            println!("  Try: adb kill-server, then re-run `pctl doctor`.");
            return Ok(());
        }
    }

    // Validate device identity
    print!("Device validation: ");
    match adb.validate_device() {
        DeviceValidation::Confirmed(ConfirmationKind::MediaPathPresent) => {
            println!("✓ Panorama confirmed");
            println!("  Media path verified: /sdcard/pcMedia/");
        }
        DeviceValidation::Confirmed(ConfirmationKind::ProductInfoMatched) => {
            println!("✓ Panorama confirmed");
            println!("  Validated via product info (media path not yet created)");
        }
        DeviceValidation::NotConnected => {
            println!("✗ No device in 'device' state");
        }
        DeviceValidation::NotPanorama { detected } => {
            println!("✗ Device does not match Panorama signatures");
            println!("  Detected: {}", detected);
            return Ok(());
        }
    }

    if serial_ok {
        println!("\n✓ All checks passed");
        if let Some(path) = should_warn_about_shadowed_packaged_pctl() {
            println!("\n⚠ Package-managed assets are installed, but `pctl` resolves to:");
            println!("  {}", path.display());
            println!("  Expected packaged path: /usr/bin/pctl");
            println!("  A leftover manual install may be shadowing the packaged binary.");
        }
    } else {
        println!("\n✗ Serial connection failed — see the error above");
    }
    Ok(())
}

// TODO(packaging): Stop writing the udev rule from inside the binary.
// Convention (Debian Policy ch. 9, Arch packaging, android-udev,
// wireshark-common, etc.) is that privileged FS work — udev rules,
// system systemd units, capabilities, groups — ships via distro
// packaging hooks (postinst / %post / .install) or a separate
// privileged helper, not from the application binary itself. This
// also satisfies privilege-separation guidance (CERT POS02-C,
// Wheeler's Secure Programming HOWTO §7.4). Once distro-managed
// package/install flows are the normal path, revisit whether
// `pctl setup` should remain as a manual-install-only fallback or be
// removed entirely.
const UDEV_RULE_DEST: &str = "/etc/udev/rules.d/70-tryx-panorama.rules";
const PACKAGE_UDEV_RULE_PATH: &str = "/usr/lib/udev/rules.d/70-tryx-panorama.rules";
const PACKAGE_USER_SERVICE_PATH: &str = "/usr/lib/systemd/user/panorama.service";
const UDEV_RULE_CONTENT: &str = include_str!("../../../packaging/70-tryx-panorama.rules");

fn package_managed_setup_assets_present_with(
    package_udev_rule_exists: bool,
    package_user_service_exists: bool,
) -> bool {
    package_udev_rule_exists || package_user_service_exists
}

fn package_managed_setup_assets_present() -> bool {
    package_managed_setup_assets_present_with(
        std::path::Path::new(PACKAGE_UDEV_RULE_PATH).exists(),
        std::path::Path::new(PACKAGE_USER_SERVICE_PATH).exists(),
    )
}

fn resolved_pctl_path() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("pctl"))
        .find(|candidate| candidate.is_file())
}

fn should_warn_about_shadowed_packaged_pctl() -> Option<std::path::PathBuf> {
    if !package_managed_setup_assets_present() {
        return None;
    }

    let resolved = resolved_pctl_path()?;
    if resolved != std::path::Path::new("/usr/bin/pctl") {
        Some(resolved)
    } else {
        None
    }
}

/// Install the bundled udev rule + apply it live. Requires root for
/// manual/cargo installs. Package-managed installs should use the packaged
/// udev rule + user service instead.
fn cmd_setup() -> Result<()> {
    if package_managed_setup_assets_present() {
        anyhow::bail!(
            "package-managed setup assets are already present ({} and/or {}).\n`pctl setup` is for manual/cargo installs only.\nUse your package manager for privileged assets, then run:\n  systemctl --user daemon-reload\n  systemctl --user enable --now panorama.service\n  pctl doctor",
            PACKAGE_UDEV_RULE_PATH,
            PACKAGE_USER_SERVICE_PATH,
        );
    }

    // Write the rule. Permission denied is the "you forgot sudo" case; wrap
    // it with a clear hint. Other failures bubble up unchanged.
    if let Err(e) = std::fs::write(UDEV_RULE_DEST, UDEV_RULE_CONTENT) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::bail!(
                "pctl setup writes to {} — re-run with `sudo pctl setup`",
                UDEV_RULE_DEST
            );
        }
        return Err(anyhow::anyhow!("failed to write {}: {}", UDEV_RULE_DEST, e));
    }
    println!("✓ Installed udev rule at {}", UDEV_RULE_DEST);

    run_udevadm(&["control", "--reload-rules"])?;
    println!("✓ Reloaded udev rules");

    run_udevadm(&[
        "trigger",
        "--action=add",
        "--subsystem-match=usb",
        "--subsystem-match=tty",
    ])?;
    println!("✓ Re-applied rule to connected devices");

    println!("\n✓ Setup complete — run `pctl doctor` once pctl is on PATH to confirm adb access.");
    Ok(())
}

/// Spawn a `udevadm <args>` invocation, surfacing useful errors. udevadm
/// writes its own message to stderr (which we inherit), so we only need to
/// wrap launch failures and exit-status failures.
fn run_udevadm(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("udevadm")
        .args(args)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("`udevadm` not found in PATH — install systemd / udev")
            } else {
                anyhow::anyhow!("failed to spawn `udevadm {}`: {}", args.join(" "), e)
            }
        })?;
    if !status.success() {
        anyhow::bail!(
            "`udevadm {}` failed with {} — re-run with `sudo pctl setup`",
            args.join(" "),
            status
        );
    }
    Ok(())
}

fn cmd_brightness(value: u8) -> Result<()> {
    use panorama_core::device::Device;

    if value > 100 {
        anyhow::bail!("Brightness must be 0-100, got {}", value);
    }

    if !try_ipc_set_brightness(value as i32)? {
        let mut device = Device::new();
        device.connect()?;
        device.set_brightness(value as i32)?;
    }
    persist_config_field(|cfg| cfg.brightness = value as i32);

    println!("✓ Brightness set to {}", value);
    Ok(())
}

fn cmd_fan_lcd(value: u8) -> Result<()> {
    use panorama_core::device::Device;

    if value > 100 {
        anyhow::bail!("Fan LCD speed must be 0-100, got {}", value);
    }

    if !try_ipc_set_fan_lcd(value as i32)? {
        let mut device = Device::new();
        device.connect()?;
        device.set_fan_lcd(value as i32)?;
    }
    persist_config_field(|cfg| cfg.fan_lcd_percent = value as i32);

    println!("✓ Fan LCD speed set to {}%", value);
    Ok(())
}

/// Apply `mutate` to the on-disk config so subsequent `pctl config show`
/// reflects the value just written to the device. Best-effort: a failed
/// load or save warns rather than bailing, since the device write has
/// already succeeded by the time this is called.
fn persist_config_field<F>(mutate: F)
where
    F: FnOnce(&mut panorama_core::config::Config),
{
    use panorama_core::config;
    let Some(mut cfg) = config::load_config() else {
        eprintln!(
            "⚠ Config at {} unreadable; not persisting the new value",
            config::config_path().display()
        );
        return;
    };
    mutate(&mut cfg);
    if !config::save_config(&cfg) {
        eprintln!(
            "⚠ Failed to write config to {}; `pctl config show` may be stale",
            config::config_path().display()
        );
    }
}

fn print_device_status(response: &panorama_core::protocol::Response) {
    let Some(json) = response.json.as_ref() else {
        println!("  (no JSON body in response)");
        return;
    };

    let available = json
        .get("availableStorage")
        .and_then(|v| v.as_u64())
        .map(|b| {
            format!(
                "{} ({} bytes)",
                humansize::format_size(b, humansize::BINARY),
                b
            )
        })
        .unwrap_or_else(|| "(field absent)".to_string());
    println!("  Available storage: {}", available);

    let status = json.get("status");
    let fan = status
        .and_then(|s| s.get("fanLCD"))
        .and_then(parse_rpm_value)
        .map(|rpm| format!("{} RPM", rpm))
        .unwrap_or_else(|| "(field absent)".to_string());
    println!("  Fan LCD: {}", fan);

    let pump = status
        .and_then(|s| s.get("turboPump"))
        .and_then(parse_rpm_value)
        .map(|rpm| format!("{} RPM", rpm))
        .unwrap_or_else(|| "(field absent)".to_string());
    println!("  Turbo pump: {}", pump);

    // `warning` is a JSON-encoded string holding an array of {type, description}.
    let warnings = json
        .get("warning")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    match warnings.as_ref().and_then(|w| w.as_array()) {
        Some(items) => {
            let active: Vec<_> = items
                .iter()
                .filter(|w| {
                    w.get("description")
                        .and_then(|d| d.as_str())
                        .map(|d| !d.eq_ignore_ascii_case("No ERROR"))
                        .unwrap_or(true)
                })
                .collect();
            if active.is_empty() {
                println!("  Warnings: none");
            } else {
                println!("  Warnings:");
                for w in active {
                    let kind = w.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                    let desc = w
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(no description)");
                    println!("    {}: {}", kind, desc);
                }
            }
        }
        None => println!("  Warnings: (field absent)"),
    }
}

/// Parse an RPM value, accepting either a JSON number or a string of digits.
fn parse_rpm_value(value: &serde_json::Value) -> Option<u32> {
    if let Some(n) = value.as_u64() {
        return Some(n as u32);
    }
    if let Some(f) = value.as_f64() {
        return Some(f.round() as u32);
    }
    value.as_str().and_then(|s| s.trim().parse().ok())
}

fn cmd_fan_rpm(watch: Option<u64>) -> Result<()> {
    use panorama_core::device::{poll_fan_status, Device, FanStatus};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let print_status = |status: FanStatus| {
        let mut parts = Vec::new();
        if let Some(rpm) = status.fan_lcd_rpm {
            parts.push(format!("fan_lcd_rpm={}", rpm));
        }
        if let Some(rpm) = status.turbo_pump_rpm {
            parts.push(format!("turbo_pump_rpm={}", rpm));
        }
        if parts.is_empty() {
            println!("(device reported no fan or pump RPM fields)");
        } else {
            println!("{}", parts.join(" "));
        }
    };

    match watch {
        None => {
            if let Some(status) = try_ipc_read_fan_status()? {
                print_status(status);
            } else {
                let mut device = Device::new();
                device.connect()?;
                let status = device.read_fan_status()?;
                print_status(status);
            }
        }
        Some(secs) => {
            if secs == 0 {
                anyhow::bail!("--watch interval must be at least 1 second");
            }
            let running = Arc::new(AtomicBool::new(true));
            {
                let running = Arc::clone(&running);
                ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))
                    .map_err(|e| anyhow::anyhow!("could not install signal handler: {}", e))?;
            }

            if let Some(status) = try_ipc_read_fan_status()? {
                print_status(status);
                while running.load(Ordering::SeqCst) {
                    interruptible_sleep(std::time::Duration::from_secs(secs), &running);
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                    match try_ipc_read_fan_status()? {
                        Some(status) => print_status(status),
                        None => anyhow::bail!("daemon IPC became unavailable during fan-rpm watch"),
                    }
                }
            } else {
                let mut device = Device::new();
                device.connect()?;
                poll_fan_status(
                    &mut device,
                    std::time::Duration::from_secs(secs),
                    &running,
                    print_status,
                )?;
            }
        }
    }
    Ok(())
}

fn cmd_reboot() -> Result<()> {
    use panorama_core::device::Device;

    if !try_ipc_reboot()? {
        let mut device = Device::new();
        device.connect()?;
        device.reboot()?;
    }

    println!("✓ Device reboot initiated");
    Ok(())
}

fn cmd_list() -> Result<()> {
    use panorama_core::adb::Adb;

    let adb = Adb::new();
    if !adb.is_device_connected() {
        anyhow::bail!("No device detected by adb");
    }

    match adb.list_media() {
        Some(files) if !files.is_empty() => {
            println!("Media files on device:");
            for file in files {
                println!("  {}", file);
            }
        }
        _ => {
            println!("No media files found on device");
        }
    }

    Ok(())
}

fn cmd_delete(files: Vec<String>, all: bool, yes: bool) -> Result<()> {
    use panorama_core::adb::Adb;

    // Mutually exclusive inputs
    if all && !files.is_empty() {
        anyhow::bail!("--all cannot be combined with filename arguments");
    }
    if !all && files.is_empty() {
        anyhow::bail!("No files specified. Use --all to delete everything.");
    }

    if let Some(suggested_files) = likely_comma_separated_delete_files(&files) {
        anyhow::bail!(
            "`pctl delete` expects separate filename arguments, not a comma-separated list.\n\n\
             Did you mean:\n  pctl delete {}",
            format_delete_suggestion(&suggested_files)
        );
    }

    // Bulk wipe: delete every media file on the device
    if all {
        let adb = Adb::new();
        if !adb.is_device_connected() {
            anyhow::bail!("No device detected by adb");
        }

        let all_files = adb.list_media().unwrap_or_default();
        if all_files.is_empty() {
            println!("No media files on device");
            return Ok(());
        }

        println!("The following {} file(s) will be deleted:", all_files.len());
        for file in &all_files {
            println!("  {}", file);
        }

        if !yes && !confirm(&format!("Delete all {} file(s)? [y/N] ", all_files.len())) {
            println!("Aborted.");
            return Ok(());
        }

        for file in &all_files {
            if adb.remove(file) {
                println!("✓ Deleted {}", file);
            } else {
                eprintln!("✗ Failed to delete {}", file);
            }
        }

        return Ok(());
    }

    // Validate no path traversal attempts
    for pattern in &files {
        if pattern.contains('/') || pattern.contains('\\') || pattern.contains("..") {
            anyhow::bail!(
                "Invalid filename '{}': path separators and '..' not allowed. \
                 Only filenames in /sdcard/pcMedia/ can be deleted.",
                pattern
            );
        }
    }

    let adb = Adb::new();
    if !adb.is_device_connected() {
        anyhow::bail!("No device detected by adb");
    }

    // Expand glob patterns by listing all files and filtering
    let all_files = adb.list_media().unwrap_or_default();
    let mut files_to_delete = Vec::new();

    for pattern in &files {
        if pattern.contains('*') {
            // Simple glob: prefix* or *suffix or prefix*suffix
            let parts: Vec<&str> = pattern.split('*').collect();
            for file in &all_files {
                let matches = match parts.as_slice() {
                    [prefix, ""] => file.starts_with(prefix),
                    ["", suffix] => file.ends_with(suffix),
                    [prefix, suffix] => file.starts_with(prefix) && file.ends_with(suffix),
                    _ => false,
                };
                if matches && !files_to_delete.contains(file) {
                    files_to_delete.push(file.clone());
                }
            }
        } else {
            files_to_delete.push(pattern.clone());
        }
    }

    if files_to_delete.is_empty() {
        println!("No files matched the specified patterns");
        return Ok(());
    }

    for file in &files_to_delete {
        if adb.remove(file) {
            println!("✓ Deleted {}", file);
        } else {
            eprintln!("✗ Failed to delete {}", file);
        }
    }

    Ok(())
}

fn likely_comma_separated_delete_files(files: &[String]) -> Option<Vec<String>> {
    if files.len() != 1 || !files[0].contains(',') {
        return None;
    }

    let parts: Vec<String> = files[0]
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();

    if parts.len() < 2 || parts.iter().any(|part| !looks_like_delete_pattern(part)) {
        return None;
    }

    Some(parts)
}

fn looks_like_delete_pattern(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return false;
    }

    value.contains('*')
        || panorama_core::media::detect_type(value) != panorama_core::media::MediaType::Unknown
}

fn format_delete_suggestion(files: &[String]) -> String {
    files
        .iter()
        .map(|file| {
            shlex::try_quote(file)
                .map(|quoted| quoted.into_owned())
                .unwrap_or_else(|_| file.clone())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn cmd_upload(file: String) -> Result<()> {
    use panorama_core::adb::Adb;

    let adb = Adb::new();
    if !adb.is_device_connected() {
        anyhow::bail!("No device detected by adb");
    }

    let remote_name = upload_file(&adb, &file)?;
    println!("✓ Uploaded {}", remote_name);
    Ok(())
}

fn cmd_library_import(dir: String) -> Result<()> {
    use panorama_core::adb::Adb;
    use panorama_core::media::{self, MediaType};
    use std::path::Path;

    if !Path::new(&dir).is_dir() {
        anyhow::bail!("Not a directory: {}", dir);
    }

    // Collect top-level media files; skip subdirectories and non-media entries.
    let mut media_files: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let path_str = path.to_string_lossy().into_owned();
        if media::detect_type(&path_str) == MediaType::Unknown {
            skipped += 1;
            continue;
        }
        media_files.push(path_str);
    }
    media_files.sort();

    if media_files.is_empty() {
        println!("No media files found in {}", dir);
        return Ok(());
    }

    let adb = Adb::new();
    if !adb.is_device_connected() {
        anyhow::bail!("No device detected by adb");
    }

    let total = media_files.len();
    let mut imported = 0usize;
    for file in &media_files {
        match upload_file(&adb, file) {
            Ok(remote_name) => {
                println!("✓ Uploaded {}", remote_name);
                imported += 1;
            }
            Err(e) => {
                eprintln!("✗ Failed {}: {}", file, e);
            }
        }
    }

    println!("\nImported {} of {} media file(s)", imported, total);
    if skipped > 0 {
        println!("Skipped {} non-media file(s)", skipped);
    }
    Ok(())
}

fn validate_device_media_filename(filename: &str) -> Result<()> {
    if !panorama_core::adb::is_safe_media_filename(filename) {
        anyhow::bail!(
            "`pctl display` expects a media filename already on the AIO, not a local path: {}\n\
             Upload local files with `pctl upload <local-file>`, then show them with \
             `pctl display <filename>`.",
            filename
        );
    }

    Ok(())
}

fn resolve_device_media(adb: &panorama_core::adb::Adb, filename: &str) -> Result<String> {
    validate_device_media_filename(filename)?;

    if !adb.file_exists(filename) {
        anyhow::bail!(
            "media not found on the AIO: {}\n\
             Use `pctl list` to see uploaded media, or upload it with \
             `pctl upload <local-file>` first.",
            filename
        );
    }

    Ok(filename.to_string())
}

/// Arguments forwarded from the `Display` clap variant. Bundled into a
/// struct because the parameter count outgrew a comfortable function
/// signature once split-mode landed.
struct DisplayArgs {
    file: Option<String>,
    split: bool,
    media2: Option<Option<String>>,
    metrics: Option<Vec<String>>,
    metrics2: Option<Vec<String>>,
    metrics_color: Option<String>,
    metrics_align: Option<MetricsAlign>,
    metrics_position: Option<MetricsPosition>,
    filter: Option<DisplayFilter>,
    ratio: Option<ScreenRatio>,
}

/// Display media that is already on the cooler and/or configure the on-screen
/// metrics overlay.
///
/// In full-screen (default) mode: at least one of `file`, `--metrics`,
/// `--ratio`, or an appearance flag must be given.
/// In `--split` mode: pane 1 (`file`) and pane 2 (`--media2`) are each
/// optional, but at least one must end up with media (from a flag or saved
/// state) — both-dark is refused.
fn cmd_display(args: DisplayArgs) -> Result<()> {
    use panorama_core::adb::Adb;
    use panorama_core::config;
    use panorama_core::device::Device;
    use panorama_core::display::{plan_display, DisplayPlanInput};

    // --- 1. Up-front validation -------------------------------------------

    if !args.split && (args.media2.is_some() || args.metrics2.is_some()) {
        anyhow::bail!("--media2 / --metrics2 require --split");
    }

    let mut state = config::load_state().unwrap_or_default();

    if let Some(filename) = &args.file {
        validate_device_media_filename(filename)?;
    }
    if let Some(Some(filename)) = &args.media2 {
        validate_device_media_filename(filename)?;
    }

    // --- 3. adb setup (only if we need to verify named device media) --------

    let needs_media_check = args.file.is_some() || matches!(&args.media2, Some(Some(_)));
    let adb = if needs_media_check {
        let a = Adb::new();
        if !a.is_device_connected() {
            anyhow::bail!("No device detected by adb");
        }
        Some(a)
    } else {
        None
    };

    // --- 4. Resolve requested media ----------------------------------------

    let file = match &args.file {
        Some(filename) => {
            let a = adb.as_ref().expect("adb initialized above");
            Some(resolve_device_media(a, filename)?)
        }
        None => None,
    };

    let media2 = if args.split {
        match &args.media2 {
            Some(Some(filename)) => {
                let a = adb.as_ref().expect("adb initialized above");
                Some(Some(resolve_device_media(a, filename)?))
            }
            Some(None) => Some(None),
            None => None,
        }
    } else {
        None
    };

    // --- 5. Build the ScreenConfig and next saved state ---------------------

    let plan = plan_display(
        DisplayPlanInput {
            file,
            split: args.split,
            media2,
            metrics: args.metrics.clone(),
            metrics2: args.metrics2.clone(),
            badges: None,
            badges2: None,
            metrics_color: args.metrics_color.clone(),
            metrics_align: args.metrics_align.map(|a| a.as_device_str().to_string()),
            metrics_position: args.metrics_position.map(|p| p.as_device_str().to_string()),
            filter: args.filter.map(|filter| filter.as_device_str().to_string()),
            ratio: args.ratio.map(|r| r.as_device_str().to_string()),
        },
        state.clone(),
    )?;

    if !try_ipc_set_screen_config(&plan.screen)? {
        let mut device = Device::new();
        device.connect()?;
        device.set_screen_config(&plan.screen)?;
    }

    // --- 6. Persist applied state ------------------------------------------

    state = plan.next_state.clone();
    if !config::save_state(&state) {
        eprintln!("⚠ Display set, but failed to persist state to display.json");
    }

    // --- 7. Summary output -------------------------------------------------

    if args.split {
        let pane1_label = plan
            .pane1_media
            .first()
            .map(String::as_str)
            .unwrap_or("(dark)");
        let pane2_label = plan
            .pane2_media
            .first()
            .map(String::as_str)
            .unwrap_or("(dark)");
        println!(
            "✓ Split mode @ {} — pane 1: {} · pane 2: {}",
            plan.screen_ratio, pane1_label, pane2_label
        );
        match &args.metrics {
            Some(t) if t.is_empty() => println!("✓ Pane 1 overlay removed"),
            Some(_) => println!("✓ Pane 1 overlay: {}", plan.sysinfo_display.join(", ")),
            None => {}
        }
        match &args.metrics2 {
            Some(t) if t.is_empty() => println!("✓ Pane 2 overlay removed"),
            Some(_) => println!("✓ Pane 2 overlay: {}", plan.sysinfo_display2.join(", ")),
            None => {}
        }
    } else {
        if args.file.is_some() {
            if let Some(name) = plan.pane1_media.first() {
                println!("✓ Now displaying {}", name);
            }
        }
        match &args.metrics {
            Some(t) if t.is_empty() => println!("✓ Metrics overlay removed"),
            Some(_) => println!("✓ Metrics overlay: {}", plan.sysinfo_display.join(", ")),
            None => {}
        }
    }
    if plan.appearance_change {
        println!(
            "✓ Overlay appearance: {} · {} · {} · filter {}",
            plan.overlay_color,
            plan.overlay_align,
            plan.overlay_position,
            args.filter.map(DisplayFilter::as_user_str).unwrap_or(
                if plan.display_filter.is_empty() {
                    "none"
                } else {
                    &plan.display_filter
                }
            )
        );
    }
    if args.ratio.is_some() {
        println!("✓ Ratio: {}", plan.screen_ratio);
    }

    Ok(())
}

fn parse_sleep_state_text(state: &str) -> Result<SleepState> {
    if state.eq_ignore_ascii_case("on") {
        Ok(SleepState::On)
    } else if state.eq_ignore_ascii_case("off") {
        Ok(SleepState::Off)
    } else {
        anyhow::bail!("sleep state must be 'on' or 'off', got '{}'", state);
    }
}

fn apply_sleep_state(
    device: &mut panorama_core::device::Device,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    state: &str,
) -> Result<Option<panorama_core::protocol::Response>> {
    use panorama_core::config;
    use panorama_core::device::{DisplaySettings, ScreenConfig};

    let state = parse_sleep_state_text(state)?;
    let on = matches!(state, SleepState::On);
    let display_in_sleep = !on;

    let display = config::load_state().unwrap_or_default();
    if display.media.is_empty() {
        anyhow::bail!(
            "no media on the cooler — upload one with `pctl upload <local-file>`, \
             then run `pctl display <filename>` so this does not blank the screen"
        );
    }
    let brightness = config::load_config().unwrap_or_default().brightness;

    let screen = ScreenConfig {
        media: display.media.clone(),
        sysinfo_display: display.sysinfo_display.clone(),
        ratio: display.ratio.clone(),
        screen_mode: display.screen_mode.clone(),
        play_mode: display.play_mode.clone(),
        settings: DisplaySettings {
            color: display.metrics_color.clone(),
            align: display.metrics_align.clone(),
            position: display.metrics_position.clone(),
            filter_value: display.display_filter.clone(),
            filter_opacity: panorama_core::display::display_filter_opacity(&display.display_filter),
            ..DisplaySettings::default()
        },
        display_in_sleep,
        ..ScreenConfig::default()
    };

    let cpu_name = read_cpu_name();
    let gpu_name = monitor
        .poll()
        .gpus
        .into_iter()
        .next()
        .map(|g| g.name)
        .unwrap_or_default();

    Ok(device.send_full_config(&screen, &cpu_name, &gpu_name, brightness, "Celsius")?)
}

/// Set screen sleep mode by sending the full `POST config` payload. `on` lets
/// the cooler sleep its screen after the idle timeout. This maps to the
/// device's `displayInSleep` flag inverted — verified on hardware: a `true`
/// `displayInSleep` keeps the screen lit, `false` lets it sleep.
fn cmd_sleep(state: SleepState) -> Result<()> {
    use panorama_core::device::Device;
    use panorama_core::metrics::SystemMonitor;

    let state_text = if matches!(state, SleepState::On) {
        "on"
    } else {
        "off"
    };

    if !try_ipc_set_sleep(state_text)? {
        let mut device = Device::new();
        let mut monitor = SystemMonitor::new();
        device.connect()?;
        apply_sleep_state(&mut device, &mut monitor, state_text)?;
    }

    if matches!(state, SleepState::On) {
        println!("✓ Screen sleep mode ON — the screen will sleep after the cooler's idle timeout when the device is no longer being actively refreshed.");
        println!("  A running keepalive daemon keeps the device active while the host is awake; the sleep setting takes effect once refresh traffic stops (for example when the daemon is stopped or the host suspends).");
    } else {
        println!("✓ Screen sleep mode OFF — the screen stays on after the idle timeout when the device is no longer being actively refreshed.");
    }
    Ok(())
}

/// Best-effort CPU model name from `/proc/cpuinfo` (empty string if unavailable).
fn read_cpu_name() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|name| name.trim().to_string())
        })
        .unwrap_or_default()
}

fn cmd_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => cmd_config_show(),
        ConfigAction::Set { key, value } => cmd_config_set(key, value),
    }
}

fn cmd_config_show() -> Result<()> {
    use panorama_core::config;

    let path = config::config_path();
    // load_config yields defaults when the file is absent, None only when it
    // exists but is unreadable or malformed — surface that rather than masking
    // a broken file behind defaults.
    let cfg = config::load_config().ok_or_else(|| {
        anyhow::anyhow!(
            "Config at {} exists but is unreadable or malformed",
            path.display()
        )
    })?;

    println!("Configuration ({}):", path.display());
    let port = if cfg.port.is_empty() {
        "(unset — auto-detect)"
    } else {
        &cfg.port
    };
    println!("  port:               {}", port);
    println!("  brightness:         {}", cfg.brightness);
    println!("  keepalive-interval: {}", cfg.keepalive_interval);
    println!("  fan-lcd-percent:    {}", cfg.fan_lcd_percent);
    Ok(())
}

fn cmd_config_set(key: String, value: String) -> Result<()> {
    use panorama_core::config;

    let path = config::config_path();
    let mut cfg = config::load_config().ok_or_else(|| {
        anyhow::anyhow!(
            "Config at {} exists but is unreadable or malformed",
            path.display()
        )
    })?;

    apply_config_set(&mut cfg, &key, &value)?;

    if !config::save_config(&cfg) {
        anyhow::bail!("Failed to write config to {}", path.display());
    }

    println!("✓ Set {} = {}", key, value);
    Ok(())
}

/// Apply a single `key = value` update to `cfg`, validating the value.
/// Pure — no I/O; the caller persists. Returns an error for an unknown key
/// or a value outside the accepted range.
fn apply_config_set(cfg: &mut panorama_core::config::Config, key: &str, value: &str) -> Result<()> {
    match key {
        "port" => cfg.port = value.to_string(),
        "brightness" => cfg.brightness = parse_percent(value, "brightness")?,
        "keepalive-interval" => {
            let secs: i32 = value.parse().map_err(|_| {
                anyhow::anyhow!("keepalive-interval must be an integer, got '{}'", value)
            })?;
            if secs < 1 {
                anyhow::bail!("keepalive-interval must be at least 1 second, got {}", secs);
            }
            cfg.keepalive_interval = secs;
        }
        "fan-lcd-percent" => cfg.fan_lcd_percent = parse_percent(value, "fan-lcd-percent")?,
        _ => anyhow::bail!(
            "Unknown config key '{}'. Valid keys: port, brightness, \
             keepalive-interval, fan-lcd-percent",
            key
        ),
    }
    Ok(())
}

/// Parse a 0-100 percentage, naming `key` in any error message.
fn parse_percent(value: &str, key: &str) -> Result<i32> {
    let n: i32 = value
        .parse()
        .map_err(|_| anyhow::anyhow!("{} must be an integer, got '{}'", key, value))?;
    if !(0..=100).contains(&n) {
        anyhow::bail!("{} must be 0-100, got {}", key, n);
    }
    Ok(n)
}

/// Initialise tracing for the daemon. Logs to stderr; under systemd that is
/// captured by the journal, so no log file is managed here. `RUST_LOG`
/// overrides the default `info` level.
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time() // journald timestamps every line already
        .with_writer(std::io::stderr)
        .init();
}

fn notify_service_manager(state: &[sd_notify::NotifyState<'_>]) {
    if let Err(e) = sd_notify::notify(state) {
        tracing::warn!("daemon: failed to notify service manager: {}", e);
    }
}

/// The keepalive/metrics loop interval, from `Config.keepalive_interval`
/// (seconds), clamped to at least 1 s against a hand-edited config file.
fn keepalive_duration(cfg: &panorama_core::config::Config) -> std::time::Duration {
    std::time::Duration::from_secs(cfg.keepalive_interval.max(1) as u64)
}

/// Sleep up to `total`, waking early (within ~250 ms) if `running` clears —
/// so SIGINT/SIGTERM is honoured without waiting out a full interval.
fn interruptible_sleep(total: std::time::Duration, running: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    let slice = std::time::Duration::from_millis(250);
    let mut elapsed = std::time::Duration::ZERO;
    while elapsed < total && running.load(Ordering::SeqCst) {
        let nap = slice.min(total - elapsed);
        std::thread::sleep(nap);
        elapsed += nap;
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcMessagePayload {
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcValuePayload {
    value: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcSleepPayload {
    state: String,
}

#[derive(Debug, Deserialize)]
struct IpcSysinfoDisplayPayload {
    items: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct IpcSysinfoPayload {
    items: Vec<panorama_core::device::SysinfoData>,
}

fn send_ipc_request(
    request: panorama_core::ipc::IpcRequest,
) -> Result<Option<panorama_core::ipc::IpcResponse>> {
    panorama_core::ipc::send_request(request)
        .map_err(|e| anyhow::anyhow!("daemon IPC request failed: {}", e))
}

fn ipc_status_name(status: panorama_core::ipc::IpcStatus) -> &'static str {
    use panorama_core::ipc::IpcStatus;

    match status {
        IpcStatus::Ok => "ok",
        IpcStatus::BadRequest => "bad request",
        IpcStatus::Unsupported => "unsupported",
        IpcStatus::DeviceNotConnected => "device not connected",
        IpcStatus::DeviceError => "device error",
        IpcStatus::InternalError => "internal error",
    }
}

fn ipc_response_message(response: &panorama_core::ipc::IpcResponse) -> String {
    response
        .payload_as::<IpcMessagePayload>()
        .ok()
        .flatten()
        .map(|payload| payload.message)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| ipc_status_name(response.status).to_string())
}

fn ipc_payload<T: DeserializeOwned>(
    response: panorama_core::ipc::IpcResponse,
    context: &str,
) -> Result<T> {
    use panorama_core::ipc::IpcStatus;

    match response.status {
        IpcStatus::Ok => response
            .payload_as::<T>()
            .map_err(|e| anyhow::anyhow!("invalid {} IPC payload: {}", context, e))?
            .ok_or_else(|| anyhow::anyhow!("daemon returned no {} payload", context)),
        _ => anyhow::bail!(
            "daemon {}: {}",
            ipc_status_name(response.status),
            ipc_response_message(&response)
        ),
    }
}

fn ipc_expect_ok(response: panorama_core::ipc::IpcResponse, context: &str) -> Result<()> {
    use panorama_core::ipc::IpcStatus;

    match response.status {
        IpcStatus::Ok => Ok(()),
        _ => anyhow::bail!(
            "daemon {} while handling {}: {}",
            ipc_status_name(response.status),
            context,
            ipc_response_message(&response)
        ),
    }
}

fn try_ipc_device_info() -> Result<Option<panorama_core::device::DeviceInfo>> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let Some(response) = send_ipc_request(IpcRequest::new(IpcCommand::DeviceInfo))? else {
        return Ok(None);
    };
    Ok(Some(ipc_payload(response, "device info")?))
}

fn try_ipc_state_all() -> Result<Option<panorama_core::protocol::Response>> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let Some(response) = send_ipc_request(IpcRequest::new(IpcCommand::StateAll))? else {
        return Ok(None);
    };
    Ok(Some(ipc_payload(response, "STATE all")?))
}

fn try_ipc_set_brightness(value: i32) -> Result<bool> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let request = IpcRequest::with_payload(IpcCommand::SetBrightness, &IpcValuePayload { value })
        .map_err(|e| anyhow::anyhow!("could not build brightness IPC request: {}", e))?;
    let Some(response) = send_ipc_request(request)? else {
        return Ok(false);
    };
    ipc_expect_ok(response, "brightness")?;
    Ok(true)
}

fn try_ipc_set_fan_lcd(value: i32) -> Result<bool> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let request = IpcRequest::with_payload(IpcCommand::SetFanLcd, &IpcValuePayload { value })
        .map_err(|e| anyhow::anyhow!("could not build fan LCD IPC request: {}", e))?;
    let Some(response) = send_ipc_request(request)? else {
        return Ok(false);
    };
    ipc_expect_ok(response, "fan LCD")?;
    Ok(true)
}

fn try_ipc_read_fan_status() -> Result<Option<panorama_core::device::FanStatus>> {
    let Some(response) = try_ipc_state_all()? else {
        return Ok(None);
    };
    Ok(Some(panorama_core::device::parse_fan_status(&response)))
}

fn try_ipc_reboot() -> Result<bool> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let Some(response) = send_ipc_request(IpcRequest::new(IpcCommand::Reboot))? else {
        return Ok(false);
    };
    ipc_expect_ok(response, "reboot")?;
    Ok(true)
}

fn try_ipc_set_screen_config(screen: &panorama_core::device::ScreenConfig) -> Result<bool> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let request = IpcRequest::with_payload(IpcCommand::SetScreenConfig, screen)
        .map_err(|e| anyhow::anyhow!("could not build screen-config IPC request: {}", e))?;
    let Some(response) = send_ipc_request(request)? else {
        return Ok(false);
    };
    ipc_expect_ok(response, "screen config")?;
    Ok(true)
}

fn try_ipc_set_sleep(state: &str) -> Result<bool> {
    use panorama_core::ipc::{IpcCommand, IpcRequest};

    let request = IpcRequest::with_payload(
        IpcCommand::SetSleep,
        &IpcSleepPayload {
            state: state.to_string(),
        },
    )
    .map_err(|e| anyhow::anyhow!("could not build sleep IPC request: {}", e))?;
    let Some(response) = send_ipc_request(request)? else {
        return Ok(false);
    };
    ipc_expect_ok(response, "sleep")?;
    Ok(true)
}

fn setup_ipc_listener() -> Result<Option<(std::os::unix::net::UnixListener, std::path::PathBuf)>> {
    use panorama_core::ipc;
    use std::os::unix::fs::PermissionsExt;

    let socket_path = match ipc::socket_path() {
        Ok(path) => path,
        Err(ipc::IpcError::MissingRuntimeDir) => {
            tracing::warn!("daemon: XDG_RUNTIME_DIR is not set; IPC socket disabled");
            return Ok(None);
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "could not determine IPC socket path: {}",
                e
            ))
        }
    };

    let socket_dir = socket_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("IPC socket path has no parent directory"))?;
    std::fs::create_dir_all(socket_dir)?;
    std::fs::set_permissions(socket_dir, std::fs::Permissions::from_mode(0o700))?;

    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("daemon: IPC listening on {}", socket_path.display());

    Ok(Some((listener, socket_path)))
}

fn cleanup_ipc_socket(socket_path: Option<&std::path::Path>) {
    let Some(path) = socket_path else {
        return;
    };
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                "daemon: failed to remove IPC socket {}: {}",
                path.display(),
                e
            );
        }
    }
}

fn daemon_wait(
    total: std::time::Duration,
    running: &std::sync::atomic::AtomicBool,
    listener: Option<&std::os::unix::net::UnixListener>,
    device: &mut panorama_core::device::Device,
    device_info: &panorama_core::device::DeviceInfo,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    active_screen: &mut Option<panorama_core::device::ScreenConfig>,
    last_state: &mut Option<panorama_core::protocol::Response>,
) {
    use std::sync::atomic::Ordering;

    let slice = std::time::Duration::from_millis(250);
    let mut elapsed = std::time::Duration::ZERO;
    while elapsed < total && running.load(Ordering::SeqCst) {
        if let Some(listener) = listener {
            service_ipc(
                listener,
                device,
                device_info,
                monitor,
                active_screen,
                last_state,
            );
        }
        let nap = slice.min(total - elapsed);
        interruptible_sleep(nap, running);
        elapsed += nap;
    }
}

fn service_ipc(
    listener: &std::os::unix::net::UnixListener,
    device: &mut panorama_core::device::Device,
    device_info: &panorama_core::device::DeviceInfo,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    active_screen: &mut Option<panorama_core::device::ScreenConfig>,
    last_state: &mut Option<panorama_core::protocol::Response>,
) {
    loop {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                if let Err(e) = handle_ipc_client(
                    &mut stream,
                    device,
                    device_info,
                    monitor,
                    active_screen,
                    last_state,
                ) {
                    tracing::warn!("daemon: IPC client request failed: {}", e);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                tracing::warn!("daemon: IPC accept failed: {}", e);
                break;
            }
        }
    }
}

fn handle_ipc_client(
    stream: &mut std::os::unix::net::UnixStream,
    device: &mut panorama_core::device::Device,
    device_info: &panorama_core::device::DeviceInfo,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    active_screen: &mut Option<panorama_core::device::ScreenConfig>,
    last_state: &mut Option<panorama_core::protocol::Response>,
) -> Result<()> {
    use std::io::Write;

    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;

    let frame = panorama_core::ipc::read_frame(stream)?;
    let request = panorama_core::ipc::IpcRequest::decode(&frame)
        .map_err(|e| anyhow::anyhow!("invalid IPC request: {}", e))?;
    let response = execute_ipc_request(
        request,
        device,
        device_info,
        monitor,
        active_screen,
        last_state,
    );
    let encoded = response
        .encode()
        .map_err(|e| anyhow::anyhow!("could not encode IPC response: {}", e))?;

    stream.write_all(&encoded)?;
    stream.flush()?;
    Ok(())
}

fn execute_ipc_request(
    request: panorama_core::ipc::IpcRequest,
    device: &mut panorama_core::device::Device,
    device_info: &panorama_core::device::DeviceInfo,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    active_screen: &mut Option<panorama_core::device::ScreenConfig>,
    last_state: &mut Option<panorama_core::protocol::Response>,
) -> panorama_core::ipc::IpcResponse {
    use panorama_core::device::ScreenConfig;
    use panorama_core::ipc::{IpcCommand, IpcResponse, IpcStatus};

    match request.command {
        IpcCommand::DeviceInfo => ipc_ok(device_info),
        IpcCommand::StateAll => {
            if let Some(response) = last_state.as_ref() {
                return ipc_ok(response);
            }
            let snapshot = monitor.poll().to_sysinfo();
            match device.send_sysinfo(&snapshot) {
                Ok(Some(response)) => {
                    *last_state = Some(response.clone());
                    ipc_ok(&response)
                }
                Ok(None) => ipc_device_error("device returned no response"),
                Err(e) => ipc_device_error(e.to_string()),
            }
        }
        IpcCommand::SetBrightness => match request.payload_as::<IpcValuePayload>() {
            Ok(Some(payload)) => match device.set_brightness(payload.value) {
                Ok(Some(response)) => ipc_ok(&response),
                Ok(None) => IpcResponse::new(IpcStatus::Ok),
                Err(e) => ipc_device_error(e.to_string()),
            },
            Ok(None) => ipc_bad_request("missing brightness payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::SetFanLcd => match request.payload_as::<IpcValuePayload>() {
            Ok(Some(payload)) => match device.set_fan_lcd(payload.value) {
                Ok(Some(response)) => ipc_ok(&response),
                Ok(None) => IpcResponse::new(IpcStatus::Ok),
                Err(e) => ipc_device_error(e.to_string()),
            },
            Ok(None) => ipc_bad_request("missing fan LCD payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::SetSleep => match request.payload_as::<IpcSleepPayload>() {
            Ok(Some(payload)) => match apply_sleep_state(device, monitor, &payload.state) {
                Ok(Some(response)) => ipc_ok(&response),
                Ok(None) => IpcResponse::new(IpcStatus::Ok),
                Err(e) => ipc_bad_request(e.to_string()),
            },
            Ok(None) => ipc_bad_request("missing sleep payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::SetScreenConfig => match request.payload_as::<ScreenConfig>() {
            Ok(Some(screen)) => {
                tracing::info!(
                    "daemon: IPC SetScreenConfig requested: {}",
                    summarize_screen_config(&screen)
                );

                // Hardware workaround: when shrinking the overlay set (for
                // example CPU+GPU -> CPU only) while the daemon owns the live
                // connection, the firmware can retain old extra labels unless
                // we clear first and then re-apply the target screen config.
                let previous_overlay_count = active_screen
                    .as_ref()
                    .map(|s| s.sysinfo_display.len() + s.sysinfo_display2.len())
                    .unwrap_or(0);
                let next_overlay_count =
                    screen.sysinfo_display.len() + screen.sysinfo_display2.len();
                if next_overlay_count < previous_overlay_count {
                    let mut cleared = screen.clone();
                    cleared.sysinfo_display.clear();
                    cleared.sysinfo_display2.clear();
                    if let Err(e) = device.set_screen_config(&cleared) {
                        tracing::warn!(
                            "daemon: pre-clear screen config before IPC update failed: {}",
                            e
                        );
                    } else {
                        std::thread::sleep(OVERLAY_CLEAR_SETTLE);
                    }
                }

                match device.set_screen_config(&screen) {
                    Ok(Some(response)) => {
                        *active_screen = Some(screen.clone());
                        send_badge_hardware_names_if_needed(device, monitor, &screen);
                        tracing::info!(
                            "daemon: active screen updated via IPC: {}",
                            summarize_screen_config(&screen)
                        );
                        ipc_ok(&response)
                    }
                    Ok(None) => {
                        *active_screen = Some(screen.clone());
                        send_badge_hardware_names_if_needed(device, monitor, &screen);
                        tracing::info!(
                            "daemon: active screen updated via IPC: {}",
                            summarize_screen_config(&screen)
                        );
                        IpcResponse::new(IpcStatus::Ok)
                    }
                    Err(e) => ipc_device_error(e.to_string()),
                }
            }
            Ok(None) => ipc_bad_request("missing screen config payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::SetSysinfoDisplay => match request.payload_as::<IpcSysinfoDisplayPayload>() {
            Ok(Some(payload)) => {
                let overlay = ScreenConfig {
                    sysinfo_display: payload.items,
                    ..ScreenConfig::default()
                };
                match device.set_sysinfo_display(&overlay) {
                    Ok(Some(response)) => ipc_ok(&response),
                    Ok(None) => IpcResponse::new(IpcStatus::Ok),
                    Err(e) => ipc_device_error(e.to_string()),
                }
            }
            Ok(None) => ipc_bad_request("missing sysinfo display payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::SendSysinfo => match request.payload_as::<IpcSysinfoPayload>() {
            Ok(Some(payload)) => match device.send_sysinfo(&payload.items) {
                Ok(Some(response)) => ipc_ok(&response),
                Ok(None) => IpcResponse::new(IpcStatus::Ok),
                Err(e) => ipc_device_error(e.to_string()),
            },
            Ok(None) => ipc_bad_request("missing sysinfo payload"),
            Err(e) => ipc_bad_request(e.to_string()),
        },
        IpcCommand::Reboot => match device.reboot() {
            Ok(Some(response)) => ipc_ok(&response),
            Ok(None) => IpcResponse::new(IpcStatus::Ok),
            Err(e) => ipc_device_error(e.to_string()),
        },
    }
}

fn summarize_screen_config(screen: &panorama_core::device::ScreenConfig) -> String {
    format!(
        "mode={} ratio={} media={:?} overlay1={:?} overlay2={:?}",
        screen.screen_mode,
        screen.ratio,
        screen.media,
        screen.sysinfo_display,
        screen.sysinfo_display2,
    )
}

fn send_badge_hardware_names_if_needed(
    device: &mut panorama_core::device::Device,
    monitor: &mut panorama_core::metrics::SystemMonitor,
    screen: &panorama_core::device::ScreenConfig,
) {
    let needs_badges = !screen.settings.badges.is_empty() || !screen.settings2.badges.is_empty();
    if !needs_badges {
        return;
    }
    let cpu_name = read_cpu_name();
    let gpu_name = read_gpu_name(monitor);
    let brightness = panorama_core::config::load_config()
        .unwrap_or_default()
        .brightness;
    if let Err(e) = device.send_full_config(screen, &cpu_name, &gpu_name, brightness, "Celsius") {
        tracing::warn!(
            "daemon: could not send full config with hardware names for badges: {}",
            e
        );
    } else {
        tracing::info!(
            "daemon: sent badge hardware names cpu='{}' gpu='{}'",
            cpu_name,
            gpu_name
        );
    }
}

fn read_gpu_name(monitor: &mut panorama_core::metrics::SystemMonitor) -> String {
    let metrics = monitor.poll();
    metrics
        .gpus
        .iter()
        .find(|gpu| {
            gpu.kind == panorama_core::metrics::GpuKind::Discrete
                && !gpu.name.trim().is_empty()
                && gpu.name != "GPU 0"
        })
        .or_else(|| {
            metrics
                .gpus
                .iter()
                .find(|gpu| !gpu.name.trim().is_empty() && gpu.name != "GPU 0")
        })
        .and_then(|gpu| clean_gpu_name(&gpu.name))
        .or_else(read_glxinfo_gpu_name)
        .or_else(read_lspci_gpu_name)
        .unwrap_or_else(|| "Unknown GPU".to_string())
}

fn read_glxinfo_gpu_name() -> Option<String> {
    let output = std::process::Command::new("glxinfo").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("OpenGL renderer string: "))
        .and_then(clean_gpu_name)
}

fn read_lspci_gpu_name() -> Option<String> {
    let output = std::process::Command::new("lspci").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("vga") || lower.contains("3d controller")
        })
        .and_then(|line| {
            line.split_once(": ")
                .and_then(|(_, name)| clean_gpu_name(name))
        })
}

fn clean_gpu_name(raw: &str) -> Option<String> {
    let mut name = raw
        .split('(')
        .next()
        .unwrap_or(raw)
        .split("/PCIe")
        .next()
        .unwrap_or(raw)
        .trim()
        .to_string();

    let bracket_names: Vec<&str> = raw
        .split('[')
        .filter_map(|part| part.split_once(']').map(|(inside, _)| inside.trim()))
        .collect();
    if let Some(marketing_name) = bracket_names.iter().rev().find(|candidate| {
        let lower = candidate.to_ascii_lowercase();
        (lower.contains("radeon") || lower.contains("geforce") || lower.contains("arc"))
            && !lower.contains("amd/ati")
    }) {
        name = (*marketing_name).to_string();
    }

    let lower = name.to_ascii_lowercase();
    if lower.starts_with("advanced micro devices") || lower.starts_with("nvidia corporation") {
        if let Some((_, suffix)) = name.split_once(']') {
            name = suffix.trim().to_string();
        }
    }

    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn ipc_ok<T: Serialize>(payload: &T) -> panorama_core::ipc::IpcResponse {
    panorama_core::ipc::IpcResponse::with_payload(panorama_core::ipc::IpcStatus::Ok, payload)
        .unwrap_or_else(|_| {
            panorama_core::ipc::IpcResponse::new(panorama_core::ipc::IpcStatus::InternalError)
        })
}

fn ipc_bad_request(message: impl Into<String>) -> panorama_core::ipc::IpcResponse {
    ipc_message(panorama_core::ipc::IpcStatus::BadRequest, message)
}

fn ipc_device_error(message: impl Into<String>) -> panorama_core::ipc::IpcResponse {
    ipc_message(panorama_core::ipc::IpcStatus::DeviceError, message)
}

fn ipc_message(
    status: panorama_core::ipc::IpcStatus,
    message: impl Into<String>,
) -> panorama_core::ipc::IpcResponse {
    panorama_core::ipc::IpcResponse::with_payload(
        status,
        &IpcMessagePayload {
            message: message.into(),
        },
    )
    .unwrap_or_else(|_| {
        panorama_core::ipc::IpcResponse::new(panorama_core::ipc::IpcStatus::InternalError)
    })
}

/// How long the cooler needs to settle after a screen reconfiguration before
/// it will answer further commands. Without this wait the loop's first
/// keepalive handshake gets no response.
const SCREEN_CONFIG_SETTLE: std::time::Duration = std::time::Duration::from_secs(2);
/// Shorter settle window for the daemon's overlay clear-then-set workaround.
/// We only need enough time for the firmware to absorb the cleared overlay
/// before the target screen config arrives.
const OVERLAY_CLEAR_SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// Run the foreground keepalive + metrics loop.
///
/// Holds the serial connection open and, every `keepalive_interval` seconds,
/// sends a handshake (the proven keepalive heartbeat) plus a sysinfo frame,
/// so the cooler keeps showing pushed media and live metrics instead of
/// reverting to its firmware default. Meant to run under a systemd user
/// service — see `packaging/panorama.service`. On a device error it exits
/// non-zero and lets systemd restart it (`Restart=on-failure`).
fn cmd_daemon() -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use panorama_core::config;
    use panorama_core::device::{Device, DisplaySettings, ScreenConfig};
    use panorama_core::metrics::SystemMonitor;

    init_tracing();

    let cfg = config::load_config().unwrap_or_default();
    let interval = keepalive_duration(&cfg);
    tracing::info!("daemon: starting (keepalive every {}s)", interval.as_secs());
    notify_service_manager(&[sd_notify::NotifyState::Status("Starting Panorama daemon")]);

    // SIGINT (Ctrl-C) and SIGTERM (`systemctl stop`) clear this flag.
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))
            .map_err(|e| anyhow::anyhow!("could not install signal handler: {}", e))?;
    }

    let mut ipc = None;

    let mut device = Device::new();
    let info = device.connect()?;
    tracing::info!("daemon: connected (firmware {})", info.firmware);

    // Re-apply persisted cooling settings so daemon startup restores the
    // configured fan behavior instead of keeping whatever the firmware was
    // last using.
    match device.set_fan_lcd(cfg.fan_lcd_percent) {
        Ok(_) => tracing::info!(
            "daemon: applied fan-lcd-percent={} from config",
            cfg.fan_lcd_percent
        ),
        Err(e) => {
            tracing::warn!(
                "daemon: could not apply fan-lcd-percent={} from config: {}",
                cfg.fan_lcd_percent,
                e
            );
        }
    }

    let mut monitor = SystemMonitor::new();
    // Discard the first poll — it has no delta baseline for CPU/network.
    let _ = monitor.poll();
    let mut last_state: Option<panorama_core::protocol::Response> = None;

    // Re-assert the last applied display so media + overlay survive a device
    // reset; the media files themselves already live on the cooler's storage.
    let state = config::load_state().unwrap_or_default();
    let is_split = state.screen_mode == "Screen Splitting";
    let mut active_screen = None;
    // In split mode either pane having media is enough to re-apply; in full
    // screen we need pane 1.
    let should_reapply = if is_split {
        !state.media.is_empty() || !state.media2.is_empty()
    } else {
        !state.media.is_empty()
    };
    if should_reapply {
        let appearance = DisplaySettings {
            color: state.metrics_color.clone(),
            align: state.metrics_align.clone(),
            position: state.metrics_position.clone(),
            badges: state.badges.clone(),
            filter_value: state.display_filter.clone(),
            filter_opacity: panorama_core::display::display_filter_opacity(&state.display_filter),
            ..DisplaySettings::default()
        };
        let appearance2 = DisplaySettings {
            badges: if is_split {
                state.badges2.clone()
            } else {
                Vec::new()
            },
            ..appearance.clone()
        };
        let media_payload = if is_split {
            // Empty string in either slot = dark pane (same wire format
            // cmd_display constructs).
            let pane1 = state.media.first().cloned().unwrap_or_default();
            let pane2 = state.media2.first().cloned().unwrap_or_default();
            vec![pane1, pane2]
        } else {
            state.media.clone()
        };
        let screen = ScreenConfig {
            media: media_payload,
            screen_mode: state.screen_mode.clone(),
            ratio: state.ratio.clone(),
            sysinfo_display: state.sysinfo_display.clone(),
            sysinfo_display2: if is_split {
                state.sysinfo_display2.clone()
            } else {
                Vec::new()
            },
            settings: appearance.clone(),
            settings2: appearance2,
            ..ScreenConfig::default()
        };
        match device.set_screen_config(&screen) {
            Ok(_) => {
                send_badge_hardware_names_if_needed(&mut device, &mut monitor, &screen);
                active_screen = Some(screen);
                tracing::info!(
                    "daemon: re-applied display (mode={}, pane1 overlay={}, pane2 overlay={})",
                    state.screen_mode,
                    state.sysinfo_display.len(),
                    state.sysinfo_display2.len(),
                );
                // Let the cooler settle after the screen reconfiguration
                // before publishing the IPC socket and before the loop sends
                // its first command.
                daemon_wait(
                    SCREEN_CONFIG_SETTLE,
                    &running,
                    None,
                    &mut device,
                    &info,
                    &mut monitor,
                    &mut active_screen,
                    &mut last_state,
                );
            }
            Err(e) => tracing::warn!("daemon: could not re-apply display: {}", e),
        }
    }

    if running.load(Ordering::SeqCst) {
        // Confirm the device is responsive after the initial display reapply
        // and settle window before publishing the IPC socket or claiming
        // readiness to systemd.
        let _ = device.handshake()?;

        // Publish the IPC socket only after the daemon is fully ready to serve.
        ipc = setup_ipc_listener()?;
        notify_service_manager(&[
            sd_notify::NotifyState::Status("Panorama daemon ready"),
            sd_notify::NotifyState::Ready,
        ]);
    }

    let result = loop {
        if let Some((listener, _)) = ipc.as_ref() {
            service_ipc(
                listener,
                &mut device,
                &info,
                &mut monitor,
                &mut active_screen,
                &mut last_state,
            );
        }

        // handshake() doubles as the keepalive heartbeat.
        if let Err(e) = device.handshake() {
            // A shutdown signal interrupts the in-flight serial read (EINTR);
            // that is a clean stop, not a device failure.
            if !running.load(Ordering::SeqCst) {
                break Ok(());
            }
            tracing::error!("daemon: keepalive failed: {}", e);
            break Err(e.into());
        }
        // The daemon only needs to push live metric values here. Overlay
        // labels themselves are already applied via full screen-config updates
        // at daemon startup and by foreground `pctl display` changes routed
        // through IPC. Re-sending `sysinfoDisplay` here caused stale labels to
        // persist on hardware instead of fully replacing the prior overlay.
        if let Some(screen) = active_screen.as_ref() {
            tracing::debug!(
                "daemon: keepalive using active screen: {}",
                summarize_screen_config(screen)
            );
        } else {
            tracing::debug!("daemon: keepalive using no active screen");
        }
        match device.send_sysinfo(&monitor.poll().to_sysinfo()) {
            Ok(Some(response)) => {
                last_state = Some(response);
                tracing::debug!("daemon: sysinfo sent");
            }
            Ok(None) => tracing::debug!("daemon: sysinfo sent"),
            Err(e) => {
                if !running.load(Ordering::SeqCst) {
                    break Ok(());
                }
                tracing::error!("daemon: send_sysinfo failed: {}", e);
                break Err(e.into());
            }
        }
        daemon_wait(
            interval,
            &running,
            ipc.as_ref().map(|(listener, _)| listener),
            &mut device,
            &info,
            &mut monitor,
            &mut active_screen,
            &mut last_state,
        );
        if !running.load(Ordering::SeqCst) {
            break Ok(());
        }
    };

    tracing::info!("daemon: shutting down");
    notify_service_manager(&[
        sd_notify::NotifyState::Status("Panorama daemon stopping"),
        sd_notify::NotifyState::Stopping,
    ]);
    device.disconnect();
    cleanup_ipc_socket(ipc.as_ref().map(|(_, path)| path.as_path()));
    result
}

/// Validate, convert if needed, and push one media file to the device.
/// Returns the remote filename on success. Prints "Converting…" / overwrite
/// notices but not the final success line — callers report that.
fn upload_file(adb: &panorama_core::adb::Adb, file: &str) -> Result<String> {
    use panorama_core::media::{self, MediaType};
    use std::path::Path;

    // Host file must exist and be a regular file
    if !Path::new(file).is_file() {
        anyhow::bail!("File not found: {}", file);
    }

    // Classify by extension
    let media_type = media::detect_type(file);
    if media_type == MediaType::Unknown {
        anyhow::bail!(
            "Unsupported file type: {}. Supported: \
             mp4/webm/mkv/avi/mov, gif, jpg/jpeg/png/bmp/webp",
            file
        );
    }

    // Convert non-MP4 video/GIF to MP4; otherwise pass through unchanged
    let (local_path, remote_name) = if media::needs_conversion(file) {
        if !media::is_ffmpeg_available() {
            anyhow::bail!("ffmpeg not found; install ffmpeg to upload {}", file);
        }

        let remote_name = media::get_converted_name(file);
        let local_path = media::tmp_file(&remote_name).to_string_lossy().into_owned();

        println!("Converting {}...", file);
        let ok = if media_type == MediaType::Gif {
            media::convert_gif_to_mp4(file, &local_path)
        } else {
            media::convert_to_mp4(file, &local_path)
        };
        if !ok {
            anyhow::bail!("Conversion failed for {}", file);
        }

        (local_path, remote_name)
    } else {
        (file.to_string(), media::get_filename(file))
    };

    if remote_name.is_empty() {
        anyhow::bail!("Could not derive a filename from {}", file);
    }

    if adb.file_exists(&remote_name) {
        println!("⚠ Overwriting existing {}", remote_name);
    }

    if adb.push(&local_path, &remote_name) {
        Ok(remote_name)
    } else {
        anyhow::bail!("Upload failed for {}", remote_name)
    }
}

/// Print `prompt`, read one line from stdin, and return `true` only for a
/// trimmed case-insensitive `y` / `yes`. Any read error (e.g. closed or
/// non-interactive stdin) fails safe and returns `false`.
fn confirm(prompt: &str) -> bool {
    use std::io::Write;

    print!("{}", prompt);
    if std::io::stdout().flush().is_err() {
        return false;
    }

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::apply_config_set;
    use panorama_core::config::Config;

    #[test]
    fn set_port_accepts_any_string() {
        let mut cfg = Config::default();
        apply_config_set(&mut cfg, "port", "/dev/ttyACM1").unwrap();
        assert_eq!(cfg.port, "/dev/ttyACM1");
    }

    #[test]
    fn set_brightness_within_range() {
        let mut cfg = Config::default();
        apply_config_set(&mut cfg, "brightness", "40").unwrap();
        assert_eq!(cfg.brightness, 40);
    }

    #[test]
    fn set_brightness_rejects_out_of_range() {
        let mut cfg = Config::default();
        assert!(apply_config_set(&mut cfg, "brightness", "150").is_err());
        // A rejected value must leave the field untouched.
        assert_eq!(cfg.brightness, Config::default().brightness);
    }

    #[test]
    fn set_brightness_rejects_negative() {
        let mut cfg = Config::default();
        assert!(apply_config_set(&mut cfg, "brightness", "-5").is_err());
    }

    #[test]
    fn set_brightness_rejects_non_integer() {
        let mut cfg = Config::default();
        assert!(apply_config_set(&mut cfg, "brightness", "bright").is_err());
    }

    #[test]
    fn set_keepalive_interval_accepts_positive() {
        let mut cfg = Config::default();
        apply_config_set(&mut cfg, "keepalive-interval", "15").unwrap();
        assert_eq!(cfg.keepalive_interval, 15);
    }

    #[test]
    fn set_keepalive_interval_rejects_zero() {
        let mut cfg = Config::default();
        assert!(apply_config_set(&mut cfg, "keepalive-interval", "0").is_err());
    }

    #[test]
    fn set_fan_lcd_percent_within_range() {
        let mut cfg = Config::default();
        apply_config_set(&mut cfg, "fan-lcd-percent", "55").unwrap();
        assert_eq!(cfg.fan_lcd_percent, 55);
    }

    #[test]
    fn set_fan_lcd_percent_rejects_out_of_range() {
        let mut cfg = Config::default();
        assert!(apply_config_set(&mut cfg, "fan-lcd-percent", "101").is_err());
    }

    #[test]
    fn set_unknown_key_errors() {
        let mut cfg = Config::default();
        let err = apply_config_set(&mut cfg, "bogus", "1").unwrap_err();
        assert!(err.to_string().contains("Unknown config key"));
    }

    #[test]
    fn clean_gpu_name_extracts_marketing_names() {
        assert_eq!(
            super::clean_gpu_name(
                "Advanced Micro Devices, Inc. [AMD/ATI] Navi 31 [Radeon RX 7900 XTX]"
            )
            .as_deref(),
            Some("Radeon RX 7900 XTX")
        );
        assert_eq!(
            super::clean_gpu_name("NVIDIA GeForce RTX 5090/PCIe/SSE2").as_deref(),
            Some("NVIDIA GeForce RTX 5090")
        );
    }

    // --- display --metrics ---------------------------------------------------

    use super::{
        format_delete_suggestion, likely_comma_separated_delete_files,
        validate_device_media_filename, Cli, Commands, DisplayFilter, ScreenRatio, SleepState,
    };
    use clap::Parser;
    use panorama_core::display::{parse_overlay_color, resolve_metric_tokens};

    fn toks(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn metric_tokens_resolve_to_device_labels() {
        let got = resolve_metric_tokens(&toks(&["cpu-temp", "gpu-temp"])).unwrap();
        assert_eq!(got, vec!["CPU Temperature", "GPU Temperature"]);
    }

    #[test]
    fn overlay_color_parses_hex_and_rgb() {
        assert_eq!(parse_overlay_color("#ff0000").unwrap(), "#FF0000");
        assert_eq!(parse_overlay_color("00ff00").unwrap(), "#00FF00");
        assert_eq!(parse_overlay_color("0,0,255").unwrap(), "#0000FF");
        assert_eq!(parse_overlay_color(" 255, 165, 0 ").unwrap(), "#FFA500");
    }

    #[test]
    fn overlay_color_rejects_bad_input() {
        assert!(parse_overlay_color("#fff").is_err()); // too short
        assert!(parse_overlay_color("gggggg").is_err()); // non-hex
        assert!(parse_overlay_color("300,0,0").is_err()); // component > 255
        assert!(parse_overlay_color("1,2").is_err()); // wrong arity
    }

    #[test]
    fn metric_tokens_datetime_is_valid() {
        let got = resolve_metric_tokens(&toks(&["datetime"])).unwrap();
        assert_eq!(got, vec!["Date&Time"]);
    }

    #[test]
    fn metric_tokens_empty_input_yields_empty() {
        assert!(resolve_metric_tokens(&[]).unwrap().is_empty());
    }

    #[test]
    fn metric_tokens_reject_unknown() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "bogus"])).unwrap_err();
        assert!(err.to_string().contains("unknown metric 'bogus'"));
    }

    #[test]
    fn metric_tokens_reject_duplicates() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "cpu-temp"])).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn metric_tokens_reject_more_than_three() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "gpu-temp", "cpu-freq", "mem-usage"]))
            .unwrap_err();
        assert!(err.to_string().contains("at most 3"));
    }

    #[test]
    fn delete_detects_comma_separated_media_files() {
        let got = likely_comma_separated_delete_files(&toks(&["clip1.mp4,clip2.mp4"])).unwrap();
        assert_eq!(got, vec!["clip1.mp4", "clip2.mp4"]);
    }

    #[test]
    fn delete_detects_comma_separated_globs() {
        let got = likely_comma_separated_delete_files(&toks(&["stats_*,logo_*"])).unwrap();
        assert_eq!(got, vec!["stats_*", "logo_*"]);
    }

    #[test]
    fn delete_allows_space_separated_args() {
        assert!(likely_comma_separated_delete_files(&toks(&["clip1.mp4", "clip2.mp4"])).is_none());
    }

    #[test]
    fn delete_allows_single_filename_containing_comma() {
        assert!(likely_comma_separated_delete_files(&toks(&["clip,final.mp4"])).is_none());
    }

    #[test]
    fn delete_suggestion_quotes_names_with_spaces() {
        let got = format_delete_suggestion(&toks(&[
            "Afro Samurai GIF by Funimation.mp4",
            "Catch Me If You Can.mp4",
        ]));
        let parsed = shlex::split(&got).unwrap();
        assert_eq!(
            parsed,
            toks(&[
                "Afro Samurai GIF by Funimation.mp4",
                "Catch Me If You Can.mp4"
            ])
        );
    }

    #[test]
    fn display_parses_file_only() {
        let cli = Cli::try_parse_from(["pctl", "display", "clip.mp4"]).unwrap();
        match cli.command {
            Commands::Display { file, metrics, .. } => {
                assert_eq!(file.as_deref(), Some("clip.mp4"));
                assert!(metrics.is_none());
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_media_filename_validation_accepts_plain_names() {
        validate_device_media_filename("clip.mp4").unwrap();
        validate_device_media_filename("Catch Me If You Can.png").unwrap();
    }

    #[test]
    fn display_media_filename_validation_rejects_local_paths() {
        let err = validate_device_media_filename("/tmp/clip.png").unwrap_err();
        assert!(err.to_string().contains("not a local path"));

        let err = validate_device_media_filename("subdir/clip.png").unwrap_err();
        assert!(err.to_string().contains("pctl upload <local-file>"));
    }

    #[test]
    fn sleep_parses_on_and_off() {
        for (arg, want_on) in [("on", true), ("off", false)] {
            let cli = Cli::try_parse_from(["pctl", "sleep", arg]).unwrap();
            match cli.command {
                Commands::Sleep { state } => {
                    assert_eq!(matches!(state, SleepState::On), want_on);
                }
                _ => panic!("expected Sleep"),
            }
        }
    }

    #[test]
    fn display_parses_metrics_with_no_values() {
        let cli = Cli::try_parse_from(["pctl", "display", "--metrics"]).unwrap();
        match cli.command {
            Commands::Display { file, metrics, .. } => {
                assert!(file.is_none());
                assert_eq!(metrics, Some(vec![]));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_file_and_metrics() {
        let cli = Cli::try_parse_from([
            "pctl",
            "display",
            "clip.mp4",
            "--metrics",
            "cpu-temp,gpu-temp",
        ])
        .unwrap();
        match cli.command {
            Commands::Display { file, metrics, .. } => {
                assert_eq!(file.as_deref(), Some("clip.mp4"));
                assert_eq!(metrics, Some(toks(&["cpu-temp", "gpu-temp"])));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_split_with_both_panes() {
        let cli = Cli::try_parse_from(["pctl", "display", "a.mp4", "--media2", "b.mp4", "--split"])
            .unwrap();
        match cli.command {
            Commands::Display {
                file,
                split,
                media2,
                ..
            } => {
                assert_eq!(file.as_deref(), Some("a.mp4"));
                assert!(split);
                assert_eq!(media2, Some(Some("b.mp4".into())));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_media2_with_no_value_as_clear() {
        let cli = Cli::try_parse_from(["pctl", "display", "a.mp4", "--split", "--media2"]).unwrap();
        match cli.command {
            Commands::Display { media2, split, .. } => {
                assert!(split);
                assert_eq!(media2, Some(None));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_ratio_one_to_one() {
        let cli = Cli::try_parse_from([
            "pctl", "display", "a.mp4", "--split", "--media2", "b.mp4", "--ratio", "1:1",
        ])
        .unwrap();
        match cli.command {
            Commands::Display { ratio, .. } => {
                assert!(matches!(ratio, Some(ScreenRatio::OneToOne)));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_ratio_two_to_one() {
        let cli = Cli::try_parse_from(["pctl", "display", "a.mp4", "--ratio", "2:1"]).unwrap();
        match cli.command {
            Commands::Display { ratio, .. } => {
                assert!(matches!(ratio, Some(ScreenRatio::TwoToOne)));
            }
            _ => panic!("expected Display"),
        }
    }

    #[test]
    fn display_parses_filter_effects() {
        for (arg, expected) in [
            ("none", DisplayFilter::None),
            ("smoke", DisplayFilter::Smoke),
            ("rain", DisplayFilter::Rain),
        ] {
            let cli = Cli::try_parse_from(["pctl", "display", "a.mp4", "--filter", arg]).unwrap();
            match cli.command {
                Commands::Display { filter, .. } => {
                    assert_eq!(
                        filter.map(DisplayFilter::as_device_str),
                        Some(expected.as_device_str())
                    );
                }
                _ => panic!("expected Display"),
            }
        }
    }

    #[test]
    fn display_rejects_invalid_filter() {
        let err = match Cli::try_parse_from(["pctl", "display", "a.mp4", "--filter", "fog"]) {
            Ok(_) => panic!("expected parse error for unsupported --filter value"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("smoke") && err.contains("rain") && err.contains("none"));
    }

    #[test]
    fn display_rejects_invalid_ratio() {
        // Cli doesn't implement Debug, so `.unwrap_err()` doesn't compile;
        // pattern-match the result instead.
        let err = match Cli::try_parse_from(["pctl", "display", "a.mp4", "--ratio", "4:3"]) {
            Ok(_) => panic!("expected parse error for unsupported --ratio value"),
            Err(e) => e.to_string(),
        };
        // clap surfaces a "possible values" hint for ValueEnum failures.
        assert!(err.contains("1:1") && err.contains("2:1"));
    }

    #[test]
    fn display_parses_split_with_paired_metrics() {
        let cli = Cli::try_parse_from([
            "pctl",
            "display",
            "a.mp4",
            "--split",
            "--media2",
            "b.mp4",
            "--metrics",
            "cpu-temp,gpu-temp",
            "--metrics2",
            "mem-usage",
        ])
        .unwrap();
        match cli.command {
            Commands::Display {
                metrics, metrics2, ..
            } => {
                assert_eq!(metrics, Some(toks(&["cpu-temp", "gpu-temp"])));
                assert_eq!(metrics2, Some(toks(&["mem-usage"])));
            }
            _ => panic!("expected Display"),
        }
    }

    // --- daemon --------------------------------------------------------------

    use super::{interruptible_sleep, ipc_response_message, keepalive_duration};
    use panorama_core::ipc::IpcStatus;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[test]
    fn keepalive_duration_uses_config_seconds() {
        let mut cfg = Config::default();
        assert_eq!(keepalive_duration(&cfg), Duration::from_secs(2));
        cfg.keepalive_interval = 3;
        assert_eq!(keepalive_duration(&cfg), Duration::from_secs(3));
    }

    #[test]
    fn keepalive_duration_clamps_to_at_least_one_second() {
        let cfg = Config {
            keepalive_interval: 0,
            ..Config::default()
        };
        assert_eq!(keepalive_duration(&cfg), Duration::from_secs(1));
        let cfg = Config {
            keepalive_interval: -5,
            ..Config::default()
        };
        assert_eq!(keepalive_duration(&cfg), Duration::from_secs(1));
    }

    #[test]
    fn interruptible_sleep_returns_early_when_flag_cleared() {
        let running = AtomicBool::new(false);
        let start = Instant::now();
        interruptible_sleep(Duration::from_secs(10), &running);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn interruptible_sleep_runs_full_duration_when_flag_set() {
        let running = AtomicBool::new(true);
        let start = Instant::now();
        interruptible_sleep(Duration::from_millis(120), &running);
        assert!(start.elapsed() >= Duration::from_millis(120));
    }

    #[test]
    fn ipc_response_message_prefers_payload_message() {
        let response = panorama_core::ipc::IpcResponse::with_payload(
            IpcStatus::DeviceError,
            &super::IpcMessagePayload {
                message: "boom".to_string(),
            },
        )
        .unwrap();
        assert_eq!(ipc_response_message(&response), "boom");
    }

    #[test]
    fn daemon_command_parses() {
        let cli = Cli::try_parse_from(["pctl", "daemon"]).unwrap();
        assert!(matches!(cli.command, Commands::Daemon));
    }

    #[test]
    fn setup_command_parses() {
        let cli = Cli::try_parse_from(["pctl", "setup"]).unwrap();
        assert!(matches!(cli.command, Commands::Setup));
    }

    #[test]
    fn udev_rule_content_has_expected_match_patterns() {
        // include_str! bakes packaging/70-tryx-panorama.rules into the binary;
        // if the path drifts or the file is corrupted, this test will catch
        // the loss of the expected udev match clauses.
        use super::UDEV_RULE_CONTENT;
        assert!(UDEV_RULE_CONTENT.contains("idVendor"));
        assert!(UDEV_RULE_CONTENT.contains("18d1"));
        assert!(UDEV_RULE_CONTENT.contains("2d0[0-5]"));
        assert!(UDEV_RULE_CONTENT.contains("uaccess"));
        assert!(UDEV_RULE_CONTENT.contains("SUBSYSTEM==\"usb\""));
        assert!(UDEV_RULE_CONTENT.contains("SUBSYSTEM==\"tty\""));
    }

    #[test]
    fn package_managed_setup_assets_present_when_packaged_udev_rule_exists() {
        assert!(super::package_managed_setup_assets_present_with(
            true, false
        ));
    }

    #[test]
    fn package_managed_setup_assets_present_when_packaged_user_service_exists() {
        assert!(super::package_managed_setup_assets_present_with(
            false, true
        ));
    }

    #[test]
    fn package_managed_setup_assets_present_is_false_when_no_package_assets_exist() {
        assert!(!super::package_managed_setup_assets_present_with(
            false, false
        ));
    }

    #[test]
    fn fan_rpm_command_parses_single_shot() {
        let cli = Cli::try_parse_from(["pctl", "fan-rpm"]).unwrap();
        assert!(matches!(cli.command, Commands::FanRpm { watch: None }));
    }

    #[test]
    fn fan_rpm_command_parses_watch_interval() {
        let cli = Cli::try_parse_from(["pctl", "fan-rpm", "--watch", "5"]).unwrap();
        assert!(matches!(cli.command, Commands::FanRpm { watch: Some(5) }));
    }
}
