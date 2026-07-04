//! Bringing media into the app: uploading + probing browser `File`s, probing
//! native file paths, and the heuristic track guess used before a probe lands.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen_futures::spawn_local;
use web_sys::{File, FileList};
use yew::prelude::*;

use crate::core::{AppState, MediaFile, StreamKind, Track, TrackOutput};

use super::bridge::{js_error_text, parse_json, probe_media, probe_native_path};
use super::state::{AppAction, AppCtx};
use super::types::ProbeResponse;

/// Shared handle for the file-id → server-job-id map. Written when a probe
/// lands (Media tab), read when running the batch (Queue tab).
pub(crate) type JobIds = Rc<RefCell<HashMap<usize, String>>>;

/// Upload browser `File`s to the server and probe them, updating the store
/// optimistically. Shared by the file `<input>` and the drag-and-drop zone.
pub(crate) fn ingest_web_files(
    files: FileList,
    state: AppCtx,
    job_ids: JobIds,
    job_log: UseStateHandle<String>,
) {
    // State captured now. Async probe callbacks rebuild the file list from this
    // stable `base` (+ the shared track accumulator) rather than cloning the
    // `state` handle, whose value is frozen at this render.
    let base = (*state).clone();
    let start = base.files.len();
    let mut pending: Vec<(usize, MediaFile, File)> = Vec::new();
    for index in 0..files.length() {
        if let Some(file) = files.get(index) {
            let id = start + pending.len() + 1;
            let media = file_to_media(id, &file);
            pending.push((id, media, file));
        }
    }
    if pending.is_empty() {
        return;
    }

    // Optimistic render so files appear immediately while probing.
    let mut next = base.clone();
    next.files
        .extend(pending.iter().map(|(_, media, _)| media.clone()));
    if next.selected_file.is_none() {
        next.selected_file = pending.first().map(|(id, _, _)| *id);
    }
    state.dispatch(AppAction::Replace(next));
    job_log.set("Uploading and probing media on the server...".into());

    let base = Rc::new(base);
    let pending_meta: Rc<Vec<(usize, MediaFile)>> =
        Rc::new(pending.iter().map(|(id, m, _)| (*id, m.clone())).collect());
    let acc: Rc<RefCell<HashMap<usize, Vec<Track>>>> = Rc::new(RefCell::new(HashMap::new()));

    for (id, media, file) in pending {
        let state = state.clone();
        let job_log = job_log.clone();
        let job_ids = job_ids.clone();
        let base = base.clone();
        let pending_meta = pending_meta.clone();
        let acc = acc.clone();
        let name = media.name.clone();
        spawn_local(async move {
            match probe_media(file).await {
                Ok(result) => match parse_json::<ProbeResponse>(result) {
                    Ok(probe) => {
                        job_ids.borrow_mut().insert(id, probe.job_id);
                        acc.borrow_mut().insert(id, probe.tracks);
                        let mut next = (*base).clone();
                        for (pid, pmeta) in pending_meta.iter() {
                            let mut media = pmeta.clone();
                            if let Some(tracks) = acc.borrow().get(pid) {
                                media.tracks = tracks.clone();
                            }
                            next.files.push(media);
                        }
                        if next.selected_file.is_none() {
                            next.selected_file = pending_meta.first().map(|(id, _)| *id);
                        }
                        state.dispatch(AppAction::Replace(next));
                        job_log.set(format!("Loaded stream metadata for {name}."));
                    }
                    Err(error) => job_log.set(format!("Probe parse failed for {name}: {error}")),
                },
                Err(error) => {
                    job_log.set(format!(
                        "Server probe failed for {name}: {}",
                        js_error_text(error)
                    ));
                }
            }
        });
    }
}

/// Probe native file paths (desktop picker or OS drag-drop) with the backend
/// FFmpeg. Shared by the "Open" button and the native drag-and-drop listener.
pub(crate) fn ingest_native_paths(
    paths: Vec<String>,
    base: AppState,
    state: AppCtx,
    job_ids: JobIds,
    job_log: UseStateHandle<String>,
) {
    if paths.is_empty() {
        return;
    }
    spawn_local(async move {
        job_log.set("Probing native files with FFmpeg...".into());
        let mut next = base;
        for path in paths {
            let fallback_name = path
                .rsplit(['/', '\\'])
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or("media")
                .to_owned();
            match probe_native_path(path.clone()).await {
                Ok(result) => match parse_json::<ProbeResponse>(result) {
                    Ok(probe) => {
                        let id = next.files.iter().map(|file| file.id).max().unwrap_or(0) + 1;
                        let name = probe.file_name.unwrap_or(fallback_name);
                        job_ids.borrow_mut().insert(id, probe.job_id);
                        next.files.push(MediaFile {
                            id,
                            name: name.clone(),
                            size_bytes: probe.size_bytes.unwrap_or(0),
                            tracks: probe.tracks,
                        });
                        if next.selected_file.is_none() {
                            next.selected_file = Some(id);
                        }
                        state.dispatch(AppAction::Replace(next.clone()));
                        job_log.set(format!("Loaded stream metadata for {name}."));
                    }
                    Err(error) => job_log.set(format!("Native probe parse failed: {error}")),
                },
                Err(error) => {
                    job_log.set(format!(
                        "Native probe failed for {fallback_name}: {}",
                        js_error_text(error)
                    ));
                }
            }
        }
    });
}

fn file_to_media(id: usize, file: &web_sys::File) -> MediaFile {
    let name = file.name();
    let lower = name.to_ascii_lowercase();
    let mut tracks = Vec::new();

    if is_audio_only(&lower) {
        tracks.push(Track {
            id: 1,
            source_index: 0,
            enabled: true,
            kind: StreamKind::Audio,
            codec: "Audio".into(),
            language: "und".into(),
            title: "Audio".into(),
            choice: TrackOutput::Copy,
        });
    } else {
        tracks.push(Track {
            id: 1,
            source_index: 0,
            enabled: true,
            kind: StreamKind::Video,
            codec: guessed_video_codec(&lower).into(),
            language: "und".into(),
            title: "Video".into(),
            choice: TrackOutput::Copy,
        });

        tracks.push(Track {
            id: 2,
            source_index: 1,
            enabled: true,
            kind: StreamKind::Audio,
            codec: "Audio".into(),
            language: "und".into(),
            title: "Main audio".into(),
            choice: TrackOutput::Copy,
        });
    }

    if lower.ends_with(".mkv") || lower.ends_with(".mp4") {
        tracks.push(Track {
            id: tracks.len() + 1,
            source_index: tracks.len(),
            enabled: false,
            kind: StreamKind::Subtitle,
            codec: "Subtitle".into(),
            language: "und".into(),
            title: "Subtitle".into(),
            choice: TrackOutput::Copy,
        });
    }

    MediaFile {
        id,
        name,
        size_bytes: file.size() as u64,
        tracks,
    }
}

fn is_audio_only(name: &str) -> bool {
    [".flac", ".wav", ".mp3", ".m4a", ".ogg", ".opus", ".aac"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

fn guessed_video_codec(name: &str) -> &'static str {
    if name.contains("hevc") || name.contains("h265") || name.contains("x265") {
        "HEVC"
    } else if name.contains("av1") {
        "AV1"
    } else if name.contains("vp9") {
        "VP9"
    } else {
        "Video"
    }
}
