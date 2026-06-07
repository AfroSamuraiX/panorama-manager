//! Tests against real captured wire frames from a connected device.
//!
//! Fixtures are taken from `panorama-mgr/docs/captures/adb_usb.pcap` — extracted
//! bulk-transfer payloads of host→device requests and device→host responses
//! during normal `STATE all 1` polling. Each frame here is the literal byte
//! sequence the device sent or received over USB.

use panorama_core::protocol::{build_frame, parse_response, FRAME_MARKER};

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

// Host→device system-metrics request, SeqNumber=713, captured 2025-06-09.
// 547 bytes on the wire. Body contains `"fans":[]` which encodes the `0x5B`
// inside as `5b 02` — exercises the unescape path.
const REQ_FRAME_0_HEX: &str = "5a0222535441544520616c6c20310d0a5365714e756d6265723d3731330d0a446174653d313737383731373538363434360d0a436f6e74656e74547970653d6a736f6e0d0a436f6e74656e744c656e6774683d3435340d0a0d0a7b226e6574776f726b223a7b2275706c6f6164223a302c22646f776e6c6f6164223a307d2c226d656d6f7279223a7b22746f74616c223a343037352c2275736564223a323333372c226c6f6164223a35372c2274656d7065726174757265223a302c227370656564223a307d2c22637075223a7b226c6f6164223a31392c2274656d7065726174757265223a302c22737065656441766572616765223a302c22706f776572223a302c22766f6c74616765223a302c227573616765223a32387d2c22677075223a7b226c6f6164223a302c2274656d7065726174757265223a302c2266616e223a302c227370656564223a302c22706f776572223a302c22766f6c74616765223a307d2c226469736b223a7b22746f74616c223a36332c2275736564223a32312c226c6f6164223a33332c226163746976697479223a312c2274656d7065726174757265223a302c22726561645370656564223a302c2277726974655370656564223a323833377d2c2266616e73223a5b025d2c226d6f74686572626f617264223a7b2274656d7065726174757265223a302c2270636854656d7065726174757265223a307d2c2274696d657374616d70223a313737383639323338353833387d5c5a";

// Device→host response to REQ_FRAME_0, AckNumber=714.
// 209 bytes on the wire. Body contains escaped `\"` quotes inside the
// `warning` JSON string.
const RESP_FRAME_0_HEX: &str = "5a00d031203230300d0a41636b4e756d6265723d3731340d0a436f6e74656e744c656e6774683d3134320d0a436f6e74656e74547970653d6a736f6e0d0a0d0a7b22737461747573223a7b2266616e4c4344223a22343830222c22747572626f50756d70223a2233303030227d2c227761726e696e67223a225b027b5c226465736372697074696f6e5c223a5c224e6f204552524f525c222c5c22747970655c223a5c2246616e204c43445c227d5d222c22617661696c61626c6553746f72616765223a333234313031333234387d085a";

// Second request/response pair, one second later. SeqNumber=714.
const REQ_FRAME_1_HEX: &str = "5a0222535441544520616c6c20310d0a5365714e756d6265723d3731340d0a446174653d313737383731373538373435390d0a436f6e74656e74547970653d6a736f6e0d0a436f6e74656e744c656e6774683d3435340d0a0d0a7b226e6574776f726b223a7b2275706c6f6164223a302c22646f776e6c6f6164223a307d2c226d656d6f7279223a7b22746f74616c223a343037352c2275736564223a323333362c226c6f6164223a35372c2274656d7065726174757265223a302c227370656564223a307d2c22637075223a7b226c6f6164223a32332c2274656d7065726174757265223a302c22737065656441766572616765223a302c22706f776572223a302c22766f6c74616765223a302c227573616765223a31397d2c22677075223a7b226c6f6164223a302c2274656d7065726174757265223a302c2266616e223a302c227370656564223a302c22706f776572223a302c22766f6c74616765223a307d2c226469736b223a7b22746f74616c223a36332c2275736564223a32312c226c6f6164223a33332c226163746976697479223a312c2274656d7065726174757265223a302c22726561645370656564223a302c2277726974655370656564223a323833377d2c2266616e73223a5b025d2c226d6f74686572626f617264223a7b2274656d7065726174757265223a302c2270636854656d7065726174757265223a307d2c2274696d657374616d70223a313737383639323338363834307d565a";

