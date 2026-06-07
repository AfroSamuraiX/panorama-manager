#!/usr/bin/env bash

set -euo pipefail

repo="${PANORAMA_REPO:-AfroSamuraiX/panorama-manager}"
install_dir="${INSTALL_DIR:-$HOME/.local/bin}"
version="${PANORAMA_VERSION:-}"
service_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
service_path="${service_dir}/panorama.service"
applications_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
os_id=""
os_version_id=""

if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  os_id="${ID:-}"
  os_version_id="${VERSION_ID:-}"
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

require_cmd curl
require_cmd cut
require_cmd grep
require_cmd install
require_cmd mktemp
require_cmd sed
require_cmd sha256sum
require_cmd systemctl
require_cmd tar
require_cmd sudo

detect_package_manager() {
  if command -v rpm-ostree >/dev/null 2>&1; then
    printf 'rpm-ostree'
    return
  fi
  if command -v dnf >/dev/null 2>&1; then
    printf 'dnf'
    return
  fi
  if command -v pacman >/dev/null 2>&1; then
    printf 'pacman'
    return
  fi
  if command -v apt-get >/dev/null 2>&1; then
    printf 'apt'
    return
  fi
  printf 'unknown'
}

fetch_release_metadata() {
  local tag="$1"
  curl -fsSL "https://api.github.com/repos/${repo}/releases/tags/${tag}"
}

release_has_asset() {
  local metadata="$1"
  local asset_name="$2"
  printf '%s' "$metadata" | grep -E "\"name\"[[:space:]]*:[[:space:]]*\"${asset_name}\"" >/dev/null 2>&1
}

verify_release_assets() {
  local metadata="$1"
  local missing=0

  for asset_name in "$archive" "$checksum" "$gui_archive" "$gui_checksum"; do
    if ! release_has_asset "$metadata" "$asset_name"; then
      printf 'Release %s is missing required asset: %s\n' "$version" "$asset_name" >&2
      missing=1
    fi
  done

  if [ "$missing" -ne 0 ]; then
    printf 'This installer requires complete release assets for both pctl and panorama-gui.\n' >&2
    printf 'Check: https://github.com/%s/releases/tag/%s\n' "$repo" "$version" >&2
    exit 1
  fi
}

print_runtime_dependency_hint() {
  package_manager="$1"

  printf 'Install the required runtime dependencies, then re-run the installer.\n' >&2

  case "$package_manager" in
    pacman)
      printf '  sudo pacman -S android-tools ffmpeg webkit2gtk-4.1\n' >&2
      printf 'Optional GPU badge fallback:\n  sudo pacman -S mesa-utils\n' >&2
      ;;
    apt)
      if [ "$os_id" = "ubuntu" ] && [ "$os_version_id" = "24.04" ]; then
        printf '  sudo apt install adb ffmpeg libwebkit2gtk-4.1-0 libgtk-3-0t64\n' >&2
        printf 'Optional GPU badge fallback:\n  sudo apt install mesa-utils\n' >&2
      else
        printf '  sudo apt install adb ffmpeg\n' >&2
        printf "Install your release's WebKitGTK 4.1 and GTK 3 runtime packages as well.\n" >&2
      fi
      ;;
    dnf)
      printf '  sudo dnf install android-tools ffmpeg-free webkit2gtk4.1\n' >&2
      printf 'Optional GPU badge fallback:\n  sudo dnf install mesa-demos\n' >&2
      ;;
    rpm-ostree)
      printf '  sudo rpm-ostree install android-tools ffmpeg-free webkit2gtk4.1\n' >&2
      printf 'Optional GPU badge fallback:\n  sudo rpm-ostree install mesa-demos\n' >&2
      printf 'rpm-ostree package layering requires a reboot after the new deployment is created.\n' >&2
      ;;
    *)
      printf 'Install `adb`, `ffmpeg`, and the GTK/WebKitGTK runtime packages for your distro.\n' >&2
      ;;
  esac
}

verify_runtime_dependencies() {
  package_manager="$1"
  status=0

  if ! command -v adb >/dev/null 2>&1; then
    printf 'Missing required runtime command: adb\n' >&2
    status=1
  fi

  if ! command -v ffmpeg >/dev/null 2>&1; then
    printf 'Missing required runtime command: ffmpeg\n' >&2
    status=1
  fi

  if [ "$status" -ne 0 ]; then
    print_runtime_dependency_hint "$package_manager"
    exit 1
  fi
}

