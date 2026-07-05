# Webcoder

A desktop media transcoder powered by [FFmpeg](https://ffmpeg.org/).
Pick codecs, quality and filters in the UI; encoding runs natively so the full
codec set is available (AV1, HEVC, …).

> Inspired by [**Nmkoder** by n00mkrad](https://github.com/n00mkrad/nmkoder).

Webcoder is a [Tauri](https://tauri.app/) app: a Yew/WASM UI in a native
window, driving the system `ffmpeg`/`ffprobe` through Tauri commands. Files are
added via the native picker or OS drag-and-drop and encoded straight into the
output folder you choose.

## Run

```sh
# Development
cargo tauri-dev

# Release bundle (AppImage/deb/…)
cargo tauri-build

# Flatpak (bundles its own FFmpeg)
flatpak-builder --force-clean --user --install build-dir flatpak/dev.webcoder.app.yml
flatpak run dev.webcoder.app
```

Requires the Rust toolchain, [Trunk](https://trunkrs.dev/) and `ffmpeg` on
`PATH` (except Flatpak, which bundles FFmpeg).

`WEBCODER_FFMPEG` / `WEBCODER_FFPROBE` override the binary paths if the tools
are not on `PATH` (the Flatpak launcher sets these to the bundled build).

## License

Based on the upstream [Nmkoder](https://github.com/n00mkrad/nmkoder) project.
