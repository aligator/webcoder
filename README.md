# Webcoder

A self-hosted media transcoder. A Rust [axum](https://github.com/tokio-rs/axum)
backend runs the **native system [FFmpeg](https://ffmpeg.org/)** and serves a
Rust [Yew](https://yew.rs/) frontend compiled to WebAssembly. Files are uploaded
to the backend, probed and transcoded there, then downloaded — so the full
native codec set is available (AV1 decode via `libdav1d`, HEVC, etc.), unlike a
browser-only WASM build.

> **Credit:** Webcoder is inspired by and derived from
> [**Nmkoder** by n00mkrad](https://github.com/n00mkrad/nmkoder), a desktop
> FFmpeg front-end. This project reimagines it as a self-hosted web app.

## How it works

1. Browser uploads a file → `POST /api/jobs`. The server stores it in a
   per-job temp directory and returns the probed streams (`ffprobe`).
2. You pick per-track codecs, quality, filters, etc. in the UI.
3. `POST /api/jobs/:id/encode` rebuilds the FFmpeg command **from the structured
   settings only** and runs it. The output is fetched from
   `GET /api/jobs/:id/output`.
4. The available encoders come from the server's own FFmpeg
   (`GET /api/encoders`), so dropdowns only list codecs it can actually run.

## Security model

FFmpeg on a server is a powerful primitive, so the backend is deliberately
constrained (see [`src/server.rs`](src/server.rs)):

- **No shell.** FFmpeg is spawned with an explicit `argv` vector — no
  word-splitting, no injection.
- **No free-form arguments.** There is no "custom args" field. The entire
  command line is rebuilt from validated, typed settings
  ([`core::validate_job`](src/core.rs) + [`core::build_args`](src/core.rs)).
- **Path control.** Input/output paths are chosen by the server inside a
  throwaway per-job directory; client filenames only contribute a sanitized
  extension. No traversal or absolute-path escape.
- **Protocol lockdown.** `-nostdin` and an input `-protocol_whitelist file,pipe`
  block FFmpeg protocol tricks (`http`, `concat:`, `subfile:`, …).
- **Limits.** Upload size cap, job concurrency semaphore, wall-clock timeout,
  and TTL sweeping of job directories.
- **Optional auth.** Set `WEBCODER_AUTH=user:pass` for HTTP Basic auth on every
  route. TLS is expected to be terminated by a reverse proxy.

## Configuration

All via environment variables (sane defaults shown):

| Variable | Default | Purpose |
| --- | --- | --- |
| `WEBCODER_ADDR` | `127.0.0.1:8080` | Listen address |
| `WEBCODER_DIST` | `dist` | Directory of the built frontend |
| `WEBCODER_WORKDIR` | `<tmp>/webcoder` | Per-job scratch directory |
| `WEBCODER_MAX_UPLOAD_MB` | `4096` | Max upload size |
| `WEBCODER_JOB_TIMEOUT_SECS` | `3600` | Per-encode wall-clock timeout |
| `WEBCODER_JOB_TTL_SECS` | `3600` | Job dir lifetime before cleanup |
| `WEBCODER_MAX_CONCURRENT` | `2` | Concurrent encodes |
| `WEBCODER_AUTH` | _(unset)_ | `user:pass` to enable HTTP Basic auth |
| `WEBCODER_FFMPEG` / `WEBCODER_FFPROBE` | `ffmpeg` / `ffprobe` | Binary paths |

## Development

Requires the Rust toolchain, [Trunk](https://trunkrs.dev/), and `ffmpeg` +
`ffprobe` on `PATH`.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

trunk build            # build the frontend into ./dist
cargo run --bin server # start the backend (serves ./dist + the API)
# open http://localhost:8080
```

The project builds two binaries: `server` (native backend) and `webcoder` (the
Yew app, compiled to WASM by Trunk). `cargo test` covers the command builder in
[`src/core.rs`](src/core.rs).

## Docker

The multi-stage `Dockerfile` builds the WASM frontend and the native server,
then produces a `debian`-based runtime image with `ffmpeg` installed, running as
an unprivileged user on port 8080.

```sh
docker build -t webcoder .
docker run --rm -p 8080:8080 webcoder
# open http://localhost:8080

# with Basic auth:
docker run --rm -p 8080:8080 -e WEBCODER_AUTH=me:secret webcoder
```

## Releases

Pushing a `v*` tag triggers [`.github/workflows/release.yml`](.github/workflows/release.yml),
which builds and pushes a container image to GHCR (`ghcr.io/<owner>/<repo>`),
tagged with the semver version and `latest`.

```sh
git tag v0.1.0
git push origin v0.1.0
```

## License

See the upstream [Nmkoder](https://github.com/n00mkrad/nmkoder) project for the
original work this is based on.
