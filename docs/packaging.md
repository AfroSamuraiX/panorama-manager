# Packaging

`panorama-mgr` ships two user-facing binaries:

- `pctl` — CLI and daemon entrypoint.
- `panorama-gui` — Tauri desktop GUI.

The root Cargo workspace intentionally keeps the Tauri backend excluded, so CLI
builds and tests stay healthy without GUI system dependencies.

## Local Release Artifacts

Build the CLI artifact only:

```bash
PANORAMA_VERSION=v0.0.0-dev scripts/package-release.sh
```

Build CLI and GUI artifacts:

```bash
PANORAMA_VERSION=v0.0.0-dev scripts/package-release.sh --gui
```

Outputs are written to `dist/`:

- `pctl-<version>-linux-x86_64.tar.gz`
- `pctl-<version>-linux-x86_64.sha256`
- `panorama-gui-<version>-linux-x86_64.tar.gz` when `--gui` is used
- `panorama-gui-<version>-linux-x86_64.sha256` when `--gui` is used

The GUI archive contains:

- `panorama-gui`
- `panorama-mgr.png`
- `panorama-mgr.desktop`

## Installer Behavior

`scripts/install.sh` always installs both release binaries plus the daemon
service. The GUI binary goes to `~/.local/bin/panorama-gui`; the icon and
desktop launcher go under XDG user data paths. The installer copies the
packaged `panorama-mgr.desktop` file from the GUI archive and rewrites `Exec=`
to the concrete installation path.

Installer prerequisites:

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

Installer runtime checks:

- Detects `pacman`, `apt`, `dnf`, and `rpm-ostree`.
- Verifies `adb` and `ffmpeg` are installed.
- Verifies the GUI binary resolves its dynamic GTK/WebKitGTK libraries via `ldd`.
- Stops with a package-manager-specific install hint when required runtime dependencies are missing.

## Runtime Dependencies

CLI/runtime dependencies:

- `adb` for device media storage access.
- `ffmpeg` for media conversion.
- `nvidia-smi` when using Nvidia GPU metrics/badges.
- `glxinfo` from `mesa-utils` as an optional GPU badge-name fallback.

GUI dependencies:

- Tauri v2 Linux WebKitGTK/GTK runtime libraries.
- The release GUI binary links against GTK 3, GDK Pixbuf, Cairo, Pango,
  WebKitGTK 4.1, JavaScriptCoreGTK 4.1, and libsoup 3 through the distro runtime.
- Confirmed package names:
  `webkit2gtk-4.1` on Arch Linux, `libwebkit2gtk-4.1-0` and `libgtk-3-0t64` on Ubuntu 24.04 LTS,
  and `webkit2gtk4.1` on Fedora 43/44 and Bazzite.

## Release CI

`.github/workflows/release.yml` builds both CLI and GUI release artifacts when a
new semantic-release version is published. It installs Rust, Node.js, Tauri's
Linux WebKitGTK/GTK dependencies, runs `scripts/package-release.sh --gui`, and
uploads both archive/checksum pairs.

Release verification downloads both artifacts, validates checksums, verifies
`pctl --help`, and checks that the GUI archive contains an executable
`panorama-gui` plus the desktop icon payload.
