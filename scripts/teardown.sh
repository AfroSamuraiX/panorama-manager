#!/usr/bin/env bash

set -euo pipefail

install_dir="${INSTALL_DIR:-$HOME/.local/bin}"
binary_path="${install_dir}/pctl"
gui_binary_path="${install_dir}/panorama-gui"
service_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
service_path="${service_dir}/panorama.service"
service_wants_path="${service_dir}/default.target.wants/panorama.service"
applications_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
desktop_entry_path="${applications_dir}/panorama-mgr.desktop"
icons_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
icon_path="${icons_dir}/panorama-mgr.png"
udev_rule_path="${UDEV_RULE_PATH:-/etc/udev/rules.d/70-tryx-panorama.rules}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

remove_path() {
  local path="$1"
  local label="$2"

  if [ -e "$path" ] || [ -L "$path" ]; then
    rm -f "$path"
    printf 'Removed %s: %s\n' "$label" "$path"
  else
    printf 'Not present: %s\n' "$path"
  fi
}

if [ "$(id -u)" -eq 0 ]; then
  printf 'Do not run this teardown script with sudo; it removes user-scoped files for the current user.\n' >&2
  printf 'The script prompts for sudo only when removing the udev rule.\n' >&2
  exit 1
fi

printf 'Tearing down panorama-mgr manual install\n'

if command -v systemctl >/dev/null 2>&1; then
  if systemctl --user disable --now panorama.service >/dev/null 2>&1; then
    printf 'Disabled and stopped user service: panorama.service\n'
  else
    printf 'User service was not active/enabled or systemd user manager is unavailable.\n'
  fi
else
  printf 'systemctl not found; skipping user service stop/disable.\n'
fi

remove_path "$service_path" "user service"
remove_path "$service_wants_path" "user service enablement link"

if command -v systemctl >/dev/null 2>&1; then
  if systemctl --user daemon-reload >/dev/null 2>&1; then
    printf 'Reloaded systemd user manager.\n'
  else
    printf 'Could not reload systemd user manager; continuing.\n'
  fi
fi

remove_path "$binary_path" "binary"
remove_path "$gui_binary_path" "gui binary"
remove_path "$desktop_entry_path" "desktop entry"
remove_path "$icon_path" "icon"

if [ -e "$udev_rule_path" ] || [ -L "$udev_rule_path" ]; then
  require_cmd sudo
  printf 'Removing udev rule with sudo: %s\n' "$udev_rule_path"
  sudo rm -f "$udev_rule_path"
  printf 'Removed udev rule: %s\n' "$udev_rule_path"

  if command -v udevadm >/dev/null 2>&1; then
    sudo udevadm control --reload-rules
    sudo udevadm trigger --action=add --subsystem-match=usb --subsystem-match=tty
    printf 'Reloaded and re-applied udev rules.\n'
  else
    printf 'udevadm not found; reboot or reload udev rules manually before reconnecting the cooler.\n'
  fi
else
  printf 'Not present: %s\n' "$udev_rule_path"
fi

printf 'Teardown complete. User config and display state were left in place.\n'
