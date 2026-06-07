# Display Capabilities

The Panorama firmware has a fixed catalog of overlay metric labels. The host
selects labels in the screen config `sysinfoDisplay` array, then pushes values
in the `STATE all` sysinfo payload.

Unknown labels are not harmless aliases: hardware probing showed that selecting
an unknown label can result in no metrics rendering at all. Treat this list as
the allowlist for display overlay labels.

For user-facing CLI metric tokens, see `pctl display --metrics` in
[`usage.md`](usage.md). This document describes firmware label behavior and
payload mapping.

## Hardware-Verified Labels

Verified on a TRYX Panorama 360, firmware `V1.0.11`, hardware `V1.1`.

| Display label     | Sysinfo payload field | Probe value shown |
|-------------------|-----------------------|-------------------|
| `CPU Temperature` | `cpu.temperature`     | `22`              |
| `CPU Usage`       | `cpu.load`            | `11`              |
| `CPU Frequency`   | `cpu.speedAverage`    | `3333`            |
| `CPU Voltage`     | `cpu.voltage`         | `55`              |
| `GPU Temperature` | `gpu.temperature`     | `88`              |
| `GPU Usage`       | `gpu.load`            | `77`              |
| `GPU Frequency`   | `gpu.speed`           | `1111`            |
| `GPU Voltage`     | `gpu.voltage`         | `456`             |
| `Date&Time`       | `timestamp`           | current date/time |

## Known Firmware Labels Not Exposed As CLI Tokens

These labels exist in the firmware catalog and are handled by the host payload
mapper, but `pctl display --metrics` does not currently expose tokens for them.
They are listed here as protocol surface, not as supported user-facing metric
tokens.

| Display label             | Sysinfo payload field     |
|---------------------------|---------------------------|
| `Hard Disk Temperature`   | `disk.temperature`        |
| `Motherboard Temperature` | `motherboard.temperature` |
| `Memory Frequency`        | `memory.speed`            |
| `Memory Utilization`      | `memory.load`             |

## Display Filter Effects

The screen config `settings.filter` object controls firmware-rendered overlay
effects independently from metric labels:

```json
"settings": {
  "filter": {
    "value": "Smoke",
    "opacity": 80
  }
}
```

Hardware-verified filter values:

| CLI value | Filter value | Result                    |
|-----------|--------------|---------------------------|
| `smoke`   | `Smoke`      | smoke effect rendered     |
| `rain`    | `Rain`       | rain/drop effect rendered |
| `none`    | empty string | effect disabled           |

Use `pctl display --filter <none|smoke|rain>` to configure these effects.
