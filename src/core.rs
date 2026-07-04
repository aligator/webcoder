#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Mux,
    Batch,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mux => "Muxing",
            Self::Batch => "Batch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecChoice {
    Copy,
    Strip,
    H264X264,
    H264Nvenc,
    H265X265,
    H265Nvenc,
    Vp9,
    Av1Svt,
    Av1Aom,
    Aac,
    Opus,
    Vorbis,
    Eac3,
    Mp3,
    Flac,
    MovText,
    Srt,
    WebVtt,
    PngSequence,
    JpegSequence,
    Gif,
}

impl CodecChoice {
    pub const VIDEO: &'static [Self] = &[
        Self::Copy,
        Self::Strip,
        Self::H264X264,
        Self::H264Nvenc,
        Self::H265X265,
        Self::H265Nvenc,
        Self::Vp9,
        Self::Av1Svt,
        Self::Av1Aom,
        Self::PngSequence,
        Self::JpegSequence,
        Self::Gif,
    ];

    pub const AUDIO: &'static [Self] = &[
        Self::Copy,
        Self::Strip,
        Self::Aac,
        Self::Opus,
        Self::Vorbis,
        Self::Eac3,
        Self::Mp3,
        Self::Flac,
    ];

    pub const SUBTITLE: &'static [Self] = &[
        Self::Copy,
        Self::Strip,
        Self::MovText,
        Self::Srt,
        Self::WebVtt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Strip => "Strip",
            Self::H264X264 => "H.264 (x264)",
            Self::H264Nvenc => "H.264 (NVENC)",
            Self::H265X265 => "H.265 (x265)",
            Self::H265Nvenc => "H.265 (NVENC)",
            Self::Vp9 => "VP9",
            Self::Av1Svt => "AV1 (SVT-AV1)",
            Self::Av1Aom => "AV1 (AOM)",
            Self::Aac => "AAC",
            Self::Opus => "Opus",
            Self::Vorbis => "Vorbis",
            Self::Eac3 => "E-AC-3",
            Self::Mp3 => "MP3",
            Self::Flac => "FLAC",
            Self::MovText => "Mov_Text",
            Self::Srt => "SRT",
            Self::WebVtt => "WebVTT",
            Self::PngSequence => "PNG sequence",
            Self::JpegSequence => "JPEG sequence",
            Self::Gif => "Animated GIF",
        }
    }

    pub fn ffmpeg_codec(self) -> Option<&'static str> {
        match self {
            Self::Copy => Some("copy"),
            Self::Strip => None,
            Self::H264X264 => Some("libx264"),
            Self::H264Nvenc => Some("h264_nvenc"),
            Self::H265X265 => Some("libx265"),
            Self::H265Nvenc => Some("hevc_nvenc"),
            Self::Vp9 => Some("libvpx-vp9"),
            Self::Av1Svt => Some("libsvtav1"),
            Self::Av1Aom => Some("libaom-av1"),
            Self::Aac => Some("aac"),
            Self::Opus => Some("libopus"),
            Self::Vorbis => Some("libvorbis"),
            Self::Eac3 => Some("eac3"),
            Self::Mp3 => Some("libmp3lame"),
            Self::Flac => Some("flac"),
            Self::MovText => Some("mov_text"),
            Self::Srt => Some("srt"),
            Self::WebVtt => Some("webvtt"),
            Self::PngSequence => Some("png"),
            Self::JpegSequence => Some("mjpeg"),
            Self::Gif => Some("gif"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityMode {
    ConstantQuality,
    Bitrate,
    FileSize,
    Vmaf,
}

impl QualityMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ConstantQuality => "Constant quality",
            Self::Bitrate => "Target bitrate",
            Self::FileSize => "Target file size",
            Self::Vmaf => "Target VMAF",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Utility {
    ReadBitrates,
    Metrics,
    TransferColor,
    ConcatMkv,
    BitrateChart,
}

impl Utility {
    pub const ALL: &'static [Self] = &[
        Self::ReadBitrates,
        Self::Metrics,
        Self::TransferColor,
        Self::ConcatMkv,
        Self::BitrateChart,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ReadBitrates => "Read bitrates",
            Self::Metrics => "Get metrics",
            Self::TransferColor => "Transfer color metadata",
            Self::ConcatMkv => "Concatenate into MKV",
            Self::BitrateChart => "Show bitrate chart",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ReadBitrates => "Estimate per-stream size and average bitrate.",
            Self::Metrics => "Prepare VMAF, SSIM, and PSNR analysis commands.",
            Self::TransferColor => "Copy color tags and HDR metadata between files.",
            Self::ConcatMkv => "Build a concat list and merge compatible files.",
            Self::BitrateChart => "Export bitrate over time for graphing.",
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

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: usize,
    pub source_index: usize,
    pub enabled: bool,
    pub kind: StreamKind,
    pub codec: String,
    pub language: String,
    pub title: String,
    pub choice: CodecChoice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConvertSettings {
    pub mode: Mode,
    pub output_name: String,
    pub container: String,
    pub custom_args_in: String,
    pub custom_args_out: String,
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
pub struct Av1anSettings {
    pub enabled: bool,
    pub encoder: CodecChoice,
    pub workers: u32,
    pub threads: u32,
    pub splitter: String,
    pub chunk_method: String,
    pub concat_mode: String,
    pub chunk_order: String,
    pub resume: bool,
    pub film_grain: u32,
    pub grain_denoise: bool,
    pub target_vmaf: u32,
    pub custom_encoder_args: String,
    pub custom_av1an_args: String,
    pub copy_subtitles: bool,
    pub copy_attachments: bool,
    pub copy_data: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub files: Vec<MediaFile>,
    pub selected_file: Option<usize>,
    pub convert: ConvertSettings,
    pub av1an: Av1anSettings,
    pub utility: Utility,
}

impl Default for ConvertSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Mux,
            output_name: "output".into(),
            container: "mkv".into(),
            custom_args_in: String::new(),
            custom_args_out: String::new(),
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
            metadata_mode: "Copy All From Input, Edit Titles/Languages".into(),
            chapter_mode: "Copy All From Input, Edit Titles/Languages".into(),
            apply_track_metadata: true,
        }
    }
}

impl Default for Av1anSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            encoder: CodecChoice::Av1Svt,
            workers: 6,
            threads: 0,
            splitter: "scenedetect".into(),
            chunk_method: "segment".into(),
            concat_mode: "ffmpeg".into(),
            chunk_order: "long-to-short".into(),
            resume: true,
            film_grain: 0,
            grain_denoise: false,
            target_vmaf: 95,
            custom_encoder_args: String::new(),
            custom_av1an_args: String::new(),
            copy_subtitles: true,
            copy_attachments: true,
            copy_data: false,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            selected_file: None,
            convert: ConvertSettings::default(),
            av1an: Av1anSettings::default(),
            utility: Utility::ReadBitrates,
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
    if state.av1an.enabled {
        build_av1an_command(state)
    } else {
        build_ffmpeg_command(state)
    }
}

