//! Serial transport for the TRYX Panorama protocol.
//!
//! The cooler exposes a USB CDC-ACM interface; on Linux the kernel's `cdc_acm`
//! driver surfaces it as a `/dev/ttyACM*` node. Discovery enumerates serial
//! ports, keeps those whose USB descriptor reports Google's vendor id
//! (`0x18d1`), and opens the match at 115200 8N1. The framing layer
//! (`crate::protocol`) is unchanged — this module only moves raw bytes.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use serialport::{ErrorKind as SerialErrorKind, SerialPort, SerialPortType};

/// Google's USB vendor id — the cooler enumerates under it.
const GOOGLE_VID: u16 = 0x18d1;
/// USB product string TRYX's display board reports (a Rockchip SoC code
/// shared across the Panorama family).
const PANORAMA_PRODUCT: &str = "cm01";

const BAUD_RATE: u32 = 115_200;
/// Frame sentinel — a complete frame starts and ends with this byte.
const FRAME_MARKER: u8 = 0x5A;

/// Upper bound on how long [`SerialTransport::receive`] waits for a frame.
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(2);
/// Per-read blocking granularity while accumulating a response.
const READ_POLL_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("serial I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "serial port '{path}' is busy — another process likely owns the Panorama device \
         (for example `pctl daemon`); stop the daemon or route the command through it"
    )]
    PortBusy {
        path: String,
        #[source]
        source: serialport::Error,
    },

    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error(
        "no Panorama device found — no /dev/ttyACM* port reports USB vendor 0x18d1 \
         (is the cooler connected and is the cdc_acm driver bound?)"
    )]
    NotFound,

    #[error(
        "multiple USB serial ports from vendor 0x18d1 and none identify as TRYX — \
         specify the port explicitly"
    )]
    Ambiguous,
}

/// A serial port that could be the cooler. Returned by
/// [`list_panorama_candidates`] so callers can present a selectable list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCandidate {
    pub port_name: String,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// An open serial connection to the Panorama device.
pub struct SerialTransport {
    port: Box<dyn SerialPort>,
}

impl SerialTransport {
    /// Open the first `/dev/ttyACM*` that looks like a Panorama.
    pub fn open() -> Result<Self, TransportError> {
        let candidates = list_panorama_candidates()?;
        let picked = pick_candidate(&candidates).ok_or(if candidates.is_empty() {
            TransportError::NotFound
        } else {
            TransportError::Ambiguous
        })?;
        Self::open_with_port(&picked.port_name)
    }

    /// Open a specific serial port by path. Escape hatch for unusual setups
    /// (e.g. several USB-serial devices present) — wired to `Config.port`.
    pub fn open_with_port(path: &str) -> Result<Self, TransportError> {
        let port = serialport::new(path, BAUD_RATE)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(READ_POLL_TIMEOUT)
            .open()
            .map_err(|source| classify_open_error(path, source))?;
        // Drop any bytes the device buffered before we connected.
        let _ = port.clear(serialport::ClearBuffer::All);
        Ok(Self { port })
    }

    /// Write a complete frame to the device.
    pub fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.port.write_all(data)?;
        self.port.flush()?;
        Ok(())
    }

    /// Discard any bytes the device has sent that we have not yet read.
    /// Call this before sending a command so a stale or unsolicited frame
    /// from an earlier exchange is not mistaken for this command's response.
    /// Best-effort — a clear failure is not worth aborting the command.
    pub fn clear_input(&mut self) {
        let _ = self.port.clear(serialport::ClearBuffer::Input);
    }

    /// Read a response frame.
    ///
    /// Serial delivers bytes piecemeal, so this accumulates reads until the
    /// buffer is a complete `0x5A … 0x5A` frame or [`RECEIVE_TIMEOUT`]
    /// elapses. It returns whatever arrived by the deadline — the caller
    /// ([`crate::protocol::parse_response`]) validates the frame.
    pub fn receive(&mut self, max_bytes: usize) -> Result<Vec<u8>, TransportError> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 256];
        let deadline = Instant::now() + RECEIVE_TIMEOUT;

        while Instant::now() < deadline && buf.len() < max_bytes {
            match self.port.read(&mut chunk) {
                Ok(0) => {}
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if is_complete_frame(&buf) {
                        break;
                    }
                }
                // A read timeout just means no bytes this slice — keep waiting
                // until the overall deadline.
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(TransportError::Io(e)),
            }
        }

        Ok(buf)
    }
}

/// True once `buf` holds a complete frame: at least the two sentinels, with
/// both ends `0x5A`. Inner `0x5A` bytes are byte-stuffed away by the protocol
/// layer, so the only unescaped markers are the frame boundaries.
fn is_complete_frame(buf: &[u8]) -> bool {
    buf.len() >= 2 && buf[0] == FRAME_MARKER && buf[buf.len() - 1] == FRAME_MARKER
}

fn classify_open_error(path: &str, source: serialport::Error) -> TransportError {
    if is_busy_serial_error(&source) {
        return TransportError::PortBusy {
            path: path.to_string(),
            source,
        };
    }
    TransportError::Serial(source)
}

