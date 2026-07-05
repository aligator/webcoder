//! The Queue tab: the batch-run board, the FFmpeg runtime panel, and the
//! per-input result cards (output folder / overwrite wiring).
//!
//! The batch results are this tab's own concern, so they live in local state;
//! the tab reaches into the shared store only for the file list and settings,
//! and reports outcomes back up through the `on_toast` callback.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::bridge::{
    js_error_text, listen_encode_progress, parse_json, pick_output_dir, run_encode,
};
use super::ingest::JobIds;
use super::state::AppCtx;
use super::types::{EncodeItem, EncodeResponse, EncodeStatus};
use super::widgets::icon;

/// Shared per-job encode progress (job id → fraction 0..1), updated by the
/// desktop `webcoder-encode-progress` event and read by the result rows.
type Progress = Rc<RefCell<HashMap<String, f64>>>;

// Desktop settings persisted in localStorage so the output folder and overwrite
// choice survive reloads/restarts — no re-picking on every run.
const LS_OUTPUT_DIR: &str = "webcoder_output_dir";
const LS_OVERWRITE: &str = "webcoder_overwrite";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|window| window.local_storage().ok().flatten())
}

fn ls_get(key: &str) -> Option<String> {
    local_storage().and_then(|store| store.get_item(key).ok().flatten())
}

