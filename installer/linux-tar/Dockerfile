# syntax=docker/dockerfile:1
FROM almalinux:8.10 AS tools
# EL8's platform Python/GCC are too old for PipeWire and current Rust crates.
# Both the build and clean offline rebuild inherit exactly these tools.
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; \
      echo 'sslcacert=/run/secrets/proxy_ca' >> /etc/dnf/dnf.conf; fi; \
    dnf -y install dnf-plugins-core epel-release && \
    dnf config-manager --set-enabled powertools && \
    dnf -y update && \
    dnf -y install gcc-toolset-14-gcc gcc-toolset-14-gcc-c++ make git curl \
      ca-certificates pkgconf-pkg-config clang clang-devel perl xz bzip2 \
      python3.12 python3.12-pip binutils rpm-build cpio \
      alsa-lib-devel pulseaudio-libs-devel systemd-devel fontconfig-devel \
      dbus-devel libxkbcommon-devel zlib-devel bzip2-devel dbus-daemon && \
    sed -i '\|^sslcacert=/run/secrets/proxy_ca$|d' /etc/dnf/dnf.conf
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export PIP_CERT=/run/secrets/proxy_ca; fi; \
    python3.12 -m venv /opt/tools && \
    /opt/tools/bin/pip install --no-cache-dir meson==1.7.2 ninja==1.11.1.4
ENV PATH="/opt/tools/bin:/opt/rh/gcc-toolset-14/root/usr/bin:/root/.cargo/bin:${PATH}"
ENV CC=gcc CXX=g++ CFLAGS="-march=x86-64 -mtune=generic" CXXFLAGS="-march=x86-64 -mtune=generic"
ENV RUSTFLAGS="-C target-cpu=x86-64"
# The upstream release includes configure and builds with the EL8 tools.
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; fi; \
    curl --fail --location --retry 3 \
      https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/nasm-2.16.03.tar.xz \
      -o /tmp/nasm.tar.xz && \
    echo '1412a1c760bbd05db026b6c0d1657affd6631cd0a63cddb6f73cc6d4aa616148  /tmp/nasm.tar.xz' | sha256sum -c - && \
    tar -xf /tmp/nasm.tar.xz -C /tmp && cd /tmp/nasm-2.16.03 && \
    ./configure --prefix=/opt/tools && make -j2 && make install && \
    rm -rf /tmp/nasm.tar.xz /tmp/nasm-2.16.03
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; fi; \
    curl --fail --location --retry 3 https://sh.rustup.rs -o /tmp/rustup.sh && \
    sh /tmp/rustup.sh -y --profile minimal --default-toolchain stable
WORKDIR /work
COPY rpm-fixture-check.py source_packages.py manifest.py /opt/rpm-check/
RUN python3 /opt/rpm-check/rpm-fixture-check.py && rm -rf /opt/rpm-check

# No FFmpeg, PipeWire, Cargo cache, or original checkout in this image.
FROM tools AS source-verify
COPY verify-sources.py source_packages.py manifest.py /opt/
CMD ["python3", "/opt/verify-sources.py", "/archives/sources", "--rebuild"]

FROM tools AS build
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; fi; \
    rustup component add rustfmt clippy && cargo install --locked --features cli cargo-about
COPY build-ffmpeg.sh /opt/build-ffmpeg.sh
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; fi; \
    bash /opt/build-ffmpeg.sh
COPY build-pipewire.sh /opt/build-pipewire.sh
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CURL_CA_BUNDLE=/run/secrets/proxy_ca; fi; \
    bash /opt/build-pipewire.sh
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then echo 'sslcacert=/run/secrets/proxy_ca' >> /etc/dnf/dnf.conf; fi; \
    dnf -y install patchelf && \
    sed -i '\|^sslcacert=/run/secrets/proxy_ca$|d' /etc/dnf/dnf.conf && \
    runuser -u nobody -- dbus-daemon --version
ENV PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig:/opt/pipewire/lib/pkgconfig
ENV LD_LIBRARY_PATH=/opt/ffmpeg/lib:/opt/pipewire/lib

# Independent runtime bases: no compiler or build-stage files are inherited.
FROM almalinux:8.10 AS verify-almalinux
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then echo 'sslcacert=/run/secrets/proxy_ca' >> /etc/dnf/dnf.conf; fi; \
    dnf -y install python3.12 binutils strace xorg-x11-server-Xvfb \
      mesa-vulkan-drivers vulkan-loader libX11 libXcursor libXi libXrandr \
      libxkbcommon libxkbcommon-x11 alsa-lib alsa-plugins-pulseaudio \
      pulseaudio pulseaudio-utils fontconfig systemd-libs ca-certificates && \
    sed -i '\|^sslcacert=/run/secrets/proxy_ca$|d' /etc/dnf/dnf.conf && \
    useradd --create-home --uid 1000 player && \
    ln -s /usr/bin/python3.12 /usr/local/bin/python3
COPY verify.py elf.py /opt/
USER player
# EL8 Mesa disables lavapipe unless explicitly enabled for testing.
ENV HOME=/home/player RH_SW_VULKAN=1
WORKDIR /home/player
CMD ["python3", "/opt/verify.py"]

FROM ubuntu:22.04 AS verify-ubuntu
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export SSL_CERT_FILE=/run/secrets/proxy_ca; \
      echo 'Acquire::https::CaInfo "/run/secrets/proxy_ca";' > /etc/apt/apt.conf.d/99-proxy-ca; fi; \
    apt-get update && apt-get install -y --no-install-recommends \
      python3 binutils strace xvfb xauth mesa-vulkan-drivers libvulkan1 \
      libx11-6 libxcursor1 libxi6 libxrandr2 libxkbcommon0 libxkbcommon-x11-0 \
      libasound2 libasound2-plugins libpulse0 pulseaudio \
      libfontconfig1 libudev1 ca-certificates && \
    rm -f /etc/apt/apt.conf.d/99-proxy-ca && \
    useradd --create-home --uid 1000 player
COPY verify.py elf.py /opt/
USER player
ENV HOME=/home/player
WORKDIR /home/player
CMD ["python3", "/opt/verify.py"]
