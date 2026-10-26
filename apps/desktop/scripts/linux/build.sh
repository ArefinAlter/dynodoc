#!/bin/bash
# Runs inside the build image (see Dockerfile). /src is the repository (read-only), /out receives the
# assets. Everything is built on the container's own disk (a bind mount is slow for a Rust build).
#   docker run --rm --cpus=4 -v <repo>:/src:ro -v <out>:/out dynodoc-linux-build bash /src/apps/desktop/scripts/linux/build.sh
set -euo pipefail
mkdir -p /work/repo && cd /work/repo
# The repository without its heavy or generated folders.
tar -C /src --exclude=node_modules --exclude=target --exclude=build --exclude=.git --exclude=src-tauri/binaries --exclude=src-tauri/resources/pg -cf - . | tar -xf -
cd apps/desktop
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install "$(grep -oP 'channel\s*=\s*"\K[^"]+' ../../rust-toolchain.toml)" --profile minimal
npm ci --ignore-scripts
node scripts/prepare-sidecar.mjs
node scripts/prepare-pg.mjs --platform linux
node client/build.mjs
npm run tauri -- build --bundles appimage,deb --config src-tauri/tauri.bundle.conf.json
mkdir -p /out
cp -v src-tauri/target/release/bundle/appimage/*.AppImage src-tauri/target/release/bundle/deb/*.deb /out/
