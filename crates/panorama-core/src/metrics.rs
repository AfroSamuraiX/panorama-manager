//! Linux system metrics collection from procfs and sysfs.
//!
//! All values are read best-effort — missing files, unreadable values, and
//! unsupported hardware (Intel/Nvidia GPU sensors) yield zeros rather than
//! errors. The exact subset of metrics this fills was driven by what the
//! TRYX Panorama display understands; see [`SystemMetrics::to_sysinfo`].
//!
//! ## Hardware coverage
//!
//! - CPU temperature: AMD Ryzen via `k10temp`/`zenpower`, Intel via `coretemp`.
//! - GPU metrics:
//!   - AMD via the `amdgpu` driver under `/sys/class/drm/cardN`.
//!   - Nvidia via `nvidia-smi` shell-out (requires the proprietary driver,
//!     same pattern as the `adb`/`ffmpeg` runtime deps).
//! - RAM, disk usage, network speed: vendor-agnostic via procfs/statvfs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::device::SysinfoData;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuMetrics {
    pub temperature: f64,
    pub usage_percent: f64,
    pub frequency_mhz: f64,
    pub core_count: u32,
}

/// Whether a GPU is a discrete card or integrated into the CPU package.
/// Drives [`SystemMetrics::to_sysinfo`] selection — discrete is preferred.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GpuKind {
    /// Dedicated discrete graphics card.
    Discrete,
    /// Integrated GPU sharing the CPU package (an APU iGPU).
    #[default]
    Integrated,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuMetrics {
    pub name: String,
    pub kind: GpuKind,
    pub temperature: f64,
    pub usage_percent: f64,
    pub frequency_mhz: f64,
    pub voltage_mv: f64,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RamMetrics {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_percent: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetMetrics {
    pub rx_speed_kbs: f64,
    pub tx_speed_kbs: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskMetrics {
    pub total_gb: u64,
    pub used_gb: u64,
    pub usage_percent: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemMetrics {
    pub cpu: CpuMetrics,
    pub gpus: Vec<GpuMetrics>,
    pub ram: RamMetrics,
    pub net: NetMetrics,
    pub disk: DiskMetrics,
}

impl SystemMetrics {
    /// Convert collected metrics into the labeled [`SysinfoData`] list the
    /// Panorama display expects. Labels match those `Device::send_sysinfo`
    /// recognises; unknown ones are silently dropped device-side.
    pub fn to_sysinfo(&self) -> Vec<SysinfoData> {
        let mut out = Vec::new();

        // CPU
        out.push(SysinfoData {
            label: "CPU Temperature".into(),
            value: format!("{:.0}", self.cpu.temperature),
            unit: "°C".into(),
        });
        out.push(SysinfoData {
            label: "CPU Usage".into(),
            value: format!("{:.0}", self.cpu.usage_percent),
            unit: "%".into(),
        });
        out.push(SysinfoData {
            label: "CPU Frequency".into(),
            value: format!("{:.0}", self.cpu.frequency_mhz),
            unit: "MHz".into(),
        });

        // GPU — prefer a discrete card, fall back to integrated. When no GPU
        // is detected at all, the labels still emit with zero values.
        let fallback = GpuMetrics::default();
        let gpu = self
            .gpus
            .iter()
            .find(|g| g.kind == GpuKind::Discrete)
            .or_else(|| self.gpus.first())
            .unwrap_or(&fallback);
        out.push(SysinfoData {
            label: "GPU Temperature".into(),
            value: format!("{:.0}", gpu.temperature),
            unit: "°C".into(),
        });
        out.push(SysinfoData {
            label: "GPU Usage".into(),
            value: format!("{:.0}", gpu.usage_percent),
            unit: "%".into(),
        });
        out.push(SysinfoData {
            label: "GPU Frequency".into(),
            value: format!("{:.0}", gpu.frequency_mhz),
            unit: "MHz".into(),
        });
        out.push(SysinfoData {
            label: "GPU Voltage".into(),
            value: format!("{:.0}", gpu.voltage_mv),
            unit: "mV".into(),
        });

        // RAM
        out.push(SysinfoData {
            label: "Memory Utilization".into(),
            value: format!("{}", self.ram.usage_percent),
            unit: "%".into(),
        });

        out
    }
}

pub struct SystemMonitor {
    core_count: u32,
    prev_cpu_idle: u64,
    prev_cpu_total: u64,
    prev_rx_bytes: u64,
    prev_tx_bytes: u64,
    prev_net_at: Option<Instant>,
}

impl SystemMonitor {
    pub fn new() -> Self {
        Self {
            core_count: read_cpu_core_count().max(1),
            prev_cpu_idle: 0,
            prev_cpu_total: 0,
            prev_rx_bytes: 0,
            prev_tx_bytes: 0,
            prev_net_at: None,
        }
    }

    /// Sample all metrics once. First call after `new()` returns 0 for any
    /// delta-derived value (CPU usage, network speed) because there is no
    /// baseline yet — subsequent calls produce real values.
    pub fn poll(&mut self) -> SystemMetrics {
        SystemMetrics {
            cpu: self.read_cpu(),
            gpus: read_gpu_metrics(),
            ram: read_ram_metrics(),
            net: self.read_net(),
            disk: read_disk_metrics(),
        }
    }

    fn read_cpu(&mut self) -> CpuMetrics {
        let usage_percent = match fs::read_to_string("/proc/stat") {
            Ok(text) => match parse_proc_stat_cpu(&text) {
                Some((idle, total)) => {
                    let delta_idle = idle.saturating_sub(self.prev_cpu_idle);
                    let delta_total = total.saturating_sub(self.prev_cpu_total);
                    self.prev_cpu_idle = idle;
                    self.prev_cpu_total = total;
                    if delta_total == 0 {
                        0.0
                    } else {
                        (1.0 - (delta_idle as f64 / delta_total as f64)) * 100.0
                    }
                }
                None => 0.0,
            },
            Err(_) => 0.0,
        };

        CpuMetrics {
            temperature: read_cpu_temperature(),
            usage_percent,
            frequency_mhz: read_cpu_frequency(),
            core_count: self.core_count,
        }
    }

    fn read_net(&mut self) -> NetMetrics {
        let now = Instant::now();
        let (rx, tx) = match fs::read_to_string("/proc/net/dev") {
            Ok(text) => parse_net_dev_totals(&text),
            Err(_) => (0, 0),
        };

        let speed = if let Some(prev_at) = self.prev_net_at {
            let dt = now.duration_since(prev_at).as_secs_f64();
            if dt > 0.0 {
                NetMetrics {
                    rx_speed_kbs: (rx.saturating_sub(self.prev_rx_bytes)) as f64 / 1024.0 / dt,
                    tx_speed_kbs: (tx.saturating_sub(self.prev_tx_bytes)) as f64 / 1024.0 / dt,
                }
            } else {
                NetMetrics::default()
            }
        } else {
            NetMetrics::default()
        };

        self.prev_rx_bytes = rx;
        self.prev_tx_bytes = tx;
        self.prev_net_at = Some(now);

        speed
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// --- Hardware-touching readers ---

fn read_sys_file(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn find_hwmon_by_name(target: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name_file = path.join("name");
        if read_sys_file(&name_file).as_deref() == Some(target) {
            return Some(path);
        }
    }
    None
}

fn read_cpu_temperature() -> f64 {
    let hwmon = find_hwmon_by_name("k10temp")
        .or_else(|| find_hwmon_by_name("zenpower"))
        .or_else(|| find_hwmon_by_name("coretemp"));
    let path = match hwmon {
        Some(p) => p,
        None => return 0.0,
    };
    read_sys_file(path.join("temp1_input"))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|raw_millis| raw_millis / 1000.0)
        .unwrap_or(0.0)
}

fn read_cpu_frequency() -> f64 {
    let entries = match fs::read_dir("/sys/devices/system/cpu") {
        Ok(e) => e,
        Err(_) => return 0.0,
    };
    let (sum_khz, count) = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if !name.starts_with("cpu")
                || !name[3..].chars().all(|c| c.is_ascii_digit())
                || name.len() <= 3
            {
                return None;
            }
            read_sys_file(e.path().join("cpufreq/scaling_cur_freq"))?
                .parse::<f64>()
                .ok()
        })
        .fold((0.0, 0u32), |(sum, n), khz| (sum + khz, n + 1));
    if count == 0 {
        0.0
    } else {
        (sum_khz / count as f64) / 1000.0 // kHz → MHz
    }
}

fn read_cpu_core_count() -> u32 {
    fs::read_dir("/sys/devices/system/cpu")
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("cpu")
                        && name.len() > 3
                        && name[3..].chars().all(|c| c.is_ascii_digit())
                })
                .count() as u32
        })
        .unwrap_or(0)
}

