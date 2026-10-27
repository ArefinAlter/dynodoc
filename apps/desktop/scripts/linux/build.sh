#!/bin/bash
# Runs inside the build image (see Dockerfile). /src.tar is the repository (git archive), /out receives the
# assets. Everything is built on the container's own disk (a bind mount is slow for a Rust build).
#   git archive --format=tar HEAD -o src.tar
#   docker run --rm --cpus=4 -v <src.tar>:/src.tar:ro -v <out>:/out dynodoc-linux-build bash -c 'mkdir -p /work/repo && tar -xf /src.tar -C /work/repo apps/desktop/scripts/linux/build.sh && bash /work/repo/apps/desktop/scripts/linux/build.sh'
set -euo pipefail
mkdir -p /work/repo && cd /work/repo
# The committed repository, as a tar file (copying a mounted folder file by file is slow).
tar -xf /src.tar -C /work/repo
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
