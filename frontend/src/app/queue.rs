//! The Queue tab: the batch-run board, the server runtime panel, and the
//! per-input result cards (download / save / zip-all wiring).
//!
//! The batch results are this tab's own concern, so they live in local state;
//! the tab reaches into the shared store only for the file list and settings,
//! and reports save-all outcomes back up through the `on_toast` callback.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::bridge::{
    js_error_text, native_app, parse_json, run_encode, save_all_outputs, save_output, with_api_key,
};
use super::ingest::JobIds;
use super::state::AppCtx;
use super::types::{EncodeItem, EncodeResponse, EncodeStatus, SaveAllResult, SaveItem};
use super::widgets::icon;

#[derive(Properties, PartialEq)]
pub(crate) struct QueueTabProps {
    pub(crate) job_ids: JobIds,
    pub(crate) on_toast: Callback<String>,
}

#[function_component(QueueTab)]
pub(crate) fn queue_tab(props: &QueueTabProps) -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    // Per-input encode results for the batch run — local to this tab.
    let results = use_state(Vec::<EncodeItem>::new);

    let is_native = native_app();
    let job_ids = &props.job_ids;
    let ready = state
        .files
        .iter()
        .filter(|file| job_ids.borrow().contains_key(&file.id))
        .count();

    let save_all = {
        let results = results.clone();
        let on_toast = props.on_toast.clone();
        Callback::from(move |_| {
            let items: Vec<SaveItem> = results
                .iter()
                .filter(|item| item.status == EncodeStatus::Done && !item.output_path.is_empty())
                .map(|item| SaveItem {
                    output_path: item.output_path.clone(),
                    output_name: item.output_name.clone(),
                })
                .collect();
            if items.is_empty() {
                return;
            }
            let Ok(payload) = serde_json::to_string(&items) else {
                return;
            };
            let on_toast = on_toast.clone();
            spawn_local(async move {
                match save_all_outputs(payload).await {
                    Ok(value) => match parse_json::<SaveAllResult>(value) {
                        Ok(result) if result.saved > 0 => {
                            on_toast.emit(format!("Copied {} files to the chosen folder.", result.saved))
                        }
                        Ok(_) => {}
                        Err(error) => on_toast.emit(format!("Save all failed: {error}")),
                    },
                    Err(error) => {
                        on_toast.emit(format!("Save all failed: {}", js_error_text(error)))
                    }
                }
            });
        })
    };

    let run_batch = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let results = results.clone();
        Callback::from(move |_| {
            // Snapshot every ready input up front (job id, per-file tracks,
            // per-file output name). The whole batch runs in one task so the
            // authoritative result list lives in `items` here — we only ever
            // write it to the state handle, never read the (stale) handle back.
            let settings = state.convert.clone();
            let mut jobs: Vec<(String, String, String, String)> = Vec::new();
            for file in state.files.iter() {
                let Some(job_id) = job_ids.borrow().get(&file.id).cloned() else {
                    continue;
                };
                let mut per_file = settings.clone();
                // Derive the output base from the input name minus its extension,
                // so "clip.mkv" → "clip" (not "clip.mkv.mkv" once the container
                // extension is appended server-side).
                let stem = file
                    .name
                    .rsplit_once('.')
                    .map(|(base, _)| base)
                    .unwrap_or(&file.name);
                per_file.output_name = crate::core::safe_stem(stem);
                let (Ok(settings_json), Ok(tracks_json)) = (
                    serde_json::to_string(&per_file),
                    serde_json::to_string(&file.tracks),
                ) else {
                    continue;
                };
                jobs.push((file.name.clone(), job_id, settings_json, tracks_json));
            }

            if jobs.is_empty() {
                return;
            }

            let results = results.clone();
            spawn_local(async move {
                let mut items: Vec<EncodeItem> = jobs
                    .iter()
                    .map(|(name, job_id, ..)| EncodeItem {
                        name: name.clone(),
                        job_id: job_id.clone(),
                        status: EncodeStatus::Running,
                        log: String::new(),
                        download_url: String::new(),
                        output_path: String::new(),
                        output_name: String::new(),
                    })
                    .collect();
                results.set(items.clone());

                for (index, (_, job_id, settings_json, tracks_json)) in jobs.into_iter().enumerate()
                {
                    match run_encode(job_id, settings_json, tracks_json).await {
                        Ok(value) => match parse_json::<EncodeResponse>(value) {
                            Ok(response) => {
                                items[index].status = if response.ok {
                                    EncodeStatus::Done
                                } else {
                                    EncodeStatus::Failed
                                };
                                items[index].log = response.log;
                                items[index].output_name = response.output_name;
                                items[index].download_url = response.download_url;
                                items[index].output_path = response.output_path.unwrap_or_default();
                            }
                            Err(error) => {
                                items[index].status = EncodeStatus::Failed;
                                items[index].log = format!("Encode parse failed: {error}");
                            }
                        },
                        Err(error) => {
                            items[index].status = EncodeStatus::Failed;
                            items[index].log =
                                format!("Server encode failed: {}", js_error_text(error));
                        }
                    }
                    results.set(items.clone());
                }
            });
        })
    };

    html! {
        <div class="stack queue-stack">
            <section class="queue-board">
                <div class="queue-metric">
                    <span>{state.files.len()}</span>
                    <small>{"inputs"}</small>
                </div>
                <div class="queue-metric">
                    <span>{ready}</span>
                    <small>{"ready"}</small>
                </div>
                <div class="queue-metric">
                    <span>{"FFmpeg"}</span>
                    <small>{"backend"}</small>
                </div>
                <div class="queue-metric">
                    <span>{state.convert.container.clone()}</span>
                    <small>{"container"}</small>
                </div>
            </section>
            <section class="settings-group wide runtime-panel">
                <div class="panel-title">
                    <div class="panel-title-label">{ icon("play_circle") }<h2>{"Server Runtime"}</h2></div>
                    {{
                        let done_ids: Vec<String> = results
                            .iter()
                            .filter(|r| r.status == EncodeStatus::Done)
                            .map(|r| r.job_id.clone())
                            .collect();
                        if results.is_empty() {
                            Html::default()
                        } else {
                            let zip_link = if done_ids.len() > 1 && !is_native {
                                let href = with_api_key(&format!("/api/zip?jobs={}", done_ids.join(",")));
                                html! {
                                    <a class="command-button accent result-download" href={href} download="webcoder-batch.zip">
                                        { icon("folder_zip") }
                                        { "Download all" }
                                    </a>
                                }
                            } else if done_ids.len() > 1 && is_native {
                                html! {
                                    <button class="command-button accent result-download" type="button" onclick={save_all.clone()}>
                                        { icon("folder") }
                                        { "Save all" }
                                    </button>
                                }
                            } else {
                                Html::default()
                            };
                            html! {
                                <div class="runtime-actions">
                                    <span class="result-status">{ format!("{}/{} done", done_ids.len(), results.len()) }</span>
                                    { zip_link }
                                </div>
                            }
                        }
                    }}
                </div>
                <button class="command-button accent" onclick={run_batch} disabled={ready == 0}>
                    { format!("RUN ({ready})") }
                </button>
                <div class="results-list">
                    { if results.is_empty() {
                        html! { <div class="result-empty">
                            { if ready == 0 { "Add inputs on the Media page." } else { "Ready. Press RUN to batch-encode." } }
                        </div> }
                    } else {
                        html! { for results.iter().map(|item| html! { <ResultRow item={item.clone()} /> }) }
                    } }
                </div>
            </section>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ResultRowProps {
    item: EncodeItem,
}

#[function_component(ResultRow)]
fn result_row(props: &ResultRowProps) -> Html {
    let item = &props.item;
    let (badge, label, status_class) = match item.status {
        EncodeStatus::Running => ("hourglass_top", "Encoding", "is-running"),
        EncodeStatus::Done => ("check_circle", "Done", "is-done"),
        EncodeStatus::Failed => ("error", "Failed", "is-failed"),
    };
    let open = item.status == EncodeStatus::Failed;
    let save_native = {
        let output_path = item.output_path.clone();
        let output_name = item.output_name.clone();
        Callback::from(move |_| {
            let output_path = output_path.clone();
            let output_name = output_name.clone();
            spawn_local(async move {
                let _ = save_output(output_path, output_name).await;
            });
        })
    };
    html! {
        <div class={classes!("result-card", status_class)}>
            <div class="result-head">
                <span class="material-symbols-rounded result-badge">{badge}</span>
                <strong>{&item.name}</strong>
                <span class="result-status">{label}</span>
                {
                    if !item.output_path.is_empty() {
                        html! {
                            <button class="command-button accent result-download" type="button" onclick={save_native}>
                                { icon("download") }
                                { "Save" }
                            </button>
                        }
                    } else if !item.download_url.is_empty() {
                        html! {
                            <a class="command-button accent result-download" href={item.download_url.clone()} download={item.output_name.clone()}>
                                { icon("download") }
                                { "Download" }
                            </a>
                        }
                    } else {
                        Html::default()
                    }
                }
            </div>
            {
                if item.log.is_empty() {
                    Html::default()
                } else {
                    html! {
                        <details class="result-log" open={open}>
                            <summary>{"Log"}</summary>
                            <textarea class="command-box log" readonly=true value={item.log.clone()} />
                        </details>
                    }
                }
            }
        </div>
    }
}
