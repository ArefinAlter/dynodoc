#!/bin/bash
# Builds the Linux assets (AppImage, .deb, engine-cli) from a tar of the committed sources, on the local
# disk of a Linux system (a bind mount of a Windows folder is far too slow for a Rust build).
#   git archive --format=tar HEAD -o src.tar
#   In the Docker image (Dockerfile): docker run --rm -v <src.tar>:/src.tar:ro -v <out>:/out <image> bash <this file>
#   In WSL (Ubuntu 22.04 with the tools of the Dockerfile): SRC_TAR=/mnt/d/.../src.tar WORK=/root/work OUT=/mnt/d/.../linux bash build.sh
set -euo pipefail
# Where things are: the defaults suit the Docker image; a WSL build sets them (see above).
SRC_TAR=${SRC_TAR:-/src.tar}
WORK=${WORK:-/work}
OUT=${OUT:-/out}
export PATH="$HOME/.cargo/bin:$PATH" APPIMAGE_EXTRACT_AND_RUN=1
rm -rf "$WORK/repo/apps/desktop/src-tauri/resources/pg" "$WORK/repo/apps/desktop/src-tauri/target/release/bundle"
mkdir -p "$WORK/repo" && cd "$WORK/repo"
# The committed repository, as a tar file (copying a mounted folder file by file is slow).
tar -xf "$SRC_TAR" -C "$WORK/repo"
cd apps/desktop
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install "$(grep -oP 'channel\s*=\s*"\K[^"]+' ../../rust-toolchain.toml)" --profile minimal
npm ci --ignore-scripts
node scripts/prepare-sidecar.mjs
node scripts/prepare-pg.mjs --platform linux
node client/build.mjs
# What the database programs load, and from where (the bundle's own lib folder or the system).
for b in postgres initdb pg_ctl; do
  echo "--- ldd $b"; ldd src-tauri/resources/pg/bin/$b | sed "s#$WORK/repo/apps/desktop/src-tauri/resources/pg#PG#"
  if ldd src-tauri/resources/pg/bin/$b | grep -q "not found"; then echo "a library is missing for $b"; exit 1; fi
done
npm run tauri -- build --bundles appimage,deb --config src-tauri/tauri.bundle.conf.json
mkdir -p "$OUT"
cp -v src-tauri/target/release/bundle/appimage/*.AppImage src-tauri/target/release/bundle/deb/*.deb "$OUT/"
cp -v src-tauri/binaries/engine-cli-x86_64-unknown-linux-gnu "$OUT/engine-cli_0.0.1b_linux-x64"