fn read_gpu_metrics() -> Vec<GpuMetrics> {
    let mut gpus = read_amd_gpu_metrics();
    gpus.extend(read_nvidia_gpu_metrics());
    gpus
}

fn read_amd_gpu_metrics() -> Vec<GpuMetrics> {
    let entries = match fs::read_dir("/sys/class/drm") {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut gpus = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let card_dir = entry.path().join("device");
        let busy_path = card_dir.join("gpu_busy_percent");
        if !busy_path.exists() {
            continue;
        }

        let mut gpu = GpuMetrics {
            name: read_sys_file(card_dir.join("product_name"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("GPU {}", &name[4..])),
            ..Default::default()
        };

        gpu.usage_percent = read_sys_file(&busy_path)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        // Temperature from any hwmon* under the device dir.
        if let Ok(hw_entries) = fs::read_dir(card_dir.join("hwmon")) {
            for hw in hw_entries.flatten() {
                if let Some(v) =
                    read_sys_file(hw.path().join("temp1_input")).and_then(|s| s.parse::<f64>().ok())
                {
                    gpu.temperature = v / 1000.0;
                    break;
                }
            }
            // Voltage from in0_input (millivolts) on any hwmon node.
            if let Ok(hw_entries) = fs::read_dir(card_dir.join("hwmon")) {
                for hw in hw_entries.flatten() {
                    if let Some(v) = read_sys_file(hw.path().join("in0_input"))
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        gpu.voltage_mv = v;
                        break;
                    }
                }
            }
        }

        if let Some(sclk) = read_sys_file(card_dir.join("pp_dpm_sclk")) {
            if let Some(freq) = parse_amdgpu_active_sclk(&sclk) {
                gpu.frequency_mhz = freq;
            }
        }

        if let Some(used) =
            read_sys_file(card_dir.join("mem_info_vram_used")).and_then(|s| s.parse::<u64>().ok())
        {
            gpu.vram_used_mb = used / (1024 * 1024);
        }
        if let Some(total) =
            read_sys_file(card_dir.join("mem_info_vram_total")).and_then(|s| s.parse::<u64>().ok())
        {
            gpu.vram_total_mb = total / (1024 * 1024);
        }

        // A discrete card carries its own large VRAM; an APU iGPU only has a
        // small BIOS-reserved UMA carve-out. 4 GiB cleanly splits the two.
        gpu.kind = if gpu.vram_total_mb >= 4096 {
            GpuKind::Discrete
        } else {
            GpuKind::Integrated
        };

        gpus.push(gpu);
    }
    gpus
}