fn is_busy_serial_error(error: &serialport::Error) -> bool {
    let description = error.to_string().to_ascii_lowercase();
    matches!(
        error.kind(),
        SerialErrorKind::NoDevice
            | SerialErrorKind::Io(std::io::ErrorKind::WouldBlock)
            | SerialErrorKind::Io(std::io::ErrorKind::Other)
    ) && (description.contains("device or resource busy")
        || description.contains("resource busy")
        || description.contains("exclusive lock")
        || description.contains("already in use"))
}

/// Enumerate serial ports and return those whose USB descriptor reports
/// Google's vendor id. String fields are filled best-effort.
pub fn list_panorama_candidates() -> Result<Vec<DeviceCandidate>, TransportError> {
    let mut out = Vec::new();
    for info in serialport::available_ports()? {
        if let SerialPortType::UsbPort(usb) = &info.port_type {
            if usb.vid == GOOGLE_VID {
                out.push(DeviceCandidate {
                    port_name: info.port_name.clone(),
                    vid: usb.vid,
                    pid: usb.pid,
                    serial: usb.serial_number.clone(),
                    manufacturer: usb.manufacturer.clone(),
                    product: usb.product.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Pick the best Panorama candidate.
///
/// 1. Prefer a candidate whose USB product string is `cm01` (TRYX's board).
/// 2. Otherwise, if exactly one candidate is present, use it.
/// 3. Otherwise return `None` — the caller must disambiguate.
fn pick_candidate(candidates: &[DeviceCandidate]) -> Option<&DeviceCandidate> {
    if let Some(c) = candidates
        .iter()
        .find(|c| c.product.as_deref() == Some(PANORAMA_PRODUCT))
    {
        return Some(c);
    }
    if candidates.len() == 1 {
        return Some(&candidates[0]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(port: &str, product: Option<&str>) -> DeviceCandidate {
        DeviceCandidate {
            port_name: port.to_string(),
            vid: GOOGLE_VID,
            pid: 0x2d03,
            serial: None,
            manufacturer: None,
            product: product.map(String::from),
        }
    }

    #[test]
    fn complete_frame_needs_both_sentinels() {
        assert!(is_complete_frame(&[0x5A, 0x5A]));
        assert!(is_complete_frame(&[0x5A, 0x01, 0x02, 0x5A]));
    }

    #[test]
    fn incomplete_frame_is_rejected() {
        assert!(!is_complete_frame(&[]));
        assert!(!is_complete_frame(&[0x5A]));
        assert!(!is_complete_frame(&[0x5A, 0x01, 0x02])); // no trailing marker
        assert!(!is_complete_frame(&[0x01, 0x02, 0x5A])); // no leading marker
    }

    #[test]
    fn pick_returns_none_for_empty_input() {
        assert_eq!(pick_candidate(&[]), None);
    }

    #[test]
    fn pick_returns_the_only_candidate_when_unambiguous() {
        let list = vec![candidate("/dev/ttyACM0", None)];
        assert_eq!(pick_candidate(&list).unwrap().port_name, "/dev/ttyACM0");
    }

    #[test]
    fn pick_prefers_cm01_product_when_multiple_present() {
        let list = vec![
            candidate("/dev/ttyACM0", Some("phone")),
            candidate("/dev/ttyACM1", Some(PANORAMA_PRODUCT)),
        ];
        let picked = pick_candidate(&list).expect("should pick the cm01 port");
        assert_eq!(picked.port_name, "/dev/ttyACM1");
    }

    #[test]
    fn pick_returns_none_when_multiple_and_no_cm01() {
        let list = vec![
            candidate("/dev/ttyACM0", Some("phone")),
            candidate("/dev/ttyACM1", Some("dock")),
        ];
        assert!(pick_candidate(&list).is_none());
    }

    #[test]
    fn pick_prefers_cm01_over_a_lone_other_port() {
        // The cm01 product match wins even when it is not the only candidate.
        let list = vec![
            candidate("/dev/ttyACM0", Some(PANORAMA_PRODUCT)),
            candidate("/dev/ttyACM1", Some("phone")),
        ];
        assert_eq!(pick_candidate(&list).unwrap().port_name, "/dev/ttyACM0");
    }

    #[test]
    fn classifies_busy_no_device_error_as_port_busy() {
        let err = serialport::Error::new(SerialErrorKind::NoDevice, "Device or resource busy");

        match classify_open_error("/dev/ttyACM0", err) {
            TransportError::PortBusy { path, .. } => assert_eq!(path, "/dev/ttyACM0"),
            other => panic!("expected PortBusy, got {other:?}"),
        }
    }

    #[test]
    fn classifies_exclusive_lock_error_as_port_busy() {
        let err = serialport::Error::new(
            SerialErrorKind::NoDevice,
            "Unable to acquire exclusive lock on serial port",
        );

        assert!(matches!(
            classify_open_error("/dev/ttyACM0", err),
            TransportError::PortBusy { .. }
        ));
    }

    #[test]
    fn leaves_non_busy_errors_as_generic_serial_errors() {
        let err = serialport::Error::new(SerialErrorKind::NoDevice, "No such file or directory");

        assert!(matches!(
            classify_open_error("/dev/ttyACM0", err),
            TransportError::Serial(_)
        ));
    }
}
