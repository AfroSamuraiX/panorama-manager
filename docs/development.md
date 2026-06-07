# Developing `panorama-mgr`

This guide covers building, testing, and the structure of the codebase. For
installing and using the tool, see the [README](../README.md) and
[docs/usage.md](usage.md).

## Prerequisites

- A stable Rust toolchain (the workspace targets edition 2021). Install via
  [rustup](https://rustup.rs/).
- System packages for the `serialport` crate's USB enumeration:
  - **Debian/Ubuntu:** `pkg-config`, `libudev-dev`
  - **Arch:** `pkgconf`, `systemd-libs` (ships `libudev`)

  These match what CI installs (`.github/workflows/rust.yml`).
- Optional, only for exercising the tool against hardware: `adb`, `ffmpeg`,
  `glxinfo` from `mesa-utils` for GPU badge-name fallback, and a connected
  Panorama cooler.

- Optional, only for GUI development: Node.js/npm and Tauri v2 Linux WebKitGTK
  prerequisites. On Arch this is typically `webkit2gtk-4.1` plus GTK stack
  packages.

## Workspace layout

```
panorama-mgr/
├── Cargo.toml                workspace root; pins shared dependency versions
│                             under [workspace.dependencies]
├── crates/
│   ├── panorama-core/        library crate — all device/protocol/system logic
│   ├── panorama-ctl/         binary crate — the `pctl` CLI
│   └── panorama-gui/         Tauri/Svelte desktop GUI; backend excluded from root workspace
├── docs/                     this guide, the usage reference, the protocol spec
└── packaging/                udev rule + systemd user unit
```

See [docs/packaging.md](packaging.md) for local release artifact packaging.

## `panorama-core` module map

`panorama-core` holds everything except CLI argument parsing. Each module has a
single responsibility:

| Module      | Responsibility                                                                                                                                                 |
|-------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `transport` | USB CDC-ACM serial I/O. Discovers the cooler's `/dev/ttyACM*` port (USB VID `0x18d1`, PID `0x2d0[0-5]`), 115200 8N1.                                           |
| `protocol`  | Wire format: `0x5A` frame delimiting, byte-stuffing, the 1-byte sum-mod-256 CRC, and the HTTP-like text request/response payload.                              |
| `device`    | High-level command surface — handshake, brightness, fan-LCD, reboot, screen config, sysinfo push. `Device::connect` opens the port and performs the handshake. |
| `adb`       | Wrapper around the system `adb` binary for media push to `/sdcard/pcMedia/`, listing, and device detection.                                                    |
| `media`     | Media-file classification and `ffmpeg`-driven conversion to MP4.                                                                                               |
| `config`    | XDG-compliant persistence of user config and saved display state.                                                                                              |
| `metrics`   | Linux `sysfs`/`procfs` reader for CPU/GPU/RAM/disk/network metrics (AMD via `amdgpu` sysfs, Nvidia via `nvidia-smi`). Feeds `device`'s sysinfo push.           |

Keep wire-format details in `protocol`; keep transport details in `transport`.
`device` composes the two and is what the CLI calls.

## Build and test

```bash
cargo build --workspace
cargo test --workspace
```

The default test run is pure logic — no hardware, no `adb`, no `ffmpeg`
required. CI runs this test path for non-docs changes on pushes and pull
requests to `main`.

### Hardware-gated tests

Tests that need a connected cooler live in
`crates/panorama-core/tests/live_device.rs` and are marked `#[ignore]` so the
default run skips them. With a cooler plugged in:

```bash
cargo test --test live_device -- --ignored --nocapture
```

The suite discovers the serial port, validates the device attributes, and
round-trips a request against the cooler. CI does not run these — if you change
transport, protocol, or device code, run them locally before opening a PR.

## Adding a new CLI command

1. Add a variant to the `Commands` enum in
   `crates/panorama-ctl/src/main.rs`. It uses `clap`'s derive API — the
   doc comment on the variant becomes its `--help` text.
2. Add a match arm for it in `main()`'s dispatch.
3. Write a `cmd_<name>` handler function.
4. If the command touches the device, add or extend a method on
   `panorama_core::device::Device` rather than building frames in the CLI.
   Keep raw wire-format logic in `panorama_core::protocol`.
5. Unit-test the pure logic. Gate anything that needs hardware behind
   `#[ignore]`.
6. Document the command in [docs/usage.md](usage.md) and add it to the command
   table in the [README](../README.md).

## Conventions

- **Errors:** `anyhow` for application-level errors in `panorama-ctl`;
  `thiserror` for typed library errors in `panorama-core`.
- **Logging:** `tracing`. The daemon initializes a subscriber that writes to
  stderr. `RUST_LOG` controls the level for foreground runs; systemd-managed
  runs use the unit environment or a user-service override.
- **Dependencies:** pinned once in the workspace `Cargo.toml` under
  `[workspace.dependencies]` and referenced from member crates — add new shared
  deps there.

## See also

- [docs/usage.md](usage.md) — the user-facing command reference
- [docs/adb-protocol.md](adb-protocol.md) — the reverse-engineered wire protocol
- [docs/daemon-ipc-design.md](daemon-ipc-design.md) — the daemon/socket architecture implemented for multi-command serial access
- `packaging/` — the udev rule and systemd unit referenced by the README
