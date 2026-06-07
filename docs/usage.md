# `pctl` command reference

`pctl` is the command-line tool for TRYX Panorama AIO coolers. This page
documents every command and flag. For installation and first-run setup, see
the [README](../README.md).

## Conventions

- Examples assume `pctl` is on your `PATH`.
- The cooler must be connected over USB. If a command reports a connection or
  permission error, run `pctl doctor` first — it diagnoses both USB access
  paths and attempts to suggest a fix.
- Positional media/file arguments are passed as separate shell arguments.
  Comma-separated lists are reserved for flag values such as `--metrics`.
- Device-protocol commands (`info`, `brightness`, `fan-lcd`, `fan-rpm`,
  `reboot`, `display`, `sleep`, `doctor`, `daemon`) use the cooler's CDC-ACM
serial protocol. When the daemon is active, those foreground commands prefer
the daemon's local Unix socket at `$XDG_RUNTIME_DIR/panorama/pctl.sock` and
fall back to direct serial access when the daemon is absent. Media commands
(`upload`, `library-import`, `list`, and `delete`) shell out to `adb`.

> Commands are hardware-verified on the standard TRYX Panorama 360. The other
> family models (SE 360, SE 240, WB) are untested — if
> a command misbehaves on one of those, please report it.

## Device inspection

### `pctl info`

Connect, perform the handshake, and print device information.

```
pctl info
```

Prints the product ID, OS, serial, app version, firmware, hardware string, and
any device attributes.

### `pctl doctor`

Run connectivity diagnostics.

```
pctl doctor
```

Checks, in order: the serial connection and handshake, whether `adb` can see
the device, and whether the device identifies as a Panorama. When the daemon is
already running, `doctor` reports `Connected via daemon IPC` and fetches live
status through the daemon instead of competing for the serial port directly. On
failure, it prints a targeted suggestion based on what it found:

- **Cooler visible to adb in `offline`, `unauthorized`, or `no permissions`
  state, OR adb sees an unrelated device while the cooler answers over
  serial** — almost always a stale adb server. Suggested fix:
  `adb kill-server`, then re-run `pctl doctor`.
- **adb sees no devices but serial works** — either a stale server or a
  missing udev rule. Try `adb kill-server` first; if adb still sees nothing,
  install the udev rule (a single copy-pasteable command is printed).
- **`adb` not installed / `adb devices` errors** — install or repair adb.
- **No serial connection either** — check the USB cable and power.

`doctor` never executes anything itself — every fix is printed as a command
you can review and run.

### `pctl setup`

Install the bundled udev rule so the cooler is reachable without `root`, then
re-apply the rule live so it takes effect on the currently connected device.

```
sudo pctl setup
```

This command is for manual/source installations and for repairing a missing or stale
udev rule. The release installer already runs it for you.

Writes `packaging/70-tryx-panorama.rules` (built into the binary, no runtime
file lookup) to `/etc/udev/rules.d/70-tryx-panorama.rules`, then runs
`udevadm control --reload-rules` and `udevadm trigger --action=add --subsystem-match=usb --subsystem-match=tty`.

- Must run as root. Without `sudo` you get a clear error and nothing is
  written.
- Safe to re-run — the rule content is fixed, so overwriting an existing
  installation is a no-op.
- If package-managed setup assets are already present (for example, a packaged
  udev rule under `/usr/lib/udev/rules.d/` and a packaged user unit under
  `/usr/lib/systemd/user/`), `pctl setup` refuses and tells you to use the
  package/systemd flow instead.
- `pctl doctor` points at this command when it diagnoses a missing udev
  rule on manual installations or a repairable permissions issue.

## Display control

### `pctl display`

Show already-uploaded media on the LCD and/or configure the on-screen metrics
overlay.

```
pctl display [<filename>] [--metrics <m1,m2,m3>] [--metrics-color <color>]
              [--metrics-align <align>] [--metrics-position <position>]
              [--filter <none|smoke|rain>] [--ratio <1:1|2:1>]
              [--split [--media2 [<filename>]] [--metrics2 <m1,m2,m3>]]
```

At least one of `<filename>`, `--metrics`, `--ratio`, `--split`, or an
appearance flag must be given. Media filenames are names already present on the
cooler; upload local files first with `pctl upload <local-file>` or
`pctl library-import <directory>`. Anything you omit carries the previously
saved value forward; an explicit flag overrides it. An overlay-only change
requires media to already be on the cooler.