const RESP_FRAME_1_HEX: &str = "5a00d031203230300d0a41636b4e756d6265723d3731350d0a436f6e74656e744c656e6774683d3134320d0a436f6e74656e74547970653d6a736f6e0d0a0d0a7b22737461747573223a7b2266616e4c4344223a22343530222c22747572626f50756d70223a2233303030227d2c227761726e696e67223a225b027b5c226465736372697074696f6e5c223a5c224e6f204552524f525c222c5c22747970655c223a5c2246616e204c43445c227d5d222c22617661696c61626c6553746f72616765223a333234313030393135327d055a";

#[test]
fn captured_response_frames_have_well_formed_boundaries() {
    for hex in [RESP_FRAME_0_HEX, RESP_FRAME_1_HEX] {
        let wire = hex_decode(hex);
        assert_eq!(wire.first(), Some(&FRAME_MARKER));
        assert_eq!(wire.last(), Some(&FRAME_MARKER));
    }
}

#[test]
fn captured_request_frames_have_well_formed_boundaries() {
    for hex in [REQ_FRAME_0_HEX, REQ_FRAME_1_HEX] {
        let wire = hex_decode(hex);
        assert_eq!(wire.first(), Some(&FRAME_MARKER));
        assert_eq!(wire.last(), Some(&FRAME_MARKER));
    }
}

#[test]
fn parse_real_device_response() {
    let wire = hex_decode(RESP_FRAME_0_HEX);
    let resp = parse_response(&wire).expect("captured response should parse");

    assert_eq!(resp.version, "1");
    assert_eq!(resp.status, "200");

    let json = resp.json.expect("response body should be JSON");
    assert_eq!(json["status"]["fanLCD"], "480");
    assert_eq!(json["status"]["turboPump"], "3000");
    assert_eq!(json["availableStorage"], 3_241_013_248_i64);

    // Sanity: AckNumber header appears literally in the raw text.
    assert!(resp.raw.contains("AckNumber=714"));
}

#[test]
fn parse_real_device_response_second_sample() {
    let wire = hex_decode(RESP_FRAME_1_HEX);
    let resp = parse_response(&wire).expect("captured response should parse");

    assert_eq!(resp.version, "1");
    assert_eq!(resp.status, "200");
    let json = resp.json.expect("body should parse");
    assert_eq!(json["status"]["fanLCD"], "450");
}

#[test]
fn parse_handles_real_request_frame_with_stuffed_payload() {
    // The request body contains `"fans":[]` — the `[` (0x5B) is byte-stuffed
    // as `5b 02`. Successful parse proves the unescape path handles real
    // captured stuffing correctly.
    let wire = hex_decode(REQ_FRAME_0_HEX);
    let resp = parse_response(&wire).expect("captured request should parse");

    // Request first line is "STATE all 1" — tokenized as version/status by the
    // generic parser. Useful here mainly as a smoke check that the parser
    // didn't bail.
    assert_eq!(resp.version, "STATE");
    assert_eq!(resp.status, "all");

    let json = resp.json.expect("metrics body should parse");
    assert!(json.get("cpu").is_some());
    assert!(json.get("memory").is_some());
    let fans = &json["fans"];
    assert!(
        fans.is_array(),
        "fans should be an (empty) array — proves [ unescape worked"
    );
    assert_eq!(fans.as_array().unwrap().len(), 0);
}

#[test]
fn build_frame_reproduces_a_captured_request_byte_for_byte() {
    // Reconstruct REQ_FRAME_0 from its known inputs. If our framing, length,
    // CRC, and byte-stuffing all match the device's expectations, this
    // produces the exact captured wire bytes.
    let body = r#"{"network":{"upload":0,"download":0},"memory":{"total":4075,"used":2337,"load":57,"temperature":0,"speed":0},"cpu":{"load":19,"temperature":0,"speedAverage":0,"power":0,"voltage":0,"usage":28},"gpu":{"load":0,"temperature":0,"fan":0,"speed":0,"power":0,"voltage":0},"disk":{"total":63,"used":21,"load":33,"activity":1,"temperature":0,"readSpeed":0,"writeSpeed":2837},"fans":[],"motherboard":{"temperature":0,"pchTemperature":0},"timestamp":1778692385838}"#;

    let rebuilt = build_frame("STATE", "all", body, "1", 713, 1_778_717_586_446);

    let expected = hex_decode(REQ_FRAME_0_HEX);
    assert_eq!(
        rebuilt, expected,
        "rebuilt wire frame must match the captured device traffic byte-for-byte"
    );
}