pub fn utility_command(state: &AppState) -> String {
    let input = state
        .selected_file()
        .map(|file| shell_quote(&file.name))
        .unwrap_or_else(|| "\"input.mkv\"".into());

    match state.utility {
        Utility::ReadBitrates => format!(
            "ffprobe -v error -show_entries stream=index,codec_type,codec_name,bit_rate:format=duration,size -of json {input}"
        ),
        Utility::Metrics => format!(
            "ffmpeg -i {input} -i \"reference.mkv\" -lavfi libvmaf=log_fmt=json:log_path=vmaf.json -f null -"
        ),
        Utility::TransferColor => format!(
            "ffmpeg -i {input} -i \"metadata-source.mkv\" -map 0 -c copy -map_metadata 1 -color_primaries bt2020 -colorspace bt2020nc -color_trc smpte2084 \"color-tagged.mkv\""
        ),
        Utility::ConcatMkv => {
            "printf \"file '%s'\\n\" *.mkv > concat.txt\nffmpeg -f concat -safe 0 -i concat.txt -c copy \"joined.mkv\"".into()
        }
        Utility::BitrateChart => format!(
            "ffprobe -v error -select_streams v:0 -show_entries packet=pts_time,size -of csv=p=0 {input} > bitrate-packets.csv"
        ),
    }
}

