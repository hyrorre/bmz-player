FROM ubuntu:22.04 AS build
ENV DEBIAN_FRONTEND=noninteractive
RUN sed -i 's/^# deb-src /deb-src /' /etc/apt/sources.list \
    && apt-get update && apt-get install -y --no-install-recommends \
       build-essential ca-certificates curl git pkg-config clang libclang-dev \
       nasm xz-utils python3 libasound2-dev libpulse-dev libudev-dev \
       libfontconfig1-dev libdbus-1-dev libxkbcommon-dev zlib1g-dev libbz2-dev
RUN curl --fail --location --retry 3 https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup component add rustfmt clippy \
    && cargo install --locked --features cli cargo-about
COPY build-ffmpeg.sh /opt/build-ffmpeg.sh
RUN bash /opt/build-ffmpeg.sh
RUN apt-get update && apt-get install -y --no-install-recommends patchelf
ENV PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig
ENV LD_LIBRARY_PATH=/opt/ffmpeg/lib
WORKDIR /work

# Independent base: no compiler, FFmpeg, or build-stage filesystem is inherited.
FROM ubuntu:22.04 AS verify
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
       python3 strace xvfb xauth mesa-vulkan-drivers libvulkan1 \
       libx11-6 libxcursor1 libxi6 libxrandr2 libxkbcommon0 libxkbcommon-x11-0 \
       libasound2 libasound2-plugins libpulse0 pulseaudio \
       libfontconfig1 libudev1 ca-certificates \
    && useradd --create-home --uid 1000 player
COPY verify.py /opt/verify.py
USER player
ENV HOME=/home/player
WORKDIR /home/player
CMD ["python3", "/opt/verify.py"]
