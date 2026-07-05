//! Bringing media into the app: probing native file paths (from the desktop
//! picker or an OS drag-drop) with the backend FFmpeg.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::core::{AppState, MediaFile};

use super::bridge::{js_error_text, parse_json, probe_native_path};
use super::state::{AppAction, AppCtx};
use super::types::ProbeResponse;

/// Shared handle for the file-id → backend-job-id map. Written when a probe
/// lands (Media tab), read when running the batch (Queue tab).
pub(crate) type JobIds = Rc<RefCell<HashMap<usize, String>>>;

/// Probe native file paths (desktop picker or OS drag-drop) with the backend
/// FFmpeg. Shared by the "Browse files" button and the native drag-and-drop
/// listener.
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
        job_log.set("Probing files with FFmpeg...".into());
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
                    Err(error) => job_log.set(format!("Probe parse failed: {error}")),
                },
                Err(error) => {
                    job_log.set(format!(
                        "Probe failed for {fallback_name}: {}",
                        js_error_text(error)
                    ));
                }
            }
        }
    });
}
