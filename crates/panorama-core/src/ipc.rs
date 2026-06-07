//! Daemon IPC wire types and framing helpers for the current local socket-based
//! command-routing path.
//!
//! This module intentionally stays at the shared foundation layer:
//! - socket path discovery under `$XDG_RUNTIME_DIR`
//! - command/status enums
//! - request/response framing helpers over a Unix stream socket
//!
//! The daemon listener, request handling, and CLI routing live in
//! `panorama-ctl`; this module provides the shared protocol pieces used by both
//! sides.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

pub const IPC_PROTOCOL_VERSION: u8 = 1;
const SOCKET_DIR_NAME: &str = "panorama";
const SOCKET_FILENAME: &str = "pctl.sock";
const FRAME_HEADER_BYTES: usize = 6;
const IPC_CONNECT_RETRIES: usize = 5;
const IPC_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(150);
const IPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("XDG_RUNTIME_DIR is not set; cannot determine the user runtime socket path")]
    MissingRuntimeDir,

    #[error("IPC payload too large: {0} bytes")]
    PayloadTooLarge(usize),

    #[error("IPC frame is truncated: expected {expected} bytes, received {actual}")]
    TruncatedFrame { expected: usize, actual: usize },

    #[error("unsupported IPC protocol version: {0}")]
    UnsupportedVersion(u8),

    #[error("unknown IPC command byte: 0x{0:02X}")]
    UnknownCommand(u8),

    #[error("unknown IPC status byte: 0x{0:02X}")]
    UnknownStatus(u8),

    #[error("invalid IPC JSON payload: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IPC I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpcCommand {
    DeviceInfo = 0x01,
    StateAll = 0x02,
    SetBrightness = 0x03,
    SetFanLcd = 0x04,
    SetSleep = 0x05,
    SetScreenConfig = 0x06,
    SetSysinfoDisplay = 0x07,
    SendSysinfo = 0x08,
    Reboot = 0x09,
}

