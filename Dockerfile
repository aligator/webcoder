# syntax=docker/dockerfile:1

########################################
# Stage 1 — build the WASM bundle
########################################
FROM --platform=$BUILDPLATFORM rust:1.95-bookworm AS builder

# Pin trunk for reproducibility. Trunk auto-downloads the matching
# wasm-bindgen-cli at build time, so we don't install it separately.
ARG TRUNK_VERSION=0.21.14

# Provided by BuildKit; arch of the build host (matches BUILDPLATFORM above).
ARG BUILDARCH

# wasm target + trunk (prebuilt binary; avoids compiling trunk from source).
# Pick the trunk asset matching the build host architecture.
RUN case "${BUILDARCH}" in \
      amd64) TRUNK_ARCH=x86_64 ;; \
      arm64) TRUNK_ARCH=aarch64 ;; \
      *) echo "unsupported arch: ${BUILDARCH}" >&2; exit 1 ;; \
    esac \
 && rustup target add wasm32-unknown-unknown \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-${TRUNK_ARCH}-unknown-linux-gnu.tar.gz" \
      | tar -xzf - -C /usr/local/bin trunk

WORKDIR /app

# Cache dependency compilation: copy manifests, fetch, then copy sources.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
 && cargo fetch --target wasm32-unknown-unknown \
 && rm -rf src

# Real sources + web assets.
COPY . .

# Trunk reads index.html; outputs to dist/.
RUN trunk build --release --dist dist

########################################
# Stage 2 — serve static bundle
########################################
FROM nginx:1.27-alpine AS runtime

# Non-root: nginx:alpine already ships an unprivileged setup on :8080 via
# the templated default, but we bring our own conf and run as the nginx user.
RUN rm -f /etc/nginx/conf.d/default.conf

COPY nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /app/dist /usr/share/nginx/html

# Drop privileges: 8080 is unprivileged, temp/pid paths live under /tmp.
USER nginx

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1

STOPSIGNAL SIGQUIT

CMD ["nginx", "-g", "daemon off;"]