verify_gui_runtime_dependencies() {
  package_manager="$1"
  gui_binary="$2"

  if ! command -v ldd >/dev/null 2>&1; then
    return
  fi

  missing_gui_libs="$(ldd "$gui_binary" 2>/dev/null | grep 'not found' || true)"
  if [ -n "$missing_gui_libs" ]; then
    printf 'Missing required GUI runtime libraries:\n%s\n' "$missing_gui_libs" >&2
    print_runtime_dependency_hint "$package_manager"
    exit 1
  fi
}

package_manager="$(detect_package_manager)"

if [ "$(id -u)" -eq 0 ]; then
  printf 'Do not run this installer with sudo; it installs a user-scoped binary and service.\n' >&2
  exit 1
fi

verify_runtime_dependencies "$package_manager"

if [ -z "$version" ]; then
  latest_release="$(curl -fsSL "https://api.github.com/repos/${repo}/releases/latest")" || {
    printf 'Could not resolve the latest %s release.\n' "$repo" >&2
    exit 1
  }
  version="$(printf '%s\n' "$latest_release" \
    | grep -m1 '"tag_name"' \
    | cut -d '"' -f4 \
    || true)"
fi

if [ -z "$version" ]; then
  printf 'Could not resolve the latest %s release.\n' "$repo" >&2
  exit 1
fi

archive="pctl-${version}-linux-x86_64.tar.gz"
checksum="pctl-${version}-linux-x86_64.sha256"
gui_archive="panorama-gui-${version}-linux-x86_64.tar.gz"
gui_checksum="panorama-gui-${version}-linux-x86_64.sha256"
base_url="https://github.com/${repo}/releases/download/${version}"
tmpdir="$(mktemp -d)"

release_metadata="$(fetch_release_metadata "$version")" || {
  printf 'Could not resolve release metadata for %s tag %s.\n' "$repo" "$version" >&2
  exit 1
}
verify_release_assets "$release_metadata"

cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

printf 'Installing pctl %s to %s\n' "$version" "$install_dir"

curl -fL -o "${tmpdir}/${archive}" "${base_url}/${archive}"
curl -fL -o "${tmpdir}/${checksum}" "${base_url}/${checksum}"

(
  cd "$tmpdir"
  sha256sum -c "$checksum"
  tar -xzf "$archive"
)

mkdir -p "$install_dir"
install -m 0755 "${tmpdir}/pctl" "${install_dir}/pctl"

printf 'Installed: %s\n' "${install_dir}/pctl"

printf 'Installing panorama-gui %s to %s\n' "$version" "$install_dir"

curl -fL -o "${tmpdir}/${gui_archive}" "${base_url}/${gui_archive}"
curl -fL -o "${tmpdir}/${gui_checksum}" "${base_url}/${gui_checksum}"

(
  cd "$tmpdir"
  sha256sum -c "$gui_checksum"
  tar -xzf "$gui_archive"
)

verify_gui_runtime_dependencies "$package_manager" "${tmpdir}/panorama-gui"

install -m 0755 "${tmpdir}/panorama-gui" "${install_dir}/panorama-gui"
mkdir -p "$icons_dir" "$applications_dir"
install -m 0644 "${tmpdir}/panorama-mgr.png" "${icons_dir}/panorama-mgr.png"
install -m 0644 "${tmpdir}/panorama-mgr.desktop" "${applications_dir}/panorama-mgr.desktop"
desktop_exec="${install_dir}/panorama-gui"
desktop_exec_escaped="${desktop_exec//&/\\&}"
desktop_exec_escaped="${desktop_exec_escaped//|/\\|}"
sed -i "s|^Exec=.*$|Exec=${desktop_exec_escaped}|" "${applications_dir}/panorama-mgr.desktop"

printf 'Installed: %s\n' "${install_dir}/panorama-gui"
printf 'Installed desktop entry: %s\n' "${applications_dir}/panorama-mgr.desktop"

printf 'Installing udev rule with pctl setup\n'
sudo "${install_dir}/pctl" setup

printf 'Installing systemd user service: %s\n' "$service_path"
mkdir -p "$service_dir"
cat > "$service_path" <<EOF
[Unit]
Description=TRYX Panorama cooler keepalive and metrics daemon
After=graphical-session.target

[Service]
Type=notify
NotifyAccess=main
ExecStart=${install_dir}/pctl daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now panorama.service

printf 'Installed: %s\n' "${install_dir}/pctl"
printf 'Enabled service: %s\n' "$service_path"
printf 'Installed GUI: %s\n' "${install_dir}/panorama-gui"
printf 'Run %s/pctl doctor to verify the device setup.\n' "$install_dir"