fn read_nvidia_gpu_metrics() -> Vec<GpuMetrics> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,temperature.gpu,utilization.gpu,clocks.current.graphics,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_nvidia_smi_csv(&String::from_utf8_lossy(&output.stdout))
}

fn read_ram_metrics() -> RamMetrics {
    let text = match fs::read_to_string("/proc/meminfo") {
        Ok(t) => t,
        Err(_) => return RamMetrics::default(),
    };
    let info = parse_meminfo(&text);
    let total_kb = info.get("MemTotal").copied().unwrap_or(0);
    let avail_kb = info.get("MemAvailable").copied().unwrap_or(0);
    let total_mb = total_kb / 1024;
    let available_mb = avail_kb / 1024;
    let used_mb = total_mb.saturating_sub(available_mb);
    let usage_percent = if total_mb == 0 {
        0
    } else {
        ((used_mb as f64 / total_mb as f64) * 100.0).round() as u32
    };
    RamMetrics {
        total_mb,
        used_mb,
        available_mb,
        usage_percent,
    }
}

fn read_disk_metrics() -> DiskMetrics {
    let total = fs4::total_space("/").unwrap_or(0);
    let avail = fs4::available_space("/").unwrap_or(0);
    let total_gb = total / (1024 * 1024 * 1024);
    let avail_gb = avail / (1024 * 1024 * 1024);
    let used_gb = total_gb.saturating_sub(avail_gb);
    let usage_percent = if total_gb == 0 {
        0
    } else {
        ((used_gb as f64 / total_gb as f64) * 100.0).round() as u32
    };
    DiskMetrics {
        total_gb,
        used_gb,
        usage_percent,
    }
}

// --- Pure parsers (unit-testable) ---

