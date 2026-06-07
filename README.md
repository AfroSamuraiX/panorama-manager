# panorama-manager

`panorama-manager` is an **unofficial** Rust CLI and Linux desktop GUI for controlling TRYX Panorama AIO LCD coolers on Linux.
It lets you upload and display media, configure brightness and fan-LCD speed, and run a background daemon for persistent display and live metrics. The CLI binary is `pctl(panorama-control)`; the GUI binary is `panorama-gui`.

## Status

**Alpha.** This is still considered alpha because hardware validation is currently limited to the standard TRYX Panorama 360 AIO.
While releases are in 0.x, interfaces may still change between releases.

## Supported models

`panorama-manager` targets the TRYX Panorama family. The base Tryx Panorama 360 is hardware-verified.
Other models are expected to share the protocol, but there is still a need for reporting and validation.

| Model                   | Hardware-verified |
|-------------------------|-------------------|
| TRYX Panorama 360 Black | Yes               |
| TRYX Panorama SE 360    | n/a               |
| TRYX Panorama SE 240    | n/a               |

If you test on an unverified model, reports are welcome. Please update the [Tested On](#tested-on) section

## Features

- Installer for CLI and GUI binaries
- Device info and diagnostics (`pctl info`, `pctl doctor`)
- Brightness, fan-LCD speed, sleep/display controls
- Media upload + display control (single/split-screen)
- Tauri desktop GUI for overview and display editing
- Automatic media conversion for non-MP4 formats via `ffmpeg`
- Optional live metrics overlay on the LCD
- Foreground keepalive daemon, with systemd user-service integration
- Persistent user configuration via XDG paths

## Requirements

Verified prereq matrix:

| Distro / family  | Package manager | Required packages                                    | Optional packages | Notes                                     |
|------------------|-----------------|------------------------------------------------------|-------------------|-------------------------------------------|
| Arch Linux       | `pacman`        | `android-tools` `ffmpeg` `webkit2gtk-4.1`            | `mesa-utils`      | `glxinfo` comes from `mesa-utils`         |
| Ubuntu 24.04 LTS | `apt`           | `adb` `ffmpeg` `libwebkit2gtk-4.1-0` `libgtk-3-0t64` | `mesa-utils`      | Exact package names verified for 24.04    |
| Fedora 43/44     | `dnf`           | `android-tools` `ffmpeg-free` `webkit2gtk4.1`        | `mesa-demos`      | `glxinfo` comes from `mesa-demos`         |
| Bazzite          | `rpm-ostree`    | `android-tools` `ffmpeg-free` `webkit2gtk4.1`        | `mesa-demos`      | Layering requires reboot after deployment |

Installer requirements:

- `curl`
- `cut`
- `grep`
- `install`
- `mktemp`
- `sed`
- `sha256sum`
- `sudo`
- `systemctl` with user services enabled
- `tar`

Runtime dependencies:

### CLI  

| Tool         | Needed for                                                    | Required? |
|--------------|---------------------------------------------------------------|-----------|
| `adb`        | Media transfer to the cooler                                  | Yes       |
| `ffmpeg`     | Converting non-MP4 media before upload                        | Yes       |
| `nvidia-smi` | Nvidia GPU metrics (ships with Nvidia driver)                 | Optional  |
| `glxinfo`    | Cleaner GPU badge names when sysfs/Nvidia data is unavailable | Optional  |

### GUI 

GUI runtime dependencies are the `WebKitGTK/GTk` libraries required by Tauri V2. The GUI binary links against
`GTK3`, `GDK Pixbuf`, `Cairo`, `Pango`, `WebKitGTK 4.1`, `JavaScriptCoreGTK 4.1`, and `libsoup 3` through the distro runtime.

## Installation

The project supports two separate install roles:

- **End user flow:** release binary + systemd user service.
- **Developer flow:** build/install from source for local development.

### End user flow (Release binary and Service)

`panorama-manager`  binaries are available on GitHub Releases. Make sure to install the runtime dependencies first.

Arch Linux:

```bash
sudo pacman -S android-tools ffmpeg webkit2gtk-4.1 mesa-utils
# Optional: mesa-utils provides glxinfo
```  

Ubuntu 24.04 LTS:

```bash
sudo apt install adb ffmpeg libwebkit2gtk-4.1-0 libgtk-3-0t64 mesa-utils
# Optional: mesa-utils provides glxinfo
```

Fedora 43/44:

```bash
sudo dnf install android-tools ffmpeg-free webkit2gtk4.1 mesa-demos
# Optional:
```  

Bazzite:

```bash
sudo rpm-ostree install android-tools ffmpeg-free webkit2gtk4.1 mesa-demos
# Optional: mesa-demos provides glxinfo
# Reboot after new deployment is created.
```  

#### Installer  

```bash
curl -fsSL -o /tmp/panorama-mgr-install.sh \
  https://raw.githubusercontent.com/afrosamuraix/panorama-manager/main/scripts/install.sh
bash /tmp/panorama-manager-install.sh
```

The installer writes `pctl` and `panorama-gui` to `~/.local/bin`, installs the
udev rule, writes the systemd user service, installs the GUI icon and desktop
launcher, and starts the daemon.

> If `~/.local/bin` is not already on your PATH`, add it before running `pctl` directly.

The installer is intended to be idempotent: rerunning it replaces the same
managed files in place, refreshes the same desktop entry and user service, and
re-applies the same udev rule without creating duplicates. 

Verify installer completion: 

```bash
~/.local/bin/pctl doctor
systemctl --user status panorama --no-pager
```

This keeps display state and live metrics persistent by making the keepalive
daemon the normal control path.

#### Uninstaller  

To remove the installed binaries, launcher assets, user service, and packaged
udev rule, do the following:

```bash
curl -fsSL -o /tmp/panorama-manager-teardown.sh
  https://raw.githubusercontent.com/bhornsby/panorama-manager/main/scripts/teardown.sh
bash /tmp/panorama-manager-teardown.sh
```

> The teardown script leaves user config and display state in place.

### Developer flow (source install)

For extra developer information see :
*   **[docs/development.md](docs/development.md)**
*   **[docs/adb-protocol.md](docs/adb-protocol.md)**
*   **[docs/display-capabilities.md](docs/display-capabilities.md)**
*   **[docs/packaging.md](docs/packaging.md)**

For contributors or local development from source:

```bash
cargo install --path crates/panorama-ctl
```

This installs `pctl` to `~/.cargo/bin`.

Developer flow does not require a managed user service. For ad-hoc development,
run the daemon directly in a terminal when needed:

```bash
pctl daemon
```

If you are also using the managed user service, restart it after reinstalling
`pctl` so the daemon picks up the new binary:

```bash
systemctl --user restart panorama
```

On daemon startup, `pctl` reapplies `fan-lcd-percent` from your saved config,
so configured fan-LCD speed survives daemon restarts.

If more than one `pctl` is installed on your system, verify your shell is
resolving the intended binary with:

```bash
command -v pctl
```


## Quick start

```bash
pctl info
pctl doctor
pctl brightness 75
pctl upload clip.png
pctl display clip.png
pctl display --metrics cpu-temp,gpu-temp,cpu-usage
```


See **[docs/usage.md](docs/usage.md)** for full command references and flags.

## Media

`panorama-mgr` ships code only — **no bundled video library**. You provide the media content:

- **MP4** sources are pushed directly.
- **WebM, MKV, AVI, MOV, GIF** are auto-converted to MP4 via `ffmpeg` before upload.
- **Still images** (PNG, JPG, BMP, WebP) are pushed as-is.

Will not provide the preset media library just cause I don't want to be accused of distributing proprietary data.

## Project layout

```text
panorama-mgr/
├── Cargo.toml                workspace root + pinned dependency versions
├── crates/
│   ├── panorama-core/        library: protocol, transport, device, adb, media, config, metrics
│   ├── panorama-ctl/         binary: the `pctl` CLI
│   └── panorama-gui/         Tauri/Svelte desktop GUI
├── docs/                     usage reference, protocol spec, developer guide
└── packaging/                udev rule + systemd user unit
```

## Tested on

| Distro     | Kernel | CPU                 | GPU             | Contributor                              |
|------------|--------|---------------------|-----------------|------------------------------------------|
| Arch Linux | 7.0.9  | AMD Ryzen 9 9950X3D | NVIDIA RTX 5090 | [@afro](https://github.com/AfroSamuraiX) |

If you've tested on a different system, feel free to add yours via PR.  

## Troubleshooting

#### `adb` not found

Install the package for your distro and re-run `pctl doctor`.

> ⚠️ Confirmed against Arch, Ubuntu 24.04, Fedora, and Bazzite docs/package indexes.
> `adb` is `android-tools` on Arch, Fedora, and Bazzite, and `adb` on Ubuntu 24.04.

#### Device is not detected

1. Run `pctl doctor`
2. Verify USB visibility with `lsusb`
3. If you used the release installer, rerun the installer as your normal user so it can refresh the managed udev rule and user service
4. If you installed manually from source or a raw binary, run `sudo pctl setup`
5. Reboot the system and run `pctl doctor` again


## Contributing

Contributions are welcome. See docs for architecture details.

## Disclaimer

This is an unofficial, community-maintained CLI and is not affiliated with or endorsed by TRYX.

While every effort has been made to ensure correctness, this software interacts directly with hardware and is provided as-is. The author is not responsible for any damage to devices, data, or systems resulting from its use.

## License

[MIT](LICENSE)