impl IpcCommand {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for IpcCommand {
    type Error = IpcError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::DeviceInfo),
            0x02 => Ok(Self::StateAll),
            0x03 => Ok(Self::SetBrightness),
            0x04 => Ok(Self::SetFanLcd),
            0x05 => Ok(Self::SetSleep),
            0x06 => Ok(Self::SetScreenConfig),
            0x07 => Ok(Self::SetSysinfoDisplay),
            0x08 => Ok(Self::SendSysinfo),
            0x09 => Ok(Self::Reboot),
            _ => Err(IpcError::UnknownCommand(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpcStatus {
    Ok = 0x00,
    BadRequest = 0x01,
    Unsupported = 0x02,
    DeviceNotConnected = 0x03,
    DeviceError = 0x04,
    InternalError = 0x05,
}

impl IpcStatus {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for IpcStatus {
    type Error = IpcError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Ok),
            0x01 => Ok(Self::BadRequest),
            0x02 => Ok(Self::Unsupported),
            0x03 => Ok(Self::DeviceNotConnected),
            0x04 => Ok(Self::DeviceError),
            0x05 => Ok(Self::InternalError),
            _ => Err(IpcError::UnknownStatus(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IpcRequest {
    pub command: IpcCommand,
    pub payload: Option<Value>,
}

impl IpcRequest {
    pub fn new(command: IpcCommand) -> Self {
        Self {
            command,
            payload: None,
        }
    }

    pub fn with_payload<T: Serialize>(command: IpcCommand, payload: &T) -> Result<Self, IpcError> {
        Ok(Self {
            command,
            payload: Some(serde_json::to_value(payload)?),
        })
    }

    pub fn payload_as<T: DeserializeOwned>(&self) -> Result<Option<T>, IpcError> {
        self.payload
            .as_ref()
            .map(|payload| serde_json::from_value(payload.clone()).map_err(IpcError::from))
            .transpose()
    }

    pub fn encode(&self) -> Result<Vec<u8>, IpcError> {
        encode_frame(self.command.as_u8(), self.payload.as_ref())
    }

    pub fn decode(frame: &[u8]) -> Result<Self, IpcError> {
        let (command, payload) = decode_frame(frame, IpcCommand::try_from)?;
        Ok(Self { command, payload })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IpcResponse {
    pub status: IpcStatus,
    pub payload: Option<Value>,
}

impl IpcResponse {
    pub fn new(status: IpcStatus) -> Self {
        Self {
            status,
            payload: None,
        }
    }

    pub fn with_payload<T: Serialize>(status: IpcStatus, payload: &T) -> Result<Self, IpcError> {
        Ok(Self {
            status,
            payload: Some(serde_json::to_value(payload)?),
        })
    }

    pub fn payload_as<T: DeserializeOwned>(&self) -> Result<Option<T>, IpcError> {
        self.payload
            .as_ref()
            .map(|payload| serde_json::from_value(payload.clone()).map_err(IpcError::from))
            .transpose()
    }

    pub fn encode(&self) -> Result<Vec<u8>, IpcError> {
        encode_frame(self.status.as_u8(), self.payload.as_ref())
    }

    pub fn decode(frame: &[u8]) -> Result<Self, IpcError> {
        let (status, payload) = decode_frame(frame, IpcStatus::try_from)?;
        Ok(Self { status, payload })
    }
}

pub fn socket_dir() -> Result<PathBuf, IpcError> {
    socket_dir_from(std::env::var("XDG_RUNTIME_DIR").ok().as_deref())
}

pub fn socket_path() -> Result<PathBuf, IpcError> {
    Ok(socket_dir()?.join(SOCKET_FILENAME))
}

/// Connect to the daemon IPC socket when it is available.
///
/// Returns `Ok(None)` for normal fallback cases, such as the daemon not running
/// or `$XDG_RUNTIME_DIR` being unavailable. Other connection errors are returned
/// because they likely indicate a broken or inaccessible socket path.
pub fn connect_client() -> Result<Option<UnixStream>, IpcError> {
    let socket_path = match socket_path() {
        Ok(path) => path,
        Err(IpcError::MissingRuntimeDir) => return Ok(None),
        Err(e) => return Err(e),
    };

    for attempt in 0..IPC_CONNECT_RETRIES {
        match UnixStream::connect(&socket_path) {
            Ok(stream) => return Ok(Some(stream)),
            Err(e) if ipc_connect_error_allows_fallback(&e) => {
                if attempt + 1 == IPC_CONNECT_RETRIES {
                    return Ok(None);
                }
                std::thread::sleep(IPC_CONNECT_RETRY_DELAY);
            }
            Err(e) => return Err(IpcError::Io(e)),
        }
    }

    Ok(None)
}

/// Send one IPC request to the daemon and decode its response.
///
/// Returns `Ok(None)` when the daemon is not available and the caller should
/// fall back to direct device access.
pub fn send_request(request: IpcRequest) -> Result<Option<IpcResponse>, IpcError> {
    let Some(mut stream) = connect_client()? else {
        return Ok(None);
    };

    stream.set_read_timeout(Some(IPC_REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(IPC_REQUEST_TIMEOUT))?;

    let encoded = request.encode()?;
    stream.write_all(&encoded)?;
    stream.flush()?;

    let frame = read_frame(&mut stream)?;
    Ok(Some(IpcResponse::decode(&frame)?))
}

/// Read one length-prefixed IPC frame from a stream.
pub fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, IpcError> {
    let mut header = [0u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut header)?;

    let payload_len = u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize;
    let mut frame = header.to_vec();
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len];
        reader.read_exact(&mut payload)?;
        frame.extend_from_slice(&payload);
    }
    Ok(frame)
}

fn ipc_connect_error_allows_fallback(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

fn socket_dir_from(runtime_dir: Option<&str>) -> Result<PathBuf, IpcError> {
    let runtime_dir = runtime_dir
        .filter(|v| !v.is_empty())
        .ok_or(IpcError::MissingRuntimeDir)?;
    Ok(PathBuf::from(runtime_dir).join(SOCKET_DIR_NAME))
}

fn encode_frame(tag: u8, payload: Option<&Value>) -> Result<Vec<u8>, IpcError> {
    let payload_bytes = match payload {
        Some(value) => serde_json::to_vec(value)?,
        None => Vec::new(),
    };

    let payload_len = u32::try_from(payload_bytes.len())
        .map_err(|_| IpcError::PayloadTooLarge(payload_bytes.len()))?;

    let mut frame = Vec::with_capacity(FRAME_HEADER_BYTES + payload_bytes.len());
    frame.push(IPC_PROTOCOL_VERSION);
    frame.push(tag);
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(&payload_bytes);
    Ok(frame)
}

fn decode_frame<T, F>(frame: &[u8], decode_tag: F) -> Result<(T, Option<Value>), IpcError>
where
    F: FnOnce(u8) -> Result<T, IpcError>,
{
    if frame.len() < FRAME_HEADER_BYTES {
        return Err(IpcError::TruncatedFrame {
            expected: FRAME_HEADER_BYTES,
            actual: frame.len(),
        });
    }

    let version = frame[0];
    if version != IPC_PROTOCOL_VERSION {
        return Err(IpcError::UnsupportedVersion(version));
    }

    let tag = decode_tag(frame[1])?;
    let payload_len = u32::from_le_bytes([frame[2], frame[3], frame[4], frame[5]]) as usize;
    let expected = FRAME_HEADER_BYTES + payload_len;
    if frame.len() != expected {
        return Err(IpcError::TruncatedFrame {
            expected,
            actual: frame.len(),
        });
    }

    let payload = if payload_len == 0 {
        None
    } else {
        Some(serde_json::from_slice(&frame[FRAME_HEADER_BYTES..])?)
    };

    Ok((tag, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct BrightnessPayload {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct MessagePayload {
        message: String,
    }

    #[test]
    fn socket_dir_uses_xdg_runtime_dir() {
        let path = socket_dir_from(Some("/run/user/1000")).unwrap();
        assert_eq!(path, PathBuf::from("/run/user/1000").join("panorama"));
    }

    #[test]
    fn socket_dir_requires_runtime_dir() {
        assert!(matches!(
            socket_dir_from(None),
            Err(IpcError::MissingRuntimeDir)
        ));
        assert!(matches!(
            socket_dir_from(Some("")),
            Err(IpcError::MissingRuntimeDir)
        ));
    }

    #[test]
    fn request_round_trips_with_typed_payload() {
        let req =
            IpcRequest::with_payload(IpcCommand::SetBrightness, &BrightnessPayload { value: 65 })
                .unwrap();

        let decoded = IpcRequest::decode(&req.encode().unwrap()).unwrap();
        assert_eq!(decoded.command, IpcCommand::SetBrightness);
        assert_eq!(
            decoded.payload_as::<BrightnessPayload>().unwrap(),
            Some(BrightnessPayload { value: 65 })
        );
    }

    #[test]
    fn response_round_trips_without_payload() {
        let resp = IpcResponse::new(IpcStatus::Ok);
        let decoded = IpcResponse::decode(&resp.encode().unwrap()).unwrap();

        assert_eq!(decoded.status, IpcStatus::Ok);
        assert!(decoded.payload.is_none());
    }

    #[test]
    fn response_round_trips_with_structured_payload() {
        let resp = IpcResponse::with_payload(
            IpcStatus::DeviceError,
            &MessagePayload {
                message: "device rejected request".to_string(),
            },
        )
        .unwrap();

        let decoded = IpcResponse::decode(&resp.encode().unwrap()).unwrap();
        assert_eq!(decoded.status, IpcStatus::DeviceError);
        assert_eq!(
            decoded.payload_as::<MessagePayload>().unwrap(),
            Some(MessagePayload {
                message: "device rejected request".to_string(),
            })
        );
    }

    #[test]
    fn read_frame_reads_header_and_payload() {
        let req =
            IpcRequest::with_payload(IpcCommand::SetBrightness, &BrightnessPayload { value: 65 })
                .unwrap();
        let encoded = req.encode().unwrap();
        let mut cursor = std::io::Cursor::new(encoded.clone());

        assert_eq!(read_frame(&mut cursor).unwrap(), encoded);
    }

    #[test]
    fn ipc_connect_fallback_recognizes_expected_io_kinds() {
        assert!(ipc_connect_error_allows_fallback(&std::io::Error::from(
            std::io::ErrorKind::NotFound,
        )));
        assert!(ipc_connect_error_allows_fallback(&std::io::Error::from(
            std::io::ErrorKind::ConnectionRefused,
        )));
        assert!(!ipc_connect_error_allows_fallback(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )));
    }

    #[test]
    fn decode_rejects_unknown_request_command() {
        let frame = [IPC_PROTOCOL_VERSION, 0xFF, 0, 0, 0, 0];
        assert!(matches!(
            IpcRequest::decode(&frame),
            Err(IpcError::UnknownCommand(0xFF))
        ));
    }

    #[test]
    fn decode_rejects_unknown_response_status() {
        let frame = [IPC_PROTOCOL_VERSION, 0xFE, 0, 0, 0, 0];
        assert!(matches!(
            IpcResponse::decode(&frame),
            Err(IpcError::UnknownStatus(0xFE))
        ));
    }

    #[test]
    fn decode_rejects_unsupported_version() {
        let frame = [
            IPC_PROTOCOL_VERSION + 1,
            IpcCommand::DeviceInfo.as_u8(),
            0,
            0,
            0,
            0,
        ];
        assert!(matches!(
            IpcRequest::decode(&frame),
            Err(IpcError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn decode_rejects_truncated_frame() {
        let frame = [
            IPC_PROTOCOL_VERSION,
            IpcCommand::DeviceInfo.as_u8(),
            4,
            0,
            0,
            0,
            b'{',
        ];
        assert!(matches!(
            IpcRequest::decode(&frame),
            Err(IpcError::TruncatedFrame {
                expected: 10,
                actual: 7,
            })
        ));
    }
}
