# TRYX Panorama — ADB Protocol Documentation

The protocol described here is assumed to be shared across the TRYX Panorama
AIO cooler family (Panorama, Panorama SE 360, Panorama SE 240, Panorama WB). It was
reverse-engineered from live USB captures of the Kanali Windows software
(running in a QEMU/KVM VM) talking to an AIO device over USB passthrough.
The capture workflow was: start the VM from virt-manager, then record USB
traffic while the AIO communicated with that VM.

> **Carrier vs. protocol.** This document describes the *frame and payload
> protocol* — that part is independent of how the bytes reach the device. The
> capture below was taken over the **USB ADB / AOA** carrier the Kanali
> Windows app uses. `panorama-mgr` reaches the *same* framed protocol over the
> cooler's **USB CDC-ACM serial port** (`/dev/ttyACM*`) instead; it uses `adb`
> only as a separate channel for pushing media files. The framing, CRC, and
> text payload are identical on either carrier.

## Overview

In the captured Kanali session, the AIO communicates over **USB ADB** using the
Android Open Accessory (AOA) profile (`18d1:2d03`). Once the ADB connection is
established the host and device exchange framed, text-based messages over
**USB bulk transfers** on endpoint 1 of device 3 (Bus 1 in a typical
single-device setup).

The protocol is a simple request/response cycle:

```
Host → AIO : system metrics as JSON
AIO  → Host: device status as JSON
```

---

## USB Layer

| Property          | Value                        |
|-------------------|------------------------------|
| Vendor ID         | `0x18d1` (Google)            |
| Product ID        | `0x2d03`                     |
| USB class         | Android Open Accessory + ADB |
| Transfer type     | Bulk                         |
| Bus (observed)    | Bus 1, Device 3, Endpoint 1  |
| Capture interface | `usbmon1` on the Linux host  |

To capture raw traffic:

```bash
sudo modprobe usbmon
sudo tcpdump -i usbmon1 -c 500 -w capture.pcap
tshark -r capture.pcap -Y "usb.src == \"1.3.1\" || usb.dst == \"1.3.1\""
```

---

## Frame Format

All messages are wrapped in a binary framing layer before being sent over the
chosen USB carrier.

```
[ 0x5A ] [ length (2 bytes BE) ] [ text payload ] [ CRC ] [ 0x5A ]
          |<——————————— byte-stuffed region ——————————————>|
```

### Framing steps (build_frame)

1. Compose the text payload (request line + headers + body).
2. Compute `wire_length = len(text_payload) + 5`.
3. Pack `[wire_length_high, wire_length_low] + text_payload_bytes`.
4. Append a 1-byte CRC: `sum(all_bytes) & 0xFF`.
5. Byte-stuff the entire inner region (see below).
6. Wrap with `0x5A` start and end markers.

### Sentinel bytes

| Byte   | Meaning                         | Escaped as  |
|--------|---------------------------------|-------------|
| `0x5A` | Frame marker (`FRAME_MARKER`)   | `0x5B 0x01` |
| `0x5B` | Escape marker (`ESCAPE_MARKER`) | `0x5B 0x02` |

---

## Text Payload Format

The payload inside the frame is HTTP-like: a request/status line, headers,
a blank line, then an optional body.

### Host → AIO (outbound request)

```
STATE all 1\r\n
SeqNumber=<n>\r\n
Date=<unix_ms>\r\n
ContentType=json\r\n
ContentLength=<len>\r\n
\r\n
<JSON body>
```

The observed outbound packet size is **547 bytes** of payload (611 bytes on
the wire including the USB/framing overhead).

#### JSON body — system metrics

```json
{
  "network":     { "upload": 0, "download": 0 },
  "memory":      { "total": 4075, "used": 2337, "load": 57, "temperature": 0, "speed": 0 },
  "cpu":         { "load": 19, "temperature": 0, "speedAverage": 0, "power": 0, "voltage": 0, "usage": 28 },
  "gpu":         { "load": 0, "temperature": 0, "fan": 0, "speed": 0, "power": 0, "voltage": 0 },
  "disk":        { "total": 63, "used": 21, "load": 33, "activity": 1, "temperature": 0, "readSpeed": 0, "writeSpeed": 2837 },
  "fans":        [ ... ],
  "motherboard": { "temperature": 0, "pchTemperature": 0 },
  "timestamp":   1778692385838
}
```

Units (inferred):
- `memory.total` / `memory.used` — MB
- `disk.total` / `disk.used` — GB
- `*.load` — percentage (0–100)
- `*.temperature` — degrees Celsius
- `*.speed` / `speedAverage` — MHz
- `disk.readSpeed` / `disk.writeSpeed` — KB/s
- `network.upload` / `network.download` — KB/s
- `timestamp` — Unix epoch milliseconds

### AIO → Host (response)

```
1 200\r\n
AckNumber=<SeqNumber+1>\r\n
ContentLength=<len>\r\n
ContentType=json\r\n
\r\n
<JSON body>
```

The observed response payload size is **209 bytes** (273 bytes on wire).

#### JSON body — device status

```json
{
  "status": {
    "fanLCD":   "480",
    "turboPump": "3000"
  },
  "warning": "[{\"description\":\"No ERROR\",\"type\":\"Fan LCD\"}]",
  "availableStorage": 3241013248
}
```

Fields:
- `fanLCD` — LCD fan speed in RPM
- `turboPump` — liquid cooling pump speed in RPM
- `warning` — JSON-encoded array of fault/status objects; `"No ERROR"` means healthy
- `availableStorage` — free storage on the AIO in bytes

---

## Timing

| Metric             | Observed value                     |
|--------------------|------------------------------------|
| Update interval    | ~1 second                          |
| Host → AIO payload | 547 bytes                          |
| AIO → Host payload | 209 bytes                          |
| Sequence numbers   | Monotonically incrementing integer |
| AckNumber          | Always `SeqNumber + 1`             |

---

## Capture & Verification Commands

```bash
# Confirm AIO is visible on host (not passed to VM)
lsusb | grep 18d1

# Load capture module and record 500 packets
sudo modprobe usbmon
sudo tcpdump -i usbmon1 -c 500 -w capture.pcap

# Inspect bulk transfers
tshark -r capture.pcap -Y "usb.src == \"1.3.1\" || usb.dst == \"1.3.1\""

# Dump full frame hex for a specific packet
tshark -r capture.pcap -Y "frame.number == 25" -x

# Decode hex payload inline
python3 -c "print(bytes.fromhex('...').decode('utf-8', errors='replace'))"
```