fn ls_set(key: &str, value: &str) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(key, value);
    }
}

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
    // Desktop: overwrite existing output files. Default off. Persisted.
    let overwrite = use_state(|| ls_get(LS_OVERWRITE).as_deref() == Some("1"));
    // Desktop: chosen output folder, persisted across sessions.
    let output_dir = use_state(|| ls_get(LS_OUTPUT_DIR).unwrap_or_default());
    // Desktop: live per-file encode progress, updated from FFmpeg events.
    let progress: Progress = use_mut_ref(HashMap::new);
    let tick = use_state(|| 0u32);
    let tick_counter = use_mut_ref(|| 0u32);

    let job_ids = &props.job_ids;
    let ready = state
        .files
        .iter()
        .filter(|file| job_ids.borrow().contains_key(&file.id))
        .count();

    // Register the desktop encode-progress listener once. Writes into the shared
    // `progress` map and bumps `tick` to force a re-render of the result rows.
    {
        let progress = progress.clone();
        let tick = tick.clone();
        let tick_counter = tick_counter.clone();
        use_effect_with((), move |_| {
            let closure = Closure::wrap(Box::new(move |job_id: String, fraction: f64| {
                progress.borrow_mut().insert(job_id, fraction);
                let mut counter = tick_counter.borrow_mut();
                *counter = counter.wrapping_add(1);
                tick.set(*counter);
            }) as Box<dyn FnMut(String, f64)>);
            listen_encode_progress(closure.as_ref().unchecked_ref());
            move || drop(closure)
        });
    }
    // Depend on `tick` so progress events re-render the rows.
    let _ = *tick;

    // Desktop: choose and remember the output folder.
    let choose_folder = {
        let output_dir = output_dir.clone();
        let on_toast = props.on_toast.clone();
        Callback::from(move |_| {
            let output_dir = output_dir.clone();
            let on_toast = on_toast.clone();
            spawn_local(async move {
                match pick_output_dir().await {
                    Ok(value) => {
                        let dir = value.as_string().unwrap_or_default();
                        if !dir.is_empty() {
                            ls_set(LS_OUTPUT_DIR, &dir);
                            output_dir.set(dir);
                        }
                    }
                    Err(error) => on_toast.emit(format!(
                        "Could not open folder picker: {}",
                        js_error_text(error)
                    )),
                }
            });
        })
    };

    let run_batch = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let results = results.clone();
        let overwrite = overwrite.clone();
        let output_dir = output_dir.clone();
        let progress = progress.clone();
        let on_toast = props.on_toast.clone();
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

            // Encodes write into the remembered output folder; abort if none chosen.
            let output_dir = (*output_dir).clone();
            if output_dir.is_empty() {
                on_toast.emit("Choose an output folder first.".to_owned());
                return;
            }

            let results = results.clone();
            let progress = progress.clone();
            let overwrite = *overwrite;
            spawn_local(async move {
                progress.borrow_mut().clear();
                let mut items: Vec<EncodeItem> = jobs
                    .iter()
                    .map(|(name, job_id, ..)| EncodeItem {
                        name: name.clone(),
                        job_id: job_id.clone(),
                        status: EncodeStatus::Running,
                        log: String::new(),
                        output_path: String::new(),
                    })
                    .collect();
                results.set(items.clone());

                for (index, (_, job_id, settings_json, tracks_json)) in jobs.into_iter().enumerate()
                {
                    match run_encode(
                        job_id,
                        settings_json,
                        tracks_json,
                        output_dir.clone(),
                        overwrite,
                    )
                    .await
                    {
                        Ok(value) => match parse_json::<EncodeResponse>(value) {
                            Ok(response) => {
                                items[index].status = if response.ok {
                                    EncodeStatus::Done
                                } else {
                                    EncodeStatus::Failed
                                };
                                items[index].log = response.log;
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
                                format!("Encode failed: {}", js_error_text(error));
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
                    <div class="panel-title-label">{ icon("play_circle") }<h2>{"FFmpeg Runtime"}</h2></div>
                    {
                        if results.is_empty() {
                            Html::default()
                        } else {
                            let done = results.iter().filter(|r| r.status == EncodeStatus::Done).count();
                            html! {
                                <div class="runtime-actions">
                                    <span class="result-status">{ format!("{}/{} done", done, results.len()) }</span>
                                </div>
                            }
                        }
                    }
                </div>
                {{
                    let toggle = {
                        let overwrite = overwrite.clone();
                        Callback::from(move |_| {
                            let next = !*overwrite;
                            ls_set(LS_OVERWRITE, if next { "1" } else { "0" });
                            overwrite.set(next);
                        })
                    };
                    let dir_empty = output_dir.is_empty();
                    html! {
                        <>
                            <div class="output-folder">
                                <span class="field-label">{ "Output folder" }</span>
                                <span class={classes!("output-folder-path", dir_empty.then_some("is-empty"))}>
                                    { if dir_empty { "No folder selected".to_owned() } else { (*output_dir).clone() } }
                                </span>
                                <button class="command-button result-download" type="button" onclick={choose_folder}>
                                    { icon("folder") }
                                    { "Choose…" }
                                </button>
                            </div>
                            <label class="overwrite-toggle">
                                <input type="checkbox" checked={*overwrite} onchange={toggle} />
                                { "Overwrite existing files" }
                            </label>
                        </>
                    }
                }}
                <button class="command-button accent" onclick={run_batch} disabled={ready == 0 || output_dir.is_empty()}>
                    { format!("RUN ({ready})") }
                </button>
                <div class="results-list">
                    { if results.is_empty() {
                        html! { <div class="result-empty">
                            { if ready == 0 { "Add inputs on the Media page." } else { "Ready. Press RUN to batch-encode." } }
                        </div> }
                    } else {
                        html! { for results.iter().map(|item| {
                            let fraction = progress.borrow().get(&item.job_id).copied();
                            html! { <ResultRow item={item.clone()} fraction={fraction} /> }
                        }) }
                    } }
                </div>
            </section>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ResultRowProps {
    item: EncodeItem,
    /// Desktop live progress for this file (0..1), if any.
    fraction: Option<f64>,
}

#[function_component(ResultRow)]
fn result_row(props: &ResultRowProps) -> Html {
    let item = &props.item;
    let running = item.status == EncodeStatus::Running;
    let percent = props.fraction.map(|value| (value * 100.0).round() as u32);
    let (badge, label, status_class) = match item.status {
        EncodeStatus::Running => (
            "hourglass_top",
            match percent {
                Some(value) => format!("Encoding {value}%"),
                None => "Encoding".to_owned(),
            },
            "is-running",
        ),
        EncodeStatus::Done => ("check_circle", "Done".to_owned(), "is-done"),
        EncodeStatus::Failed => ("error", "Failed".to_owned(), "is-failed"),
    };
    let open = item.status == EncodeStatus::Failed;
    html! {
        <div class={classes!("result-card", status_class)}>
            <div class="result-head">
                <span class="material-symbols-rounded result-badge">{badge}</span>
                <strong>{&item.name}</strong>
                <span class="result-status">{label}</span>
                {
                    // Encodes write straight into the chosen output folder; show
                    // the saved path once a file has been produced.
                    if !item.output_path.is_empty() {
                        html! { <span class="result-path" title={item.output_path.clone()}>{&item.output_path}</span> }
                    } else {
                        Html::default()
                    }
                }
            </div>
            {
                if running {
                    let style = percent
                        .map(|value| format!("width:{value}%"))
                        .unwrap_or_else(|| "width:0%".to_owned());
                    let bar_class = if percent.is_some() {
                        classes!("progress-bar")
                    } else {
                        classes!("progress-bar", "indeterminate")
                    };
                    html! {
                        <div class="progress-track">
                            <div class={bar_class} style={style}></div>
                        </div>
                    }
                } else {
                    Html::default()
                }
            }
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
