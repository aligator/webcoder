# syntax=docker/dockerfile:1

########################################
# Stage 1 — build the WASM frontend + native app
########################################
# Build for the target platform so the native `server` binary matches the
# runtime architecture. (The WASM bundle is arch-independent.)
FROM --platform=$TARGETPLATFORM rust:1.95-trixie AS builder

ARG TRUNK_VERSION=0.21.14
ARG TARGETARCH

RUN case "${TARGETARCH}" in \
      amd64) TRUNK_ARCH=x86_64 ;; \
      arm64) TRUNK_ARCH=aarch64 ;; \
      *) echo "unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
 && rustup target add wasm32-unknown-unknown \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-${TRUNK_ARCH}-unknown-linux-gnu.tar.gz" \
      | tar -xzf - -C /usr/local/bin trunk

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      patchelf \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

# Frontend bundle (WASM) and the release native app binary.
RUN trunk build --release --dist dist \
 && cargo build --release -p webcoder-desktop --bin webcoder

########################################
# Stage 2 — runtime: system FFmpeg + app in headless mode
########################################
FROM debian:trixie-slim AS runtime

# ffmpeg brings full native codec support (AV1 decode via libdav1d, etc.).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ffmpeg \
      ca-certificates \
      wget \
      libwebkit2gtk-4.1-0 \
      libayatana-appindicator3-1 \
      librsvg2-2 \
 && rm -rf /var/lib/apt/lists/*

# Unprivileged runtime user; work/temp dirs it can write.
RUN useradd --system --create-home --uid 10001 webcoder \
 && mkdir -p /app/work \
 && chown -R webcoder:webcoder /app

WORKDIR /app
COPY --from=builder /app/dist /app/dist
COPY --from=builder /app/target/release/webcoder /usr/local/bin/webcoder

USER webcoder

ENV WEBCODER_ADDR=0.0.0.0:8080 \
    WEBCODER_DIST=/app/dist \
    WEBCODER_WORKDIR=/app/work

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1

STOPSIGNAL SIGTERM

CMD ["webcoder", "--headless"]
