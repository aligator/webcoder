#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Attachment,
}

impl StreamKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Video => "VID",
            Self::Audio => "AUD",
            Self::Subtitle => "SUB",
            Self::Attachment => "ATT",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Subtitle => "Subtitle",
            Self::Attachment => "Attachment",
        }
    }

    fn ffmpeg_prefix(self) -> &'static str {
        match self {
            Self::Video => "v",
            Self::Audio => "a",
            Self::Subtitle => "s",
            Self::Attachment => "t",
        }
    }
}

/// A track's output disposition. `Encoder` holds a raw FFmpeg encoder name
/// (e.g. `libx264`) taken directly from the bundled WASM core's `-encoders`
/// list — nothing is hardcoded, so the dropdown always reflects what the
/// browser build can actually run.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TrackOutput {
    Copy,
    Strip,
    Encoder(String),
}

impl TrackOutput {
    pub fn label(&self) -> &str {
        match self {
            Self::Copy => "Copy",
            Self::Strip => "Strip",
            Self::Encoder(name) => name,
        }
    }

    pub fn from_label(value: &str) -> Self {
        match value {
            "Copy" => Self::Copy,
            "Strip" => Self::Strip,
            other => Self::Encoder(other.to_owned()),
        }
    }

    pub fn ffmpeg_codec(&self) -> Option<&str> {
        match self {
            Self::Copy => Some("copy"),
            Self::Strip => None,
            Self::Encoder(name) => Some(name),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QualityMode {
    ConstantQuality,
    Bitrate,
    FileSize,
}

impl QualityMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ConstantQuality => "Constant quality",
            Self::Bitrate => "Target bitrate",
            Self::FileSize => "Target file size",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaFile {
    pub id: usize,
    pub name: String,
    pub size_bytes: u64,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: usize,
    pub source_index: usize,
    pub enabled: bool,
    pub kind: StreamKind,
    pub codec: String,
    pub language: String,
    pub title: String,
    pub choice: TrackOutput,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConvertSettings {
    pub output_name: String,
    pub container: String,
    pub preset: String,
    pub quality_mode: QualityMode,
    pub quality_value: u32,
    pub bitrate_kbps: u32,
    pub target_size_mb: u32,
    pub color_format: String,
    pub fps: String,
    pub resize: String,
    pub crop_mode: String,
    pub crop: String,
    pub trim_start: String,
    pub trim_end: String,
    pub trim_duration: String,
    pub audio_channels: String,
    pub audio_bitrate_kbps: u32,
    pub burn_subtitles: bool,
    pub metadata_mode: String,
    pub chapter_mode: String,
    pub apply_track_metadata: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub files: Vec<MediaFile>,
    pub selected_file: Option<usize>,
    pub convert: ConvertSettings,
}

pub const METADATA_COPY: &str = "Copy Input Metadata, Apply Track Titles/Languages";
pub const METADATA_STRIP_KEEP_TRACKS: &str = "Apply Track Titles/Languages, Strip Other Metadata";
pub const METADATA_STRIP_ALL: &str = "Strip All Metadata";
pub const CHAPTERS_COPY: &str = "Copy Chapters From Input";
pub const CHAPTERS_STRIP: &str = "Strip Chapters";

impl Default for ConvertSettings {
    fn default() -> Self {
        Self {
            output_name: "output".into(),
            container: "mkv".into(),
            preset: "medium".into(),
            quality_mode: QualityMode::ConstantQuality,
            quality_value: 24,
            bitrate_kbps: 4500,
            target_size_mb: 1200,
            color_format: "yuv420p".into(),
            fps: String::new(),
            resize: String::new(),
            crop_mode: "Disable".into(),
            crop: String::new(),
            trim_start: String::new(),
            trim_end: String::new(),
            trim_duration: String::new(),
            audio_channels: "source".into(),
            audio_bitrate_kbps: 160,
            burn_subtitles: false,
            metadata_mode: METADATA_COPY.into(),
            chapter_mode: CHAPTERS_COPY.into(),
            apply_track_metadata: true,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            selected_file: None,
            convert: ConvertSettings::default(),
        }
    }
}

impl AppState {
    pub fn selected_file(&self) -> Option<&MediaFile> {
        self.selected_file
            .and_then(|id| self.files.iter().find(|file| file.id == id))
    }
}

pub fn command_preview(state: &AppState) -> String {
    build_ffmpeg_command(state)
}

fn output_name_from_settings(settings: &ConvertSettings) -> String {
    format!(
        "{}.{}",
        settings.output_name.trim().trim_end_matches('.'),
        settings.container.trim().trim_start_matches('.')
    )
}

fn build_ffmpeg_command(state: &AppState) -> String {
    let Some(file) = state.selected_file() else {
        return "Drop or select an input file to generate a command.".into();
    };

    let mut args = vec!["ffmpeg".to_owned()];
    args.extend(build_ffmpeg_args(state, file, true));
    args.join(" ")
}

fn build_ffmpeg_args(state: &AppState, file: &MediaFile, quoted: bool) -> Vec<String> {
    let output = output_name_from_settings(&state.convert);
    build_args(&state.convert, &file.tracks, &file.name, &output, quoted)
}

/// Build the FFmpeg argument vector (everything after the `ffmpeg` program name)
/// from structured settings only. `input`/`output` are supplied explicitly so
/// the server can pin them to sandboxed paths it controls rather than trusting
/// any client-provided path. There is deliberately no free-form custom-args
/// escape hatch: the whole command is derived from validated fields.
pub fn build_args(
    settings: &ConvertSettings,
    tracks: &[Track],
    input: &str,
    output: &str,
    quoted: bool,
) -> Vec<String> {
    let mut args = vec!["-hide_banner".to_owned(), "-y".to_owned()];

    args.extend(["-i".to_owned(), path_arg(input, quoted)]);

    let write_track_metadata =
        settings.apply_track_metadata && settings.metadata_mode != METADATA_STRIP_ALL;
    for track in tracks.iter().filter(|track| track.enabled) {
        if track.choice != TrackOutput::Strip {
            args.push("-map".into());
            args.push(format!("0:{}?", track.source_index));
        }
    }

    // Some bundled encoders are flagged experimental by the WASM core (e.g. the
    // native `opus`/`vorbis` encoders). FFmpeg refuses them unless `-strict`
    // is relaxed, so enable it when such an encoder is selected.
    let needs_experimental = tracks
        .iter()
        .filter(|track| track.enabled)
        .any(|track| matches!(track.choice.ffmpeg_codec(), Some("opus" | "vorbis")));
    if needs_experimental {
        args.push("-strict".into());
        args.push("experimental".into());
    }

    let mut output_index = 0usize;
    let mut per_type_index: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for track in tracks.iter().filter(|track| track.enabled) {
        if track.choice == TrackOutput::Strip {
            continue;
        }

        // Absolute output stream index for `-metadata:s:N`, and a per-kind index
        // for `-c:<type>:N` — the two use different numbering schemes.
        let stream_id = output_index;
        output_index += 1;
        let prefix = track.kind.ffmpeg_prefix();
        let type_index = per_type_index.entry(prefix).or_insert(0);
        if let Some(codec) = track.choice.ffmpeg_codec() {
            args.push(format!("-c:{prefix}:{type_index}"));
            args.push(codec.into());
        }
        *type_index += 1;

        if write_track_metadata && !track.language.trim().is_empty() {
            args.push(format!("-metadata:s:{stream_id}"));
            args.push(format!("language={}", track.language.trim()));
        }

        if write_track_metadata && !track.title.trim().is_empty() {
            args.push(format!("-metadata:s:{stream_id}"));
            args.push(if quoted {
                shell_quote(&format!("title={}", track.title.trim()))
            } else {
                format!("title={}", track.title.trim())
            });
        }
    }

    append_quality_args(&mut args, settings);
    append_video_filter_args(&mut args, settings, input, quoted);
    append_audio_args(&mut args, settings);

    append_metadata_args(&mut args, settings);
    args.push(path_arg(output, quoted));
    args
}

fn append_quality_args(args: &mut Vec<String>, settings: &ConvertSettings) {
    match settings.quality_mode {
        QualityMode::ConstantQuality => {
            args.push("-crf".into());
            args.push(settings.quality_value.to_string());
            args.push("-preset".into());
            args.push(settings.preset.clone());
        }
        QualityMode::Bitrate => {
            args.push("-b:v".into());
            args.push(format!("{}k", settings.bitrate_kbps));
        }
        QualityMode::FileSize => {
            args.push("-fs".into());
            args.push(format!("{}M", settings.target_size_mb));
        }
    }
}

fn append_video_filter_args(
    args: &mut Vec<String>,
    settings: &ConvertSettings,
    input: &str,
    quoted: bool,
) {
    let mut filters = Vec::new();

    if !settings.trim_start.trim().is_empty() {
        args.push("-ss".into());
        args.push(settings.trim_start.trim().into());
    }

    if !settings.trim_duration.trim().is_empty() {
        args.push("-t".into());
        args.push(settings.trim_duration.trim().into());
    } else if !settings.trim_end.trim().is_empty() {
        args.push("-to".into());
        args.push(settings.trim_end.trim().into());
    }

    if !settings.fps.trim().is_empty() {
        filters.push(format!("fps={}", settings.fps.trim()));
    }

    if !settings.resize.trim().is_empty() {
        filters.push(format!("scale={}", settings.resize.trim()));
    }

    if settings.crop_mode == "Manual" && !settings.crop.trim().is_empty() {
        filters.push(format!("crop={}", settings.crop.trim()));
    }

    if settings.burn_subtitles {
        filters.push(format!("subtitles={}", path_arg(input, quoted)));
    }

    if !filters.is_empty() {
        args.push("-vf".into());
        args.push(filters.join(","));
    }

    if settings.color_format != "source" {
        args.push("-pix_fmt".into());
        args.push(settings.color_format.clone());
    }
}

fn append_audio_args(args: &mut Vec<String>, settings: &ConvertSettings) {
    if settings.audio_channels != "source" {
        args.push("-ac".into());
        args.push(settings.audio_channels.clone());
    }

    if settings.audio_bitrate_kbps > 0 {
        args.push("-b:a".into());
        args.push(format!("{}k", settings.audio_bitrate_kbps));
    }
}

fn append_metadata_args(args: &mut Vec<String>, settings: &ConvertSettings) {
    if settings.metadata_mode == METADATA_STRIP_KEEP_TRACKS {
        args.push("-map_metadata".into());
        args.push("-1".into());
    } else if settings.metadata_mode == METADATA_STRIP_ALL {
        args.push("-map_metadata".into());
        args.push("-1".into());
    }

    if settings.chapter_mode == CHAPTERS_STRIP {
        args.push("-map_chapters".into());
        args.push("-1".into());
    }
}

fn path_arg(path: &str, quoted: bool) -> String {
    if quoted {
        shell_quote(path)
    } else {
        path.to_owned()
    }
}

/// Output containers the server is willing to mux into. Anything else is
/// rejected before an FFmpeg process is ever spawned.
pub const ALLOWED_CONTAINERS: &[&str] = &["mkv", "mp4", "mov", "webm", "gif"];
const ALLOWED_PRESETS: &[&str] = &["ultrafast", "veryfast", "fast", "medium", "slow", "slower"];
const ALLOWED_PIXEL_FORMATS: &[&str] = &["source", "yuv420p", "yuv420p10le", "yuv444p", "rgb24"];
const ALLOWED_CROP_MODES: &[&str] = &["Disable", "Manual"];
const ALLOWED_AUDIO_CHANNELS: &[&str] = &["source", "1", "2", "6", "8"];
const ALLOWED_METADATA_MODES: &[&str] = &[
    METADATA_COPY,
    METADATA_STRIP_KEEP_TRACKS,
    METADATA_STRIP_ALL,
];
const ALLOWED_CHAPTER_MODES: &[&str] = &[CHAPTERS_COPY, CHAPTERS_STRIP];

/// Reduce an arbitrary user string to a safe file stem: ASCII alphanumerics,
/// dash, underscore and dot only, never empty, length-capped. Used so the
/// client can never influence the on-disk path (traversal, absolute paths,
/// option-injection via a leading dash, etc.).
pub fn safe_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let trimmed = cleaned.trim_matches(['.', '-', '_', ' ']).to_owned();
    if trimmed.is_empty() {
        "output".to_owned()
    } else {
        trimmed
    }
}

fn token_ok(value: &str, extra: &str) -> bool {
    !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// Validate a decode/encode job built from client-supplied settings against the
/// set of encoder names the local FFmpeg actually offers. Returns a
/// human-readable reason on the first problem. This is the security gate: every
/// value that ends up on the FFmpeg command line is checked here, and there is
/// no path for free-form arguments to reach the process.
pub fn validate_job(
    settings: &ConvertSettings,
    tracks: &[Track],
    stream_count: usize,
    encoders: &std::collections::HashSet<String>,
) -> Result<(), String> {
    if !ALLOWED_CONTAINERS.contains(&settings.container.as_str()) {
        return Err(format!("Unsupported container: {}", settings.container));
    }
    if settings.output_name.trim().is_empty() {
        return Err("Output name is empty.".into());
    }
    if !ALLOWED_PRESETS.contains(&settings.preset.as_str()) {
        return Err("Unsupported preset value.".into());
    }
    if !ALLOWED_PIXEL_FORMATS.contains(&settings.color_format.as_str()) {
        return Err("Unsupported pixel format value.".into());
    }
    if !ALLOWED_CROP_MODES.contains(&settings.crop_mode.as_str()) {
        return Err("Unsupported crop mode.".into());
    }
    if !ALLOWED_METADATA_MODES.contains(&settings.metadata_mode.as_str()) {
        return Err("Unsupported metadata mode.".into());
    }
    if !ALLOWED_CHAPTER_MODES.contains(&settings.chapter_mode.as_str()) {
        return Err("Unsupported chapter mode.".into());
    }
    if !(1..=63).contains(&settings.quality_value) {
        return Err("CRF / CQ value must be between 1 and 63.".into());
    }
    if !(64..=250_000).contains(&settings.bitrate_kbps) {
        return Err("Video bitrate must be between 64 and 250000 kbps.".into());
    }
    if !(1..=500_000).contains(&settings.target_size_mb) {
        return Err("Target size must be between 1 and 500000 MB.".into());
    }
    if settings.audio_bitrate_kbps > 6_400 {
        return Err("Audio bitrate must be between 0 and 6400 kbps.".into());
    }
    if !settings.fps.trim().is_empty() && !token_ok(settings.fps.trim(), "./") {
        return Err("Invalid fps value.".into());
    }
    for (label, value) in [("resize", &settings.resize), ("crop", &settings.crop)] {
        let value = value.trim();
        if !value.is_empty() && !token_ok(value, "x:.,-") {
            return Err(format!("Invalid {label} value."));
        }
    }
    for (label, value) in [
        ("trim start", &settings.trim_start),
        ("trim end", &settings.trim_end),
        ("trim duration", &settings.trim_duration),
    ] {
        let value = value.trim();
        if !value.is_empty() && !token_ok(value, ":.") {
            return Err(format!("Invalid {label} value."));
        }
    }
    if !ALLOWED_AUDIO_CHANNELS.contains(&settings.audio_channels.as_str()) {
        return Err("Unsupported audio channel value.".into());
    }

    let mut enabled = 0usize;
    for track in tracks.iter().filter(|track| track.enabled) {
        enabled += 1;
        if track.source_index >= stream_count {
            return Err(format!(
                "Track references stream {} but the input has {stream_count} streams.",
                track.source_index
            ));
        }
        if let TrackOutput::Encoder(name) = &track.choice {
            if !token_ok(name, "-_.") {
                return Err(format!("Invalid encoder name: {name}"));
            }
            if !encoders.contains(name) {
                return Err(format!("Encoder not available on server: {name}"));
            }
        }
        if track.language.chars().any(|c| c.is_control()) {
            return Err("Track language contains invalid characters.".into());
        }
        if track.title.chars().any(|c| c.is_control()) {
            return Err("Track title contains invalid characters.".into());
        }
    }
    if enabled == 0 {
        return Err("No enabled tracks to encode.".into());
    }
    Ok(())
}

pub fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb >= 1.0 {
        format!("{gb:.2} GB")
    } else {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    }
}

pub fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/' | ':'))
    {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media_state() -> AppState {
        AppState {
            files: vec![MediaFile {
                id: 1,
                name: "source.mkv".into(),
                size_bytes: 1_879_048_192,
                tracks: vec![
                    Track {
                        id: 1,
                        source_index: 0,
                        enabled: true,
                        kind: StreamKind::Video,
                        codec: "HEVC 10-bit".into(),
                        language: "und".into(),
                        title: "Main video".into(),
                        choice: TrackOutput::Encoder("libsvtav1".into()),
                    },
                    Track {
                        id: 2,
                        source_index: 1,
                        enabled: true,
                        kind: StreamKind::Audio,
                        codec: "DTS-HD MA 5.1".into(),
                        language: "eng".into(),
                        title: "Surround".into(),
                        choice: TrackOutput::Encoder("libopus".into()),
                    },
                    Track {
                        id: 3,
                        source_index: 2,
                        enabled: true,
                        kind: StreamKind::Subtitle,
                        codec: "PGS".into(),
                        language: "eng".into(),
                        title: "Signs and songs".into(),
                        choice: TrackOutput::Encoder("srt".into()),
                    },
                ],
            }],
            selected_file: Some(1),
            ..AppState::default()
        }
    }

    #[test]
    fn ffmpeg_command_maps_enabled_tracks() {
        let state = media_state();
        let command = command_preview(&state);

        assert!(command.contains("ffmpeg"));
        assert!(command.contains("-map 0:0?"));
        assert!(command.contains("-map 0:1?"));
        assert!(command.contains("-c:v:0 libsvtav1"));
        assert!(command.contains("-c:a:0 libopus"));
    }

    #[test]
    fn shell_quote_escapes_spaces() {
        assert_eq!(shell_quote("clip one.mkv"), "\"clip one.mkv\"");
        assert_eq!(shell_quote("clip.mkv"), "clip.mkv");
    }

    #[test]
    fn subtitle_burn_shares_video_filter_chain() {
        let mut state = media_state();
        state.convert.fps = "24".into();
        state.convert.resize = "1280:-2".into();
        state.convert.crop_mode = "Manual".into();
        state.convert.crop = "1280:720:0:0".into();
        state.convert.burn_subtitles = true;

        let args = build_ffmpeg_args(&state, state.selected_file().unwrap(), false);
        let filter_count = args.iter().filter(|arg| *arg == "-vf").count();
        assert_eq!(filter_count, 1);
        let filter = args
            .windows(2)
            .find_map(|pair| (pair[0] == "-vf").then_some(pair[1].as_str()))
            .unwrap();
        assert!(filter.contains("fps=24"));
        assert!(filter.contains("scale=1280:-2"));
        assert!(filter.contains("crop=1280:720:0:0"));
        assert!(filter.contains("subtitles=source.mkv"));
    }

    #[test]
    fn strip_all_metadata_keeps_chapter_choice_separate() {
        let mut state = media_state();
        state.convert.metadata_mode = METADATA_STRIP_ALL.into();
        state.convert.chapter_mode = CHAPTERS_COPY.into();

        let args = build_ffmpeg_args(&state, state.selected_file().unwrap(), false);
        assert!(args.windows(2).any(|pair| pair == ["-map_metadata", "-1"]));
        assert!(!args.windows(2).any(|pair| pair == ["-map_chapters", "-1"]));
        assert!(!args.iter().any(|arg| arg.starts_with("language=")));
        assert!(!args.iter().any(|arg| arg.starts_with("title=")));
    }

    #[test]
    fn validation_rejects_unknown_convert_options() {
        let mut state = media_state();
        state.convert.crop_mode = "Automatic".into();
        let encoders = ["libsvtav1", "libopus", "srt"]
            .into_iter()
            .map(str::to_owned)
            .collect();

        let error = validate_job(
            &state.convert,
            &state.selected_file().unwrap().tracks,
            3,
            &encoders,
        )
        .unwrap_err();
        assert_eq!(error, "Unsupported crop mode.");
    }
}
