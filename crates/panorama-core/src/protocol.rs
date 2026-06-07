//! Wire-format framing and parsing for the TRYX Panorama protocol.
//!
//! Frame layout: `0x5A [len BE u16] [text payload] [crc] 0x5A`, with `0x5A`
//! and `0x5B` byte-stuffed throughout the inner region. The text payload is
//! HTTP-like — a request/status line, CRLF-delimited headers, a blank line,
//! and an optional JSON body.
//!
//! Full wire-format spec: `docs/adb-protocol.md` at the workspace root.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FRAME_MARKER: u8 = 0x5A;
pub const ESCAPE_MARKER: u8 = 0x5B;

/// Single-byte sum-mod-256 checksum used as the frame's trailing CRC.
pub fn calculate_crc(data: &[u8]) -> u8 {
    data.iter()
        .copied()
        .fold(0u32, |acc, b| acc.wrapping_add(b as u32)) as u8
}

/// Byte-stuff sentinels so they cannot appear inside a frame's inner region.
///
/// `0x5A → 0x5B 0x01`, `0x5B → 0x5B 0x02`.
pub fn escape_data(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        match b {
            FRAME_MARKER => {
                out.push(ESCAPE_MARKER);
                out.push(0x01);
            }
            ESCAPE_MARKER => {
                out.push(ESCAPE_MARKER);
                out.push(0x02);
            }
            _ => out.push(b),
        }
    }
    out
}

