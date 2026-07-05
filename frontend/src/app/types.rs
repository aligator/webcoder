//! Plain data types shared across the app views: bridge response shapes, the
//! batch-queue item state, and the navigation enums.

use serde::Deserialize;

use crate::core::{StreamKind, Track};

#[derive(Deserialize)]
pub(crate) struct ApiEncoder {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) description: String,
}

#[derive(Deserialize)]
pub(crate) struct ProbeResponse {
    pub(crate) job_id: String,
    #[allow(dead_code)]
    pub(crate) stream_count: usize,
    pub(crate) tracks: Vec<Track>,
    pub(crate) file_name: Option<String>,
    pub(crate) size_bytes: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct EncodeResponse {
    pub(crate) ok: bool,
    pub(crate) log: String,
    pub(crate) output_name: String,
    pub(crate) download_url: String,
    pub(crate) output_path: Option<String>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum EncodeStatus {
    Running,
    Done,
    Failed,
}

/// One row in the batch queue: the state of encoding a single input file.
#[derive(Clone, PartialEq)]
pub(crate) struct EncodeItem {
    pub(crate) name: String,
    pub(crate) job_id: String,
    pub(crate) status: EncodeStatus,
    pub(crate) log: String,
    pub(crate) download_url: String,
    pub(crate) output_path: String,
    pub(crate) output_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tab {
    Media,
    Convert,
    Queue,
}

impl Tab {
    pub(crate) const ALL: &'static [(Self, &'static str, &'static str)] = &[
        (Self::Media, "video_library", "Media"),
        (Self::Convert, "tune", "Convert"),
        (Self::Queue, "queue", "Queue"),
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Media => "Media",
            Self::Convert => "Convert",
            Self::Queue => "Queue",
        }
    }

    pub(crate) fn subtitle(self) -> &'static str {
        match self {
            Self::Media => "Add files and set per-stream copy, strip, or transcode.",
            Self::Convert => "Tune container, codecs, quality, resize, crop, and audio settings.",
            Self::Queue => "Batch-encode every input on the server with native FFmpeg.",
        }
    }
}

/// One encoder offered by the server's FFmpeg, from `ffmpeg -encoders`. Drives
/// the per-track output dropdown so it lists only codecs the backend can run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrowserEncoder {
    pub(crate) name: String,
    pub(crate) kind: StreamKind,
    pub(crate) description: String,
}

pub(crate) fn kind_from_str(value: &str) -> StreamKind {
    match value {
        "Video" => StreamKind::Video,
        "Audio" => StreamKind::Audio,
        "Subtitle" => StreamKind::Subtitle,
        _ => StreamKind::Attachment,
    }
}
