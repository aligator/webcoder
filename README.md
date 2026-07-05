# Webcoder

A self-hosted and desktop media transcoder powered by [FFmpeg](https://ffmpeg.org/).
Pick codecs, quality and filters in the UI; encoding runs natively so the full
codec set is available (AV1, HEVC, …).

> Inspired by [**Nmkoder** by n00mkrad](https://github.com/n00mkrad/nmkoder).

`webcoder` is one binary with two modes:

- **desktop** (default) — Tauri app with a native file picker.
- **`--headless`** — HTTP server for Docker/self-hosting.

## Run

```sh
# Desktop / development
cargo tauri-dev

# Headless server → http://localhost:8080
cargo headless

# Docker
docker run --rm -p 8080:8080 -e WEBCODER_AUTH=me:secret ghcr.io/aligator/webcoder

# Flatpak (bundles its own FFmpeg)
flatpak-builder --force-clean --user --install build-dir flatpak/dev.webcoder.app.yml
flatpak run dev.webcoder.app
```

Requires the Rust toolchain, [Trunk](https://trunkrs.dev/) and `ffmpeg` on
`PATH` (except Flatpak, which bundles FFmpeg).

## Configuration

Headless mode reads environment variables — notably `WEBCODER_ADDR`
(default `127.0.0.1:8080`) and `WEBCODER_AUTH=user:pass` for HTTP Basic auth.
See [`src-tauri/src/server.rs`](src-tauri/src/server.rs) for the full list.

## License

Based on the upstream [Nmkoder](https://github.com/n00mkrad/nmkoder) project.