pub fn ffmpeg_args(state: &AppState) -> Vec<String> {
    if state.av1an.enabled {
        return Vec::new();
    }

    let Some(file) = state.selected_file() else {
        return Vec::new();
    };

    build_ffmpeg_args(state, file, false)
}

pub fn output_file_name(state: &AppState) -> String {
    output_name_from_settings(&state.convert)
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
    let mut args = vec!["-hide_banner".to_owned(), "-y".to_owned()];

    extend_shell_words(&mut args, &state.convert.custom_args_in);
    args.extend(["-i".to_owned(), path_arg(&file.name, quoted)]);

    for track in file.tracks.iter().filter(|track| track.enabled) {
        if track.choice != CodecChoice::Strip {
            args.push("-map".into());
            args.push(format!("0:{}?", track.source_index));
        }
    }

    for track in file.tracks.iter().filter(|track| track.enabled) {
        if track.choice == CodecChoice::Strip {
            continue;
        }

        let stream_id = track.id.saturating_sub(1);
        if let Some(codec) = track.choice.ffmpeg_codec() {
            args.push(format!("-c:{}:{stream_id}", track.kind.ffmpeg_prefix()));
            args.push(codec.into());
        }

        if !track.language.trim().is_empty() {
            args.push(format!("-metadata:s:{stream_id}"));
            args.push(format!("language={}", track.language.trim()));
        }

        if state.convert.apply_track_metadata && !track.title.trim().is_empty() {
            args.push(format!("-metadata:s:{stream_id}"));
            args.push(if quoted {
                shell_quote(&format!("title={}", track.title.trim()))
            } else {
                format!("title={}", track.title.trim())
            });
        }
    }

    append_quality_args(&mut args, &state.convert);
    append_video_filter_args(&mut args, &state.convert);
    append_audio_args(&mut args, &state.convert);

    if state.convert.burn_subtitles {
        args.push("-vf".into());
        args.push(format!("subtitles={}", path_arg(&file.name, quoted)));
    }

    append_metadata_args(&mut args, &state.convert);
    extend_shell_words(&mut args, &state.convert.custom_args_out);
    args.push(path_arg(&output_name_from_settings(&state.convert), quoted));
    args
}