/// Parse the first `cpu ...` line of `/proc/stat` into `(idle_jiffies, total_jiffies)`.
/// Both values are cumulative since boot — caller computes deltas between samples.
fn parse_proc_stat_cpu(text: &str) -> Option<(u64, u64)> {
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let user = parts.next()?.parse::<u64>().ok()?;
    let nice = parts.next()?.parse::<u64>().ok()?;
    let system = parts.next()?.parse::<u64>().ok()?;
    let idle = parts.next()?.parse::<u64>().ok()?;
    let iowait = parts.next()?.parse::<u64>().ok()?;
    let irq = parts.next()?.parse::<u64>().ok()?;
    let softirq = parts.next()?.parse::<u64>().ok()?;
    let total_idle = idle + iowait;
    let total = user + nice + system + idle + iowait + irq + softirq;
    Some((total_idle, total))
}

/// Parse `/proc/meminfo` into a `name → kB` map. Lines that don't match
/// `Name:  N kB` (or `Name:  N`) are silently skipped.
fn parse_meminfo(text: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let key = match parts.next() {
            Some(k) => k.trim_end_matches(':'),
            None => continue,
        };
        if let Some(value) = parts.next().and_then(|v| v.parse::<u64>().ok()) {
            out.insert(key.to_string(), value);
        }
    }
    out
}

/// Sum `(rx_bytes, tx_bytes)` across all non-loopback interfaces in
/// `/proc/net/dev` output.
fn parse_net_dev_totals(text: &str) -> (u64, u64) {
    let mut rx_total = 0_u64;
    let mut tx_total = 0_u64;
    for line in text.lines() {
        let line = line.trim();
        let (iface, rest) = match line.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        if iface.trim() == "lo" {
            continue;
        }
        let fields: Vec<&str> = rest.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        // /proc/net/dev column order: rx_bytes ... (col 0) ... tx_bytes (col 8)
        if let Ok(rx) = fields[0].parse::<u64>() {
            rx_total += rx;
        }
        if let Ok(tx) = fields[8].parse::<u64>() {
            tx_total += tx;
        }
    }
    (rx_total, tx_total)
}

/// Parse `nvidia-smi --query-gpu=...,--format=csv,noheader,nounits` output
/// into one [`GpuMetrics`] per row. Unparseable numeric fields default to 0
/// (e.g. `[Not Supported]` from older drivers). nvidia-smi does not
/// reliably expose voltage, so [`GpuMetrics::voltage_mv`] is always 0 here.
fn parse_nvidia_smi_csv(text: &str) -> Vec<GpuMetrics> {
    text.lines().filter_map(parse_nvidia_smi_row).collect()
}

fn parse_nvidia_smi_row(line: &str) -> Option<GpuMetrics> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 6 || parts[0].is_empty() {
        return None;
    }
    Some(GpuMetrics {
        name: parts[0].to_string(),
        // Desktop NVIDIA cards are always discrete.
        kind: GpuKind::Discrete,
        temperature: parts[1].parse().unwrap_or(0.0),
        usage_percent: parts[2].parse().unwrap_or(0.0),
        frequency_mhz: parts[3].parse().unwrap_or(0.0),
        voltage_mv: 0.0,
        vram_used_mb: parts[4].parse().unwrap_or(0),
        vram_total_mb: parts[5].parse().unwrap_or(0),
    })
}

