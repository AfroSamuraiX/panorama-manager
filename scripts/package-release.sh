#!/usr/bin/env bash

set -euo pipefail

version="${PANORAMA_VERSION:-dev}"
target="linux-x86_64"
build_gui="${PANORAMA_BUILD_GUI:-0}"

if [ "${1:-}" = "--gui" ]; then
  build_gui=1
fi

mkdir -p dist

printf 'Building pctl release binary...\n'
cargo build --release -p panorama-ctl

pctl_archive="pctl-${version}-${target}.tar.gz"
pctl_checksum="pctl-${version}-${target}.sha256"
rm -f "dist/${pctl_archive}" "dist/${pctl_checksum}"
install -m 0755 target/release/pctl dist/pctl
tar -C dist -czf "dist/${pctl_archive}" pctl
(
  cd dist
  sha256sum "${pctl_archive}" > "${pctl_checksum}"
  rm -f pctl
)
printf 'Wrote dist/%s\n' "${pctl_archive}"

if [ "${build_gui}" != "1" ]; then
  printf 'Skipping GUI artifact. Re-run with --gui or PANORAMA_BUILD_GUI=1 to include it.\n'
  exit 0
fi

printf 'Building panorama-gui release binary...\n'
(
  cd crates/panorama-gui
  # The GUI build toolchain lives in devDependencies. Install it transiently
  # for CI/local packaging, but only ship the built binary and launcher assets.
  npm ci --include=dev --no-audit --no-fund
  npm run tauri:build
)

gui_archive="panorama-gui-${version}-${target}.tar.gz"
gui_checksum="panorama-gui-${version}-${target}.sha256"
gui_stage="dist/panorama-gui-${version}-${target}"
rm -rf "${gui_stage}" "dist/${gui_archive}" "dist/${gui_checksum}"
mkdir -p "${gui_stage}"
install -m 0755 crates/panorama-gui/src-tauri/target/release/panorama-gui "${gui_stage}/panorama-gui"
install -m 0644 crates/panorama-gui/src/lib/assets/aio-icon.png "${gui_stage}/panorama-mgr.png"
install -m 0644 packaging/panorama-mgr.desktop "${gui_stage}/panorama-mgr.desktop"
tar -C "${gui_stage}" -czf "dist/${gui_archive}" panorama-gui panorama-mgr.png panorama-mgr.desktop
(
  cd dist
  sha256sum "${gui_archive}" > "${gui_checksum}"
)
rm -rf "${gui_stage}"
printf 'Wrote dist/%s\n' "${gui_archive}"