/// Reverse byte-stuffing applied by [`escape_data`]. A trailing lone
/// `ESCAPE_MARKER` with no follow-up byte is left untouched.
pub fn unescape_data(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == ESCAPE_MARKER && i + 1 < data.len() {
            match data[i + 1] {
                0x01 => {
                    out.push(FRAME_MARKER);
                    i += 2;
                    continue;
                }
                0x02 => {
                    out.push(ESCAPE_MARKER);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

/// Decoded contents of a parsed wire frame.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Response {
    pub raw: String,
    pub body: String,
    pub version: String,
    pub status: String,
    pub json: Option<Value>,
}

/// Build a complete wire frame ready for transmission.
///
/// `timestamp_ms` is the value placed in the `Date=` header. The device
/// interprets this as wall-clock time without applying any timezone, so the
/// caller is expected to pre-apply the host's UTC offset.
pub fn build_frame(
    request_state: &str,
    cmd_type: &str,
    content: &str,
    version: &str,
    seq_number: u64,
    timestamp_ms: i64,
) -> Vec<u8> {
    let request_line = format!("{request_state} {cmd_type} {version}\r\n");
    let headers = format!(
        "SeqNumber={seq_number}\r\n\
         Date={timestamp_ms}\r\n\
         ContentType=json\r\n\
         ContentLength={}\r\n",
        content.len()
    );
    let text_payload = format!("{request_line}{headers}\r\n{content}");

    let wire_length = (text_payload.len() + 5) as u16;

    let mut raw = Vec::with_capacity(2 + text_payload.len() + 1);
    raw.push((wire_length >> 8) as u8);
    raw.push((wire_length & 0xFF) as u8);
    raw.extend_from_slice(text_payload.as_bytes());
    raw.push(calculate_crc(&raw));

    let stuffed = escape_data(&raw);

    let mut wire = Vec::with_capacity(stuffed.len() + 2);
    wire.push(FRAME_MARKER);
    wire.extend_from_slice(&stuffed);
    wire.push(FRAME_MARKER);
    wire
}

/// Parse a complete wire frame into a [`Response`].
///
/// Returns `None` if the frame is too short or missing its boundary markers.
/// CRC bytes are present in the frame but not validated here.
pub fn parse_response(data: &[u8]) -> Option<Response> {
    if data.len() < 4 {
        return None;
    }
    if data[0] != FRAME_MARKER || data[data.len() - 1] != FRAME_MARKER {
        return None;
    }

    let inner = &data[1..data.len() - 1];
    let decoded = unescape_data(inner);

    if decoded.len() < 3 {
        return None;
    }

    // Skip the 2-byte length prefix and the trailing 1-byte CRC.
    let msg_bytes = &decoded[2..decoded.len() - 1];
    let msg_text = String::from_utf8_lossy(msg_bytes).into_owned();

    let mut resp = Response {
        raw: msg_text.clone(),
        ..Default::default()
    };

    if let Some(boundary) = msg_text.find("\r\n\r\n") {
        let header_block = &msg_text[..boundary];
        resp.body = msg_text[boundary + 4..].to_string();

        if !resp.body.is_empty() {
            if let Ok(value) = serde_json::from_str::<Value>(&resp.body) {
                resp.json = Some(value);
            }
        }

        let status_line = match header_block.find("\r\n") {
            Some(nl) => &header_block[..nl],
            None => header_block,
        };

        let mut parts = status_line.split_whitespace();
        if let Some(v) = parts.next() {
            resp.version = v.to_string();
        }
        if let Some(s) = parts.next() {
            resp.status = s.to_string();
        }
    }

    Some(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_sums_modulo_256() {
        assert_eq!(calculate_crc(&[]), 0);
        assert_eq!(calculate_crc(&[1, 2, 3]), 6);
        assert_eq!(calculate_crc(&[0xFF, 0xFF]), 0xFE);
        // 256 copies of 0xFF sum to 0xFF00 — low byte is 0.
        assert_eq!(calculate_crc(&vec![0xFF; 256]), 0);
    }

    #[test]
    fn escape_passes_through_plain_bytes() {
        let input: &[u8] = b"hello world";
        assert_eq!(escape_data(input), input);
    }

    #[test]
    fn escape_replaces_sentinels() {
        let input = [0x5A, 0x5B, 0x42];
        assert_eq!(escape_data(&input), vec![0x5B, 0x01, 0x5B, 0x02, 0x42]);
    }

    #[test]
    fn escape_unescape_round_trip() {
        let sample: &[u8] = &[0x00, 0x5A, 0x5B, 0xFF, 0x5A, 0x5B, 0x42];
        assert_eq!(unescape_data(&escape_data(sample)), sample);
    }

    #[test]
    fn unescape_leaves_trailing_lone_escape_alone() {
        // Defensive: a stray 0x5B at the very end (no follow-up) passes through.
        assert_eq!(unescape_data(&[0x42, 0x5B]), vec![0x42, 0x5B]);
    }

    #[test]
    fn build_frame_is_wrapped_in_markers() {
        let wire = build_frame("STATE", "all", "{}", "1", 0, 0);
        assert_eq!(wire.first(), Some(&FRAME_MARKER));
        assert_eq!(wire.last(), Some(&FRAME_MARKER));
        assert!(wire.len() > 2);
    }

    #[test]
    fn build_frame_records_content_length_header() {
        let body = r#"{"x":1}"#;
        let wire = build_frame("STATE", "all", body, "1", 0, 0);
        let parsed = parse_response(&wire).expect("parse should succeed");
        assert!(parsed
            .raw
            .contains(&format!("ContentLength={}", body.len())));
    }

    #[test]
    fn build_then_parse_recovers_response_shape() {
        // Use response-shaped tokens ("1 200 ") so parse_response's
        // version/status extraction yields meaningful values.
        let body = r#"{"hello":"world","n":42}"#;
        let wire = build_frame("1", "200", body, "", 42, 1_700_000_000_000);

        let parsed = parse_response(&wire).expect("parse should succeed");
        assert_eq!(parsed.version, "1");
        assert_eq!(parsed.status, "200");
        assert_eq!(parsed.body, body);

        let json = parsed.json.expect("body should parse as JSON");
        assert_eq!(json["hello"], "world");
        assert_eq!(json["n"], 42);
    }

    #[test]
    fn parse_handles_frames_with_stuffed_payload_bytes() {
        // Force the payload to contain bytes that need stuffing — verifies
        // round-trip survives escape/unescape on non-trivial content.
        let body = r#"{"sentinel":"Z["}"#;
        let wire = build_frame("1", "200", body, "", 1, 0);

        // Sanity: the wire must be longer than naive length because of stuffing.
        assert!(wire.len() > 4);

        let parsed = parse_response(&wire).expect("parse should succeed");
        assert_eq!(parsed.body, body);
    }

    #[test]
    fn parse_rejects_undersized_frames() {
        assert!(parse_response(&[]).is_none());
        assert!(parse_response(&[FRAME_MARKER]).is_none());
        assert!(parse_response(&[FRAME_MARKER, FRAME_MARKER]).is_none());
        assert!(parse_response(&[FRAME_MARKER, 0x00, FRAME_MARKER]).is_none());
    }

    #[test]
    fn parse_rejects_missing_markers() {
        assert!(parse_response(&[0x5A, 0x00, 0x00, 0x00]).is_none());
        assert!(parse_response(&[0x00, 0x00, 0x00, 0x5A]).is_none());
    }
}
