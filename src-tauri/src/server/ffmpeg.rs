//! Thin wrappers around the system `ffmpeg`/`ffprobe` binaries: encoder
//! detection, input probing, and the small text/output helpers around them.

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;

use webcoder_frontend::core::{StreamKind, Track, TrackOutput};

use super::EncoderInfo;

pub(crate) async fn probe_encoders(
    ffmpeg: &str,
) -> Result<Vec<EncoderInfo>, Box<dyn std::error::Error>> {
    let out = tokio::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse_encoders(&text))
}

fn parse_encoders(text: &str) -> Vec<EncoderInfo> {
    // Lines look like: " V....D libx264   libx264 H.264 / AVC ..."
    let mut seen = HashSet::new();
    let mut list = Vec::new();
    for line in text.lines() {
        let bytes = line.as_bytes();
        // Need leading whitespace, 6 flag chars, space, then name.
        let trimmed = line.trim_start();
        if trimmed.len() < 8 {
            continue;
        }
        let flags = &trimmed[..6];
        let kind = match flags.as_bytes()[0] {
            b'V' => "Video",
            b'A' => "Audio",
            b'S' => "Subtitle",
            _ => continue,
        };
        if !flags[1..]
            .bytes()
            .all(|c| c == b'.' || c.is_ascii_alphabetic())
        {
            continue;
        }
        let rest = trimmed[6..].trim_start();
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else { continue };
        if name.is_empty() || !name.bytes().next().unwrap_or(b' ').is_ascii_alphanumeric() {
            continue;
        }
        let description = parts.next().unwrap_or("").trim().to_owned();
        let _ = bytes;
        if seen.insert(name.to_owned()) {
            list.push(EncoderInfo {
                name: name.to_owned(),
                kind: kind.to_owned(),
                description,
            });
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

pub(crate) async fn probe_input(
    ffprobe: &str,
    input: &PathBuf,
) -> Result<(Vec<Track>, usize, Option<Value>), Box<dyn std::error::Error>> {
    let out = tokio::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(input)
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    let json: Value = serde_json::from_slice(&out.stdout)?;
    let streams = json.get("streams").and_then(|v| v.as_array()).cloned();
    let format = json.get("format").cloned();

    let mut tracks = Vec::new();
    if let Some(streams) = &streams {
        for (fallback, stream) in streams.iter().enumerate() {
            let index = stream
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(fallback);
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let kind = match codec_type {
                "video" => StreamKind::Video,
                "audio" => StreamKind::Audio,
                "subtitle" => StreamKind::Subtitle,
                _ => StreamKind::Attachment,
            };
            let codec = stream
                .get("codec_long_name")
                .and_then(|v| v.as_str())
                .or_else(|| stream.get("codec_name").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_owned();
            let tags = stream.get("tags");
            let language = tags
                .and_then(|t| t.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("und")
                .to_owned();
            let title = tags
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or(kind.label())
                .to_owned();
            tracks.push(Track {
                id: fallback + 1,
                source_index: index,
                enabled: true,
                kind,
                codec,
                language,
                title,
                choice: TrackOutput::Copy,
            });
        }
    }
    let stream_count = streams.map(|s| s.len()).unwrap_or(0);
    Ok((tracks, stream_count, format))
}

pub(crate) async fn find_output(dir: &PathBuf) -> Option<PathBuf> {
    // The encode writes its single result into the job's `out/` subdir under the
    // friendly output name; return that file.
    let mut rd = tokio::fs::read_dir(dir.join("out")).await.ok()?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if entry.path().is_file() {
            return Some(entry.path());
        }
    }
    None
}

pub(crate) fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let start = text.len() - max;
    format!("…\n{}", &text[start..])
}
