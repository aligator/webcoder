# Webcoder

A browser-based media transcoder. Webcoder runs [FFmpeg](https://ffmpeg.org/)
entirely client-side via WebAssembly — no uploads, no server-side processing.
The UI is built in Rust with [Yew](https://yew.rs/) and compiled to WASM.

> **Credit:** Webcoder is inspired by and derived from
> [**Nmkoder** by n00mkrad](https://github.com/n00mkrad/nmkoder), a desktop
> FFmpeg front-end. This project reimagines it as a self-contained web app.

## Features

- Client-side probing and transcoding — media never leaves the browser
- FFmpeg + ffprobe via [`@ffmpeg/ffmpeg`](https://github.com/ffmpegwasm/ffmpeg.wasm)
- Drag-and-drop input, live encode log, downloadable output

## Development

Requires the Rust toolchain and [Trunk](https://trunkrs.dev/).

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

trunk serve            # dev server with live reload
trunk build --release  # production bundle in ./dist
```

## Docker

The provided multi-stage `Dockerfile` builds the WASM bundle and serves it with
nginx (unprivileged, on port 8080).

```sh
docker build -t webcoder .
docker run --rm -p 8080:8080 webcoder
# open http://localhost:8080
```

## Releases

Pushing a `v*` tag triggers [`.github/workflows/release.yml`](.github/workflows/release.yml), which:

1. Builds and pushes a container image to GHCR
   (`ghcr.io/<owner>/<repo>`), tagged with the semver version and `latest`.
2. Builds the static bundle, packages it as `webcoder-<tag>.tar.gz`, and
   attaches it to an auto-generated GitHub Release.

```sh
git tag v0.1.0
git push origin v0.1.0
```

## License

See the upstream [Nmkoder](https://github.com/n00mkrad/nmkoder) project for the
original work this is based on.