/// Pick the active frequency (marked with `*`) from an amdgpu `pp_dpm_sclk` blob.
/// Example line: `1: 1000Mhz *`.
fn parse_amdgpu_active_sclk(text: &str) -> Option<f64> {
    for line in text.lines() {
        if !line.contains('*') {
            continue;
        }
        // Find the digit run before "Mhz"/"MHz".
        let lower = line.to_ascii_lowercase();
        let mhz_pos = lower.find("mhz")?;
        let prefix = &line[..mhz_pos];
        let digits: String = prefix
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return digits.parse::<f64>().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proc_stat_extracts_idle_and_total() {
        let sample = "cpu  3357 0 4313 1362393 192 50 5 0 0 0\n\
                      cpu0 800 0 1000 340000 50 10 1 0 0 0\n";
        let (idle, total) = parse_proc_stat_cpu(sample).unwrap();
        // idle + iowait = 1362393 + 192
        assert_eq!(idle, 1_362_585);
        // user+nice+system+idle+iowait+irq+softirq = 3357+0+4313+1362393+192+50+5
        assert_eq!(total, 1_370_310);
    }

    #[test]
    fn parse_proc_stat_returns_none_when_first_line_not_cpu() {
        let sample = "intr 12345\ncpu 1 2 3 4 5 6 7\n";
        assert!(parse_proc_stat_cpu(sample).is_none());
    }

    #[test]
    fn parse_proc_stat_returns_none_when_fields_missing() {
        let sample = "cpu 1 2 3\n";
        assert!(parse_proc_stat_cpu(sample).is_none());
    }

    #[test]
    fn parse_meminfo_extracts_known_keys() {
        let sample = "MemTotal:       16384000 kB\n\
                      MemFree:         1234567 kB\n\
                      MemAvailable:    8192000 kB\n\
                      Cached:          5000000 kB\n";
        let info = parse_meminfo(sample);
        assert_eq!(info.get("MemTotal").copied(), Some(16_384_000));
        assert_eq!(info.get("MemAvailable").copied(), Some(8_192_000));
        assert_eq!(info.get("Cached").copied(), Some(5_000_000));
    }

    #[test]
    fn parse_meminfo_skips_malformed_lines() {
        let sample = "MemTotal:       16384000 kB\n\
                      Garbage line without colon\n\
                      MemFree:         abc kB\n\
                      MemAvailable:    8192000 kB\n";
        let info = parse_meminfo(sample);
        assert_eq!(info.get("MemTotal").copied(), Some(16_384_000));
        assert!(!info.contains_key("MemFree")); // unparseable value
        assert_eq!(info.get("MemAvailable").copied(), Some(8_192_000));
    }

    #[test]
    fn parse_net_dev_sums_non_loopback_rx_tx() {
        let sample = "Inter-|   Receive                                                |  Transmit\n\
                      face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
                          lo: 100 1 0 0 0 0 0 0 200 2 0 0 0 0 0 0\n\
                        eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n\
                        wlan0: 5000 50 0 0 0 0 0 0 3000 30 0 0 0 0 0 0\n";
        let (rx, tx) = parse_net_dev_totals(sample);
        // lo skipped; eth0 + wlan0
        assert_eq!(rx, 1000 + 5000);
        assert_eq!(tx, 2000 + 3000);
    }

    #[test]
    fn parse_net_dev_handles_no_data_interfaces_gracefully() {
        let sample = "Inter-|...\nface |...\n  lo: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n";
        let (rx, tx) = parse_net_dev_totals(sample);
        assert_eq!(rx, 0);
        assert_eq!(tx, 0);
    }

    #[test]
    fn parse_net_dev_ignores_lines_without_colon() {
        let sample = "Inter-|...\nface |...\n";
        let (rx, tx) = parse_net_dev_totals(sample);
        assert_eq!((rx, tx), (0, 0));
    }

    #[test]
    fn parse_amdgpu_sclk_extracts_starred_frequency() {
        let sample = "0: 500Mhz\n\
                      1: 1000Mhz *\n\
                      2: 1500Mhz\n";
        assert_eq!(parse_amdgpu_active_sclk(sample), Some(1000.0));
    }

    #[test]
    fn parse_amdgpu_sclk_handles_capitalized_units() {
        let sample = "0: 500MHz *\n";
        assert_eq!(parse_amdgpu_active_sclk(sample), Some(500.0));
    }

    #[test]
    fn parse_amdgpu_sclk_returns_none_when_no_active_marker() {
        let sample = "0: 500Mhz\n1: 1000Mhz\n";
        assert_eq!(parse_amdgpu_active_sclk(sample), None);
    }

    #[test]
    fn parse_amdgpu_sclk_returns_none_for_empty_input() {
        assert_eq!(parse_amdgpu_active_sclk(""), None);
    }

    #[test]
    fn parse_nvidia_smi_extracts_gpu_fields() {
        let sample = "NVIDIA GeForce RTX 5090, 45, 12, 1500, 1024, 24576\n";
        let gpus = parse_nvidia_smi_csv(sample);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 5090");
        assert_eq!(gpus[0].temperature, 45.0);
        assert_eq!(gpus[0].usage_percent, 12.0);
        assert_eq!(gpus[0].frequency_mhz, 1500.0);
        assert_eq!(gpus[0].vram_used_mb, 1024);
        assert_eq!(gpus[0].vram_total_mb, 24576);
        assert_eq!(gpus[0].kind, GpuKind::Discrete);
        // nvidia-smi doesn't reliably expose voltage.
        assert_eq!(gpus[0].voltage_mv, 0.0);
    }

    #[test]
    fn parse_nvidia_smi_handles_multiple_gpus() {
        let sample = "Tesla V100, 60, 80, 1380, 8000, 16384\n\
                      GeForce RTX 4090, 55, 40, 2400, 2000, 24576\n";
        let gpus = parse_nvidia_smi_csv(sample);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "Tesla V100");
        assert_eq!(gpus[1].name, "GeForce RTX 4090");
    }

    #[test]
    fn parse_nvidia_smi_skips_rows_with_too_few_fields() {
        let sample = "RTX 4090, 50, 30, 2400, 1000, 24576\n\
                      not enough fields, oops\n";
        assert_eq!(parse_nvidia_smi_csv(sample).len(), 1);
    }

    #[test]
    fn parse_nvidia_smi_treats_unparseable_values_as_zero() {
        // Older drivers return "[Not Supported]" for fields not exposed by the GPU.
        let sample = "RTX 4090, [Not Supported], 30, 2400, 1000, 24576\n";
        let gpus = parse_nvidia_smi_csv(sample);
        assert_eq!(gpus[0].temperature, 0.0);
        assert_eq!(gpus[0].usage_percent, 30.0);
    }

    #[test]
    fn parse_nvidia_smi_returns_empty_for_empty_input() {
        assert!(parse_nvidia_smi_csv("").is_empty());
        assert!(parse_nvidia_smi_csv("\n\n").is_empty());
    }

    #[test]
    fn to_sysinfo_produces_labels_recognised_by_device_send_sysinfo() {
        let metrics = SystemMetrics {
            cpu: CpuMetrics {
                temperature: 65.0,
                usage_percent: 42.0,
                frequency_mhz: 3800.0,
                core_count: 16,
            },
            gpus: vec![GpuMetrics {
                name: "RX 7900 XTX".into(),
                temperature: 55.0,
                usage_percent: 30.0,
                frequency_mhz: 2300.0,
                voltage_mv: 950.0,
                ..Default::default()
            }],
            ram: RamMetrics {
                total_mb: 32_000,
                used_mb: 12_800,
                available_mb: 19_200,
                usage_percent: 40,
            },
            disk: DiskMetrics::default(),
            net: NetMetrics::default(),
        };

        let sysinfo = metrics.to_sysinfo();

        let labels: Vec<&str> = sysinfo.iter().map(|s| s.label.as_str()).collect();
        // Labels device.rs's apply_sysinfo_metric matches on:
        for required in &[
            "CPU Temperature",
            "CPU Usage",
            "CPU Frequency",
            "GPU Temperature",
            "GPU Usage",
            "GPU Frequency",
            "GPU Voltage",
            "Memory Utilization",
        ] {
            assert!(labels.contains(required), "missing label: {}", required);
        }

        let cpu_temp = sysinfo
            .iter()
            .find(|s| s.label == "CPU Temperature")
            .unwrap();
        assert_eq!(cpu_temp.value, "65");
        assert_eq!(cpu_temp.unit, "°C");
    }

    #[test]
    fn to_sysinfo_emits_zero_gpu_block_when_no_gpus_detected() {
        let metrics = SystemMetrics {
            cpu: CpuMetrics::default(),
            gpus: Vec::new(),
            ram: RamMetrics::default(),
            net: NetMetrics::default(),
            disk: DiskMetrics::default(),
        };
        let sysinfo = metrics.to_sysinfo();
        for label in &[
            "GPU Temperature",
            "GPU Usage",
            "GPU Frequency",
            "GPU Voltage",
        ] {
            let entry = sysinfo
                .iter()
                .find(|s| &s.label == label)
                .unwrap_or_else(|| panic!("missing label: {label}"));
            assert_eq!(entry.value, "0", "{label} should be zero");
        }
    }

    #[test]
    fn to_sysinfo_prefers_discrete_gpu_over_integrated() {
        let metrics = SystemMetrics {
            gpus: vec![
                GpuMetrics {
                    name: "AMD iGPU".into(),
                    kind: GpuKind::Integrated,
                    temperature: 40.0,
                    ..Default::default()
                },
                GpuMetrics {
                    name: "RTX 5090".into(),
                    kind: GpuKind::Discrete,
                    temperature: 70.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let gpu_temp = metrics
            .to_sysinfo()
            .into_iter()
            .find(|s| s.label == "GPU Temperature")
            .unwrap();
        // The discrete card wins even though the iGPU is listed first.
        assert_eq!(gpu_temp.value, "70");
    }
}
