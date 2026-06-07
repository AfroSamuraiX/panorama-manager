//! Live-hardware smoke tests. All `#[ignore]`d by default so `cargo test`
//! stays green without a device.
//!
//! Run when the Panorama cooler is plugged in:
//!
//! ```bash
//! cargo test --test live_device -- --ignored --nocapture
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use panorama_core::adb::{Adb, ConfirmationKind, DeviceValidation};
use panorama_core::device::Device;
use panorama_core::metrics::SystemMonitor;
use panorama_core::protocol::{build_frame, parse_response};
use panorama_core::transport::{list_panorama_candidates, SerialTransport};

#[test]
#[ignore = "requires Panorama hardware connected via USB"]
fn discovery_finds_at_least_one_serial_candidate() {
    let candidates = list_panorama_candidates().expect("serial port enumeration failed");

    println!("Found {} serial candidate(s):", candidates.len());
    for c in &candidates {
        // Serial intentionally omitted: per-unit identifier, treat as sensitive.
        println!(
            "  port={} vid={:#06x} pid={:#06x} manufacturer={:?} product={:?}",
            c.port_name, c.vid, c.pid, c.manufacturer, c.product
        );
    }

    assert!(
        !candidates.is_empty(),
        "no /dev/ttyACM* ports from vendor 0x18d1 — is the cooler connected \
         and is the cdc_acm driver bound?"
    );
}

#[test]
#[ignore = "requires Panorama hardware connected via USB"]
fn adb_validate_device_confirms_panorama() {
    let adb = Adb::new();
    let result = adb.validate_device();
    println!("validation result: {:?}", result);

    match result {
        DeviceValidation::Confirmed(ConfirmationKind::MediaPathPresent) => {
            println!("✓ Panorama confirmed via /sdcard/pcMedia/ filesystem check");
        }
        DeviceValidation::Confirmed(ConfirmationKind::ProductInfoMatched) => {
            println!("✓ Panorama confirmed via getprop (fresh device — no media pushed yet)");
        }
        DeviceValidation::NotConnected => {
            panic!("adb sees no connected device — is the cooler plugged in and authorized?");
        }
        DeviceValidation::NotPanorama { detected } => {
            panic!(
                "adb sees a device that doesn't match Panorama signatures (detected: {})",
                detected
            );
        }
    }
}

#[test]
#[ignore = "requires Panorama hardware connected via USB"]
fn device_connect_and_handshake_returns_info() {
    let mut device = Device::new();
    let info = device.connect().expect("Device::connect failed");
    assert!(device.is_connected());

    println!("handshake info:");
    println!("  product_id:  {}", info.product_id);
    println!("  os:          {}", info.os);
    // Serial intentionally omitted: per-unit identifier, treat as sensitive.
    println!("  app_version: {}", info.app_version);
    println!("  firmware:    {}", info.firmware);
    println!("  hardware:    {}", info.hardware);
    println!("  attributes:  {:?}", info.attributes);

    assert!(
        !info.product_id.is_empty(),
        "handshake should populate product_id (was empty)"
    );
}

#[test]
#[ignore = "requires Panorama hardware connected via USB"]
fn system_monitor_polls_and_sends_sysinfo_to_device() {
    let mut monitor = SystemMonitor::new();
    // First poll establishes the delta baseline; second poll yields real
    // CPU usage and network speed numbers.
    let _ = monitor.poll();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let metrics = monitor.poll();

    println!("collected metrics:");
    println!(
        "  cpu:  {:.0}°C  {:.0}%  {:.0} MHz  ({} cores)",
        metrics.cpu.temperature,
        metrics.cpu.usage_percent,
        metrics.cpu.frequency_mhz,
        metrics.cpu.core_count
    );
    for (i, gpu) in metrics.gpus.iter().enumerate() {
        println!(
            "  gpu{}: {:.0}°C  {:.0}%  {:.0} MHz  {} mV  ({}/{} MB VRAM)",
            i,
            gpu.temperature,
            gpu.usage_percent,
            gpu.frequency_mhz,
            gpu.voltage_mv,
            gpu.vram_used_mb,
            gpu.vram_total_mb
        );
    }
    println!(
        "  ram:  {}/{} MB  ({}%)",
        metrics.ram.used_mb, metrics.ram.total_mb, metrics.ram.usage_percent
    );
    println!(
        "  disk: {}/{} GB ({}%)",
        metrics.disk.used_gb, metrics.disk.total_gb, metrics.disk.usage_percent
    );
    println!(
        "  net:  rx {:.1} kB/s  tx {:.1} kB/s",
        metrics.net.rx_speed_kbs, metrics.net.tx_speed_kbs
    );

    let sysinfo = metrics.to_sysinfo();
    println!("→ sending {} sysinfo entries", sysinfo.len());

    let mut device = Device::new();
    device.connect().expect("Device::connect failed");
    let response = device
        .send_sysinfo(&sysinfo)
        .expect("send_sysinfo failed")
        .expect("no response from device");

    println!("device responded: {} {}", response.version, response.status);
    if let Some(json) = &response.json {
        if let Some(status) = json.get("status") {
            println!("  status: {}", status);
        }
    }
    assert_eq!(response.status, "200");
}

#[test]
#[ignore = "requires Panorama hardware connected via USB"]
fn handshake_round_trips_a_state_all_request() {
    let mut transport = SerialTransport::open()
        .expect("SerialTransport::open() failed — check the cooler is connected");

    // Stub metrics body — the device responds to the request regardless of
    // metric values, so all zeros is fine for a connectivity check.
    let body = r#"{"network":{"upload":0,"download":0},"memory":{"total":0,"used":0,"load":0,"temperature":0,"speed":0},"cpu":{"load":0,"temperature":0,"speedAverage":0,"power":0,"voltage":0,"usage":0},"gpu":{"load":0,"temperature":0,"fan":0,"speed":0,"power":0,"voltage":0},"disk":{"total":0,"used":0,"load":0,"activity":0,"temperature":0,"readSpeed":0,"writeSpeed":0},"fans":[],"motherboard":{"temperature":0,"pchTemperature":0},"timestamp":0}"#;

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before 1970?")
        .as_millis() as i64;

    let frame = build_frame("STATE", "all", body, "1", 1, now_ms);
    println!("→ sending {} bytes", frame.len());
    transport.send(&frame).expect("send() failed");

    let resp_bytes = transport.receive(4096).expect("receive() failed");
    println!("← received {} bytes", resp_bytes.len());

    let parsed = parse_response(&resp_bytes).expect("response did not parse");
    println!("status: {} {}", parsed.version, parsed.status);
    if let Some(json) = &parsed.json {
        println!(
            "body:\n{}",
            serde_json::to_string_pretty(json).unwrap_or_default()
        );
    }

    assert_eq!(parsed.version, "1");
    assert_eq!(parsed.status, "200", "device did not respond with 200");
}