| Argument / flag      | Description                                                                                                                                                                                                                                                                                                                                                                         |
|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `<filename>`         | Media filename already on the cooler. Optional if you are only changing the overlay or pane-2 content.                                                                                                                                                                                                                                                                              |
| `--metrics`          | Comma-separated metric tokens, 0-3 (see table below). Pass `--metrics` with **no value** to remove the overlay; omit it entirely to leave the overlay unchanged.                                                                                                                                                                                                                    |
| `--metrics-color`    | Overlay text color: `#RRGGBB` hex (or bare `RRGGBB`), or an `R,G,B` triple such as `255,0,0`.                                                                                                                                                                                                                                                                                       |
| `--metrics-align`    | Overlay horizontal alignment: `left`, `center`, or `right`.                                                                                                                                                                                                                                                                                                                         |
| `--metrics-position` | Overlay vertical position: `top`, `center`, or `bottom`.                                                                                                                                                                                                                                                                                                                            |
| `--filter`           | Firmware-rendered display effect: `none`, `smoke`, or `rain`. Omit it to keep the saved effect.                                                                                                                                                                                                                                                                                     |
| `--ratio`            | Screen aspect ratio: `2:1` (the cooler's native landscape ratio) or `1:1` (halves the screen into two squares). Default is mode-driven — full-screen uses `2:1`, split uses `1:1` — so most invocations don't need this flag. Setting `1:1` in full-screen renders the media as a square in half the screen, leaving the other half black; the firmware honors the value literally. |
| `--split`            | Render in split-screen mode (two panes side-by-side). Required for `--media2` / `--metrics2`. At least one pane must end up with media.                                                                                                                                                                                                                                             |
| `--media2`           | Pane-2 media filename already on the cooler (split only). Pass `--media2` with **no value** to clear pane 2 (dark); omit it to keep whatever is saved.                                                                                                                                                                                                                              |
| `--metrics2`         | Pane-2 metrics overlay (split only). Same syntax as `--metrics`.                                                                                                                                                                                                                                                                                                                    |

```bash
# Upload a local file, then show the uploaded filename
pctl upload clip.mp4
pctl display clip.mp4

# Show CPU and GPU temperature over whatever is on screen
pctl display --metrics cpu-temp,gpu-temp

# Restyle the overlay without changing the media
pctl display --metrics-color 255,0,0 --metrics-align right --metrics-position top

# Add or remove firmware display effects
pctl display --filter smoke
pctl display --filter rain
pctl display --filter none

# Remove the overlay
pctl display --metrics

# Split-screen with two clips, each pane its own metrics, 1:1 ratio
pctl display left.mp4 --split --media2 right.mp4 \
  --metrics cpu-temp --metrics2 gpu-temp --ratio 1:1

# Split with only one pane filled (other goes dark)
pctl display left.mp4 --split

# Leave split: any invocation without --split returns to full screen
pctl display clip.mp4
```

In split mode the two panes share appearance flags (`--metrics-color`,
`--metrics-align`, `--metrics-position`, `--filter`) — independent per-pane
styling is not yet exposed.

#### Metrics overlay tokens

The cooler renders at most **3** metrics at once.

| Token         | Shown on screen as   |
|---------------|----------------------|
| `cpu-temp`    | CPU Temperature      |
| `cpu-usage`   | CPU Usage            |
| `cpu-freq`    | CPU Frequency        |
| `gpu-temp`    | GPU Temperature      |
| `gpu-usage`   | GPU Usage            |
| `gpu-freq`    | GPU Frequency        |
| `gpu-voltage` | GPU Voltage          |
| `mem-usage`   | Memory Utilization   |
| `datetime`    | Date & Time          |

The metric values themselves are pushed by `pctl daemon` — run the daemon to
keep them live.

For the hardware-verified firmware label/effect list, see
[`display-capabilities.md`](display-capabilities.md).

### `pctl brightness`

Set screen brightness.

```
pctl brightness <0-100>
```

### `pctl fan-lcd <0-100>`

Set fan-LCD cooling speed. Values passed are percentages.

```
pctl fan-lcd <0-100>
```

### `pctl fan-rpm`

Read current fan and pump RPM.

```
pctl fan-rpm
pctl fan-rpm --watch <seconds>
```

- Without `--watch`, prints one RPM sample and exits.
- With `--watch`, polls every N seconds until interrupted.

### `pctl sleep`

Set screen sleep mode — whether the screen sleeps when the cooler is idle.

```
pctl sleep <on|off>
```

- `on` lets the screen sleep after the device's idle timeout once the device
  is no longer being actively refreshed.
- `off` keeps the screen on after the idle timeout once the device is no
  longer being actively refreshed.

Requires media to already be on the cooler (run `pctl upload <local-file>`,
then `pctl list` to confirm upload succeeded, then run
`pctl display <filename>`).
A running keepalive daemon keeps the device active while the host is awake, so
this setting is most visible once refresh traffic stops — for example, when the
daemon is stopped or the host suspends.

## Media management

### `pctl upload`

Upload a single local media file to the cooler.

```
pctl upload <local-file>
```

Non-MP4 video (WebM, MKV, AVI, MOV, GIF) is converted to MP4 with `ffmpeg`
before the push; MP4 and still images are pushed as-is. Use `pctl list` to see
the uploaded filename, then show it with `pctl display <filename>`.

### `pctl library-import`

Batch-upload every media file in a directory (top level only — subdirectories
are skipped).

```
pctl library-import <directory>
```

Non-media files are skipped and counted in the summary.

### `pctl list`

List the media files currently on the device.

```
pctl list
```

### `pctl delete`

Delete media files from the device.

```
pctl delete <file>...           # by name
pctl delete '<pattern>'         # by glob
pctl delete --all [--yes]       # everything
```

| Flag          | Description                                                                        |
|---------------|------------------------------------------------------------------------------------|
| `<file>...`   | One or more filenames or glob patterns.                                            |
| `--all`       | Delete every media file on the device. Cannot be combined with filename arguments. |
| `--yes`, `-y` | Skip the confirmation prompt (only affects `--all`).                               |

Glob support is straightforward: `prefix*`, `*suffix`, and `prefix*suffix`. Path
separators and `..` are rejected — only files in `/sdcard/pcMedia/` can be
deleted.

```bash
pctl delete clip.mp4
pctl delete clip1.mp4 clip2.mp4
pctl delete 'stats_*'
pctl delete --all --yes
```

Pass multiple filenames as separate arguments, not as a comma-separated list.
For example, use `pctl delete clip1.mp4 clip2.mp4`, not
`pctl delete clip1.mp4,clip2.mp4`.

## Device control

### `pctl reboot`

Reboot the cooler.

```
pctl reboot
```

## Configuration

`pctl` persists settings to an XDG config file
(`~/.config/tryx-panorama-mgr/config.json` by default).

### `pctl config show`

Print the current configuration and its file path.

```
pctl config show
```

### `pctl config set`

Update a single configuration value.

```
pctl config set <key> <value>
```

| Key                  | Value                                      | Default      |
|----------------------|--------------------------------------------|--------------|
| `port`               | Serial port path, or empty for auto-detect | empty (auto) |
| `brightness`         | Integer 0-100                              | `75`         |
| `keepalive-interval` | Integer ≥ 1, in **seconds**                | `2`          |
| `fan-lcd-percent`    | Integer 0-100                              | `30`         |

```bash
pctl config set brightness 60
pctl config set keepalive-interval 5
```

## Daemon

### `pctl daemon`

Run the foreground keepalive + metrics loop.

```
pctl daemon
```

The daemon holds the serial connection open and, every `keepalive-interval`
seconds, sends a handshake plus a system-metrics frame. It also exposes a
user-scoped Unix socket at `$XDG_RUNTIME_DIR/panorama/pctl.sock`, and
foreground commands such as `info`, `doctor`, `brightness`, `fan-lcd`,
`fan-rpm`, `display`, `sleep`, and `reboot` automatically use that socket when
it is available. This keeps pushed media and the live metrics overlay on screen
instead of letting the cooler revert to its firmware default, without forcing
other commands to fight the daemon for the serial port.

It is intended to run as a systemd user service — see the README's
[installation section](../README.md#installation) and the end-user unit file
`packaging/panorama.service`.
The daemon logs to stderr (captured by the journal under systemd). For a
foreground daemon, set `RUST_LOG` before running `pctl daemon` to change the log
level. For the systemd service, set the log level with a user-service override
because the unit defines its own `Environment=RUST_LOG=info`.
It exits cleanly on `SIGINT`/`SIGTERM`.

```bash
systemctl --user edit panorama
# Add:
# [Service]
# Environment=RUST_LOG=debug
systemctl --user restart panorama
```

If you rebuild or reinstall `pctl` with `cargo install`, restart the service so
its long-running daemon process picks up the new binary:

```bash
systemctl --user restart panorama
```

## Troubleshooting

| Symptom                            | Try                                                                                                                            |
|------------------------------------|--------------------------------------------------------------------------------------------------------------------------------|
| "No device detected by adb"        | Run `pctl doctor`. Often a stale adb server — `adb kill-server` and retry. Check the USB cable and that the cooler is powered. |
| Serial connection fails            | Install the udev rule — see the README's installation section.                                                                 |
| `adb` / `ffmpeg` "not found"       | Install the runtime dependencies listed in the README.                                                                         |
| Overlay shows but values are stale | Run `pctl daemon` — it pushes the live metric values.                                                                          |

## See also

- [README](../README.md) — installation and overview
- [docs/development.md](development.md) — building and contributing
- [docs/adb-protocol.md](adb-protocol.md) — the wire protocol