fn build_av1an_command(state: &AppState) -> String {
    let Some(file) = state.selected_file() else {
        return "Drop or select an input file to generate a command.".into();
    };

    let mut args = vec![
        "av1an".to_owned(),
        "-i".to_owned(),
        shell_quote(&file.name),
        "-o".to_owned(),
        output_path(&state.convert),
        "--encoder".into(),
        match state.av1an.encoder {
            CodecChoice::Av1Aom => "aom",
            CodecChoice::Av1Svt => "svt-av1",
            CodecChoice::Vp9 => "vpx",
            CodecChoice::H265X265 => "x265",
            _ => "svt-av1",
        }
        .into(),
        "--workers".into(),
        state.av1an.workers.to_string(),
        "--split-method".into(),
        state.av1an.splitter.clone(),
        "--chunk-method".into(),
        state.av1an.chunk_method.clone(),
        "--concat".into(),
        state.av1an.concat_mode.clone(),
        "--chunk-order".into(),
        state.av1an.chunk_order.clone(),
    ];

    if state.av1an.resume {
        args.push("--resume".into());
    }

    if state.convert.quality_mode == QualityMode::Vmaf {
        args.push("--target-quality".into());
        args.push(state.av1an.target_vmaf.to_string());
    } else {
        args.push("-v".into());
        args.push(format!("--crf {}", state.convert.quality_value));
    }

    if state.av1an.encoder == CodecChoice::Av1Svt && state.av1an.film_grain > 0 {
        args.push("--photon-noise".into());
        args.push(state.av1an.film_grain.to_string());
    }

    if state.av1an.threads > 0 {
        args.push("--set-thread-affinity".into());
        args.push(state.av1an.threads.to_string());
    }

    if state.av1an.grain_denoise {
        args.push("--photon-noise-denoise".into());
    }

    if state.av1an.copy_subtitles {
        args.push("--keep".into());
        args.push("s".into());
    }

    if state.av1an.copy_attachments {
        args.push("--keep".into());
        args.push("t".into());
    }

    if state.av1an.copy_data {
        args.push("--keep".into());
        args.push("d".into());
    }

    extend_shell_words(&mut args, &state.av1an.custom_encoder_args);
    extend_shell_words(&mut args, &state.av1an.custom_av1an_args);
    args.join(" ")
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
        QualityMode::Vmaf => {
            args.push("-crf".into());
            args.push(settings.quality_value.to_string());
        }
    }
}

fn append_video_filter_args(args: &mut Vec<String>, settings: &ConvertSettings) {
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

    if settings.crop_mode == "Automatic" {
        filters.push("cropdetect".into());
    } else if settings.crop_mode == "Manual" && !settings.crop.trim().is_empty() {
        filters.push(format!("crop={}", settings.crop.trim()));
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
    if settings.metadata_mode == "Apply Titles/Languages, Strip Rest" {
        args.push("-map_metadata".into());
        args.push("-1".into());
    } else if settings.metadata_mode == "Strip All Metadata Including Titles/Languages" {
        args.push("-map_metadata".into());
        args.push("-1".into());
        args.push("-map_chapters".into());
        args.push("-1".into());
    }

    if settings.chapter_mode == "Strip All Metadata Including Titles/Languages" {
        args.push("-map_chapters".into());
        args.push("-1".into());
    }
}

fn extend_shell_words(args: &mut Vec<String>, value: &str) {
    args.extend(value.split_whitespace().map(ToOwned::to_owned));
}

fn output_path(settings: &ConvertSettings) -> String {
    shell_quote(&output_name_from_settings(settings))
}

fn path_arg(path: &str, quoted: bool) -> String {
    if quoted {
        shell_quote(path)
    } else {
        path.to_owned()
    }
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
                        choice: CodecChoice::Av1Svt,
                    },
                    Track {
                        id: 2,
                        source_index: 1,
                        enabled: true,
                        kind: StreamKind::Audio,
                        codec: "DTS-HD MA 5.1".into(),
                        language: "eng".into(),
                        title: "Surround".into(),
                        choice: CodecChoice::Opus,
                    },
                    Track {
                        id: 3,
                        source_index: 2,
                        enabled: true,
                        kind: StreamKind::Subtitle,
                        codec: "PGS".into(),
                        language: "eng".into(),
                        title: "Signs and songs".into(),
                        choice: CodecChoice::Srt,
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
        assert!(command.contains("-c:a:1 libopus"));
    }

    #[test]
    fn utility_commands_use_selected_input() {
        let mut state = media_state();
        state.utility = Utility::BitrateChart;

        assert!(utility_command(&state).contains("source.mkv"));
        assert!(utility_command(&state).contains("bitrate-packets.csv"));
    }

    #[test]
    fn shell_quote_escapes_spaces() {
        assert_eq!(shell_quote("clip one.mkv"), "\"clip one.mkv\"");
        assert_eq!(shell_quote("clip.mkv"), "clip.mkv");
    }
}
