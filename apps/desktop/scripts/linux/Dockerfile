# The build image for the Linux desktop assets (AppImage and .deb): Ubuntu 22.04, so the programs
# need glibc 2.35 or newer. Nothing of the product is in the image; the source is mounted by build.sh.
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential curl ca-certificates git pkg-config xz-utils unzip zstd file patchelf wget fakeroot \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libayatana-appindicator3-dev libsecret-1-dev libssl-dev \
      libfuse2 desktop-file-utils dpkg-dev squashfs-tools \
      xvfb xauth x11-utils imagemagick dbus dbus-x11 gnome-keyring libgl1-mesa-dri \
    && rm -rf /var/lib/apt/lists/*
# Node 20 (the page's build tools) and a non-root user: PostgreSQL refuses to run as root, and the
# smoke test starts the real app.
RUN curl -fsSL https://nodejs.org/dist/v20.18.1/node-v20.18.1-linux-x64.tar.xz | tar -xJ -C /usr/local --strip-components=1
RUN useradd -m builder
USER builder
ENV PATH=/home/builder/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
WORKDIR /work
