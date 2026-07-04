use crate::core::{
    ALLOWED_CONTAINERS, AppState, CHAPTERS_COPY, CHAPTERS_STRIP, ConvertSettings, METADATA_COPY,
    METADATA_STRIP_ALL, METADATA_STRIP_KEEP_TRACKS, MediaFile, QualityMode, StreamKind, Track,
    TrackOutput, command_preview, format_size,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, File, HtmlInputElement, HtmlSelectElement, InputEvent};
use yew::TargetCast;
use yew::prelude::*;

#[wasm_bindgen(module = "/assets/api.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = getEncoders)]
    async fn get_encoders() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = probeMedia)]
    async fn probe_media(file: File) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = runEncode)]
    async fn run_encode(
        job_id: String,
        settings_json: String,
        tracks_json: String,
    ) -> Result<JsValue, JsValue>;
}

#[derive(Deserialize)]
struct ApiEncoder {
    name: String,
    kind: String,
    description: String,
}

#[derive(Deserialize)]
struct ProbeResponse {
    job_id: String,
    #[allow(dead_code)]
    stream_count: usize,
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
struct EncodeResponse {
    ok: bool,
    log: String,
    output_name: String,
    download_url: String,
}

#[derive(Clone, PartialEq)]
enum EncodeStatus {
    Running,
    Done,
    Failed,
}

/// One row in the batch queue: the state of encoding a single input file.
#[derive(Clone, PartialEq)]
struct EncodeItem {
    name: String,
    job_id: String,
    status: EncodeStatus,
    log: String,
    download_url: String,
    output_name: String,
}

/// Decode a JSON string returned by the api.js bridge into `T`.
fn parse_json<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, String> {
    let text = value
        .as_string()
        .ok_or("Expected a JSON string from bridge.")?;
    serde_json::from_str(&text).map_err(|e| format!("Bad response: {e}"))
}

fn js_error_text(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| "unknown error".into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tab {
    Media,
    Convert,
    Queue,
}

impl Tab {
    const ALL: &'static [(Self, &'static str, &'static str)] = &[
        (Self::Media, "video_library", "Media"),
        (Self::Convert, "tune", "Convert"),
        (Self::Queue, "queue", "Queue"),
    ];
}

/// One encoder offered by the server's FFmpeg, from `ffmpeg -encoders`. Drives
/// the per-track output dropdown so it lists only codecs the backend can run.
#[derive(Clone, Debug, PartialEq)]
struct BrowserEncoder {
    name: String,
    kind: StreamKind,
    description: String,
}

fn kind_from_str(value: &str) -> StreamKind {
    match value {
        "Video" => StreamKind::Video,
        "Audio" => StreamKind::Audio,
        "Subtitle" => StreamKind::Subtitle,
        _ => StreamKind::Attachment,
    }
}

#[function_component(App)]
pub fn app() -> Html {
    let state = use_state(AppState::default);
    let tab = use_state(|| Tab::Media);
    let copied = use_state(|| false);
    let show_command_preview = use_state(|| false);
    let job_log = use_state(|| "Server FFmpeg runtime idle.".to_owned());
    let browser_encoders = use_state(Vec::<BrowserEncoder>::new);
    // Per-input encode results for the batch run.
    let results = use_state(Vec::<EncodeItem>::new);
    // Maps a local file id to the server-side job id returned by the probe
    // upload, so the encode request can reference the already-uploaded input.
    let job_ids = use_mut_ref(HashMap::<usize, String>::new);

    // Fetch the encoders the server's FFmpeg supports once on load so every
    // output dropdown lists only codecs the backend can actually run.
    {
        let browser_encoders = browser_encoders.clone();
        let job_log = job_log.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match get_encoders().await {
                    Ok(value) => match parse_json::<Vec<ApiEncoder>>(value) {
                        Ok(list) => browser_encoders.set(
                            list.into_iter()
                                .map(|e| BrowserEncoder {
                                    kind: kind_from_str(&e.kind),
                                    name: e.name,
                                    description: e.description,
                                })
                                .collect(),
                        ),
                        Err(error) => job_log.set(format!("Encoder list error: {error}")),
                    },
                    Err(error) => {
                        job_log.set(format!("Encoder list failed: {}", js_error_text(error)))
                    }
                }
            });
            || ()
        });
    }

    let active_command = command_preview(&state);

    let on_copy = {
        let active_command = active_command.clone();
        let copied = copied.clone();
        Callback::from(move |_| {
            copy_to_clipboard(&active_command);
            copied.set(true);
        })
    };

    let toggle_command_preview = {
        let show_command_preview = show_command_preview.clone();
        Callback::from(move |_| show_command_preview.set(!*show_command_preview))
    };

    html! {
        <main class="app-shell">
            <aside class="side-rail">
                <div class="brand">
                    <span class="brand-mark material-symbols-rounded">{"movie_filter"}</span>
                    <div>
                        <strong>{"Webcoder"}</strong>
                        <small>{"WebAssembly"}</small>
                    </div>
                </div>
                <nav class="tab-list">
                    { for Tab::ALL.iter().map(|(item, icon, label)| {
                        let item = *item;
                        let tab = tab.clone();
                        let active = *tab == item;
                        html! {
                            <button
                                class={classes!("tab-button", active.then_some("active"))}
                                title={*label}
                                onclick={Callback::from(move |_| tab.set(item))}
                            >
                                <span class="material-symbols-rounded">{*icon}</span>
                                <b>{*label}</b>
                            </button>
                        }
                    })}
                </nav>
                <div class="rail-status">
                    <span>{state.files.len()}</span>
                    <small>{"inputs"}</small>
                </div>
            </aside>

            <section class="workspace">
                <header class="topbar">
                    <div>
                        <h1>{page_title(*tab)}</h1>
                        <p>{page_subtitle(*tab)}</p>
                    </div>
                    <div class="topbar-actions">
                        <button
                            class={classes!("icon-button", "subtle", "material-symbols-rounded", (*show_command_preview).then_some("active"))}
                            title={if *show_command_preview { "Hide command preview" } else { "Show command preview" }}
                            onclick={toggle_command_preview}
                        >
                            {"terminal"}
                        </button>
                    </div>
                </header>

                <section class={classes!("content-grid", (!*show_command_preview).then_some("preview-hidden"))}>
                    <div class="primary-pane">
                        { match *tab {
                            Tab::Media => view_media(&state, &job_ids, &job_log, &browser_encoders),
                            Tab::Convert => view_convert(&state),
                            Tab::Queue => view_queue(
                                &state,
                                &job_ids,
                                &results,
                            ),
                        }}
                    </div>
                    {
                        if *show_command_preview {
                            html! {
                                <aside class="preview-pane">
                                    <div class="panel-title">
                                        { icon("terminal") }
                                        <h2>{"Command Preview"}</h2>
                                    </div>
                                    <textarea class="command-box" readonly=true value={active_command.clone()} />
                                    <div class="preview-actions">
                                        <button class="icon-button material-symbols-rounded" title="Copy command" onclick={on_copy}>{"content_copy"}</button>
                                        <span class="copy-state">{ if *copied { "Copied" } else { "Ready" } }</span>
                                    </div>
                                    <div class="mini-stack">
                                        <strong>{"Selected"}</strong>
                                        { selected_summary(&state) }
                                    </div>
                                </aside>
                            }
                        } else {
                            Html::default()
                        }
                    }
                </section>
            </section>
        </main>
    }
}

fn page_title(tab: Tab) -> &'static str {
    match tab {
        Tab::Media => "Media",
        Tab::Convert => "Convert",
        Tab::Queue => "Queue",
    }
}

fn page_subtitle(tab: Tab) -> &'static str {
    match tab {
        Tab::Media => "Add files and set per-stream copy, strip, or transcode.",
        Tab::Convert => "Tune container, codecs, quality, resize, crop, and audio settings.",
        Tab::Queue => "Batch-encode every input on the server with native FFmpeg.",
    }
}

fn view_media(
    state: &UseStateHandle<AppState>,
    job_ids: &Rc<RefCell<HashMap<usize, String>>>,
    job_log: &UseStateHandle<String>,
    browser_encoders: &UseStateHandle<Vec<BrowserEncoder>>,
) -> Html {
    let on_files = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let job_log = job_log.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            let Some(files) = input.files() else {
                return;
            };

            // State captured at this render. Async probe callbacks must rebuild
            // the file list from this stable `base` (+ the shared track
            // accumulator below) rather than cloning the `state` handle, whose
            // value is frozen at this render — cloning it in an async task would
            // clobber the optimistic insert and make files vanish on upload.
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
            state.set(next);
            job_log.set("Uploading and probing media on the server...".into());

            let base = Rc::new(base);
            let pending_meta: Rc<Vec<(usize, MediaFile)>> =
                Rc::new(pending.iter().map(|(id, m, _)| (*id, m.clone())).collect());
            let acc: Rc<RefCell<HashMap<usize, Vec<Track>>>> =
                Rc::new(RefCell::new(HashMap::new()));

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
                                // Rebuild the authoritative file list from the
                                // stable base plus whatever tracks have arrived.
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
                                state.set(next);
                                job_log.set(format!("Loaded stream metadata for {name}."));
                            }
                            Err(error) => {
                                job_log.set(format!("Probe parse failed for {name}: {error}"))
                            }
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
        })
    };

    html! {
        <div class="media-grid">
            <div class="media-main">
                { view_tracks(state, browser_encoders) }
            </div>
            <aside class="media-files">
                <section class="drop-zone">
                    <input id="file-picker" type="file" multiple=true onchange={on_files} />
                    <label for="file-picker">
                        { icon("add") }
                        <strong>{"Select media files"}</strong>
                        <small>{"Files upload to the server and are probed with FFmpeg."}</small>
                    </label>
                </section>

                <div class="file-toolbar">
                    <button class="icon-button subtle material-symbols-rounded" title="Move selected file up" onclick={move_selected_file(state, -1)}>{"keyboard_arrow_up"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Move selected file down" onclick={move_selected_file(state, 1)}>{"keyboard_arrow_down"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Sort files by name" onclick={sort_files(state)}>{"sort_by_alpha"}</button>
                </div>

                <section class="file-list">
                    { for state.files.iter().map(|file| view_file_row(state, file)) }
                </section>
            </aside>
        </div>
    }
}

fn view_file_row(state: &UseStateHandle<AppState>, file: &MediaFile) -> Html {
    let file_id = file.id;
    let selected = state.selected_file == Some(file_id);
    let select_file = {
        let state = state.clone();
        Callback::from(move |_| {
            let mut next = (*state).clone();
            next.selected_file = Some(file_id);
            state.set(next);
        })
    };

    let remove_file = {
        let state = state.clone();
        Callback::from(move |_| {
            let mut next = (*state).clone();
            next.files.retain(|file| file.id != file_id);
            if next.selected_file == Some(file_id) {
                next.selected_file = next.files.first().map(|file| file.id);
            }
            state.set(next);
        })
    };

    html! {
        <article class={classes!("file-row", selected.then_some("selected"))}>
            <button class="row-main" onclick={select_file}>
                <span class="file-token material-symbols-rounded">{"movie"}</span>
                <span>
                    <strong>{&file.name}</strong>
                    <small>{format!("{} · {} tracks", format_size(file.size_bytes), file.tracks.len())}</small>
                </span>
            </button>
            <button class="icon-button subtle material-symbols-rounded" title="Remove file" onclick={remove_file}>{"delete"}</button>
        </article>
    }
}

fn move_selected_file(state: &UseStateHandle<AppState>, direction: isize) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let Some(file_id) = state.selected_file else {
            return;
        };

        let mut next = (*state).clone();
        if let Some(index) = next.files.iter().position(|file| file.id == file_id) {
            let target = (index as isize + direction).clamp(0, next.files.len() as isize - 1);
            next.files.swap(index, target as usize);
            state.set(next);
        }
    })
}

fn sort_files(state: &UseStateHandle<AppState>) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let mut next = (*state).clone();
        next.files
            .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        state.set(next);
    })
}

fn view_tracks(
    state: &UseStateHandle<AppState>,
    browser_encoders: &UseStateHandle<Vec<BrowserEncoder>>,
) -> Html {
    let Some(file) = state.selected_file() else {
        return empty_panel("No selected input");
    };

    let file_id = file.id;

    html! {
        <div class="stack">
            <div class="section-head">
                <div>
                    <span>{"STREAMS"}</span>
                    <h2>{&file.name}</h2>
                </div>
                <div class="preview-actions">
                    <button class="icon-button subtle material-symbols-rounded" title="Check all" onclick={set_tracks_checked(state, file_id, true)}>{"select_check_box"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Check none" onclick={set_tracks_checked(state, file_id, false)}>{"disabled_by_default"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Sort tracks" onclick={sort_tracks(state, file_id)}>{"sort"}</button>
                </div>
            </div>
            <div class="default-track-row">
                { view_default_track_select(file, StreamKind::Audio) }
                { view_default_track_select(file, StreamKind::Subtitle) }
            </div>
            <div class="track-table">
                <div class="track-head">
                    <span>{"On"}</span>
                    <span>{"Type"}</span>
                    <span>{"Codec"}</span>
                    <span>{"Language"}</span>
                    <span>{"Title"}</span>
                    <span>{"Output"}</span>
                    <span>{"Move"}</span>
                </div>
                { for file.tracks.iter().map(|track| view_track_row(state, file_id, track, browser_encoders)) }
            </div>
            <textarea class="stream-details" readonly=true value={stream_details(file)} />
        </div>
    }
}

fn view_track_row(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track: &Track,
    browser_encoders: &UseStateHandle<Vec<BrowserEncoder>>,
) -> Html {
    let track_id = track.id;
    html! {
        <div class="track-row">
            <input
                type="checkbox"
                checked={track.enabled}
                onchange={update_track_bool(state, file_id, track_id, |track, value| track.enabled = value)}
            />
            <span class="track-fact">{track.kind.label()}</span>
            <span class="track-fact" title={track.codec.clone()}>{&track.codec}</span>
            <input
                value={track.language.clone()}
                oninput={update_track_text(state, file_id, track_id, |track, value| track.language = value)}
            />
            <input
                value={track.title.clone()}
                oninput={update_track_text(state, file_id, track_id, |track, value| track.title = value)}
            />
            <select
                value={track.choice.label().to_owned()}
                onchange={update_track_codec(state, file_id, track_id)}
            >
                { encoder_options(browser_encoders, track.kind, &track.choice) }
            </select>
            <div class="row-actions">
                <button class="icon-button subtle material-symbols-rounded" title="Move up" onclick={move_track(state, file_id, track_id, -1)}>{"keyboard_arrow_up"}</button>
                <button class="icon-button subtle material-symbols-rounded" title="Move down" onclick={move_track(state, file_id, track_id, 1)}>{"keyboard_arrow_down"}</button>
            </div>
        </div>
    }
}

fn view_convert(state: &UseStateHandle<AppState>) -> Html {
    let settings = &state.convert;

    html! {
        <div class="settings-grid">
            <section class="settings-group">
                <div class="panel-title">{ icon("output") }<h2>{"Output"}</h2></div>
                { select_field("Container", settings.container.clone(), ALLOWED_CONTAINERS, update_convert_select(state, |settings, value| settings.container = value)) }
                { select_field("Preset", settings.preset.clone(), &["ultrafast", "veryfast", "fast", "medium", "slow", "slower"], update_convert_select(state, |settings, value| settings.preset = value)) }
                { select_field("Color Format", settings.color_format.clone(), &["source", "yuv420p", "yuv420p10le", "yuv444p", "rgb24"], update_convert_select(state, |settings, value| settings.color_format = value)) }
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("speed") }<h2>{"Quality"}</h2></div>
                <select
                    value={settings.quality_mode.label()}
                    onchange={update_quality_mode(state)}
                >
                    { selected_option(QualityMode::ConstantQuality.label(), settings.quality_mode.label()) }
                    { selected_option(QualityMode::Bitrate.label(), settings.quality_mode.label()) }
                    { selected_option(QualityMode::FileSize.label(), settings.quality_mode.label()) }
                </select>
                { number_field("CRF / CQ", settings.quality_value, 1, 63, update_convert_number(state, |settings, value| settings.quality_value = value)) }
                { number_field("Bitrate kbps", settings.bitrate_kbps, 64, 250000, update_convert_number(state, |settings, value| settings.bitrate_kbps = value)) }
                { number_field("Target MB", settings.target_size_mb, 1, 500000, update_convert_number(state, |settings, value| settings.target_size_mb = value)) }
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("movie") }<h2>{"Video"}</h2></div>
                <div class="three-col">
                    { text_field("FPS", settings.fps.clone(), update_convert_text(state, |settings, value| settings.fps = value)) }
                    { text_field("Scale", settings.resize.clone(), update_convert_text(state, |settings, value| settings.resize = value)) }
                    { select_field("Crop Mode", settings.crop_mode.clone(), &["Disable", "Manual"], update_convert_select(state, |settings, value| settings.crop_mode = value)) }
                    { text_field("Crop", settings.crop.clone(), update_convert_text(state, |settings, value| settings.crop = value)) }
                </div>
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("graphic_eq") }<h2>{"Audio"}</h2></div>
                { select_field("Channels", settings.audio_channels.clone(), &["source", "1", "2", "6", "8"], update_convert_select(state, |settings, value| settings.audio_channels = value)) }
                { number_field("Stereo kbps", settings.audio_bitrate_kbps, 0, 6400, update_convert_number(state, |settings, value| settings.audio_bitrate_kbps = value)) }
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("subtitles") }<h2>{"Subtitles"}</h2></div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={settings.burn_subtitles}
                        onchange={update_convert_bool(state, |settings, value| settings.burn_subtitles = value)}
                    />
                    <span>{"Burn in selected subtitle stream"}</span>
                </label>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("tune") }<h2>{"Advanced"}</h2></div>
                <div class="three-col">
                    { text_field("Trim Start", settings.trim_start.clone(), update_convert_text(state, |settings, value| settings.trim_start = value)) }
                    { text_field("Trim End", settings.trim_end.clone(), update_convert_text(state, |settings, value| settings.trim_end = value)) }
                    { text_field("Duration", settings.trim_duration.clone(), update_convert_text(state, |settings, value| settings.trim_duration = value)) }
                </div>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("newspaper") }<h2>{"Metadata"}</h2></div>
                <div class="two-col">
                    { select_field("Metadata", settings.metadata_mode.clone(), &[METADATA_COPY, METADATA_STRIP_KEEP_TRACKS, METADATA_STRIP_ALL], update_convert_select(state, |settings, value| settings.metadata_mode = value)) }
                    { select_field("Chapters", settings.chapter_mode.clone(), &[CHAPTERS_COPY, CHAPTERS_STRIP], update_convert_select(state, |settings, value| settings.chapter_mode = value)) }
                </div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={settings.apply_track_metadata}
                        onchange={update_convert_bool(state, |settings, value| settings.apply_track_metadata = value)}
                    />
                    <span>{"Apply track titles and languages from Track List"}</span>
                </label>
            </section>
        </div>
    }
}

fn view_queue(
    state: &UseStateHandle<AppState>,
    job_ids: &Rc<RefCell<HashMap<usize, String>>>,
    results: &UseStateHandle<Vec<EncodeItem>>,
) -> Html {
    let ready = state
        .files
        .iter()
        .filter(|file| job_ids.borrow().contains_key(&file.id))
        .count();

    let run_batch = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let results = results.clone();
        Callback::from(move |_| {
            // Snapshot every ready input up front (job id, per-file tracks,
            // per-file output name). The whole batch runs in one task so the
            // authoritative result list lives in `items` here — we only ever
            // write it to the state handle, never read the (stale) handle back.
            let settings = (*state).convert.clone();
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
                            let zip_link = if done_ids.len() > 1 {
                                let href = format!("/api/zip?jobs={}", done_ids.join(","));
                                html! {
                                    <a class="command-button accent result-download" href={href} download="webcoder-batch.zip">
                                        { icon("folder_zip") }
                                        { "Download all" }
                                    </a>
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
                        html! { for results.iter().map(view_result_row) }
                    } }
                </div>
            </section>
        </div>
    }
}

fn view_result_row(item: &EncodeItem) -> Html {
    let (badge, label, status_class) = match item.status {
        EncodeStatus::Running => ("hourglass_top", "Encoding", "is-running"),
        EncodeStatus::Done => ("check_circle", "Done", "is-done"),
        EncodeStatus::Failed => ("error", "Failed", "is-failed"),
    };
    let open = item.status == EncodeStatus::Failed;
    html! {
        <div class={classes!("result-card", status_class)}>
            <div class="result-head">
                <span class="material-symbols-rounded result-badge">{badge}</span>
                <strong>{&item.name}</strong>
                <span class="result-status">{label}</span>
                {
                    if !item.download_url.is_empty() {
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

fn selected_summary(state: &UseStateHandle<AppState>) -> Html {
    if let Some(file) = state.selected_file() {
        html! {
            <div class="summary">
                <span>{&file.name}</span>
                <span>{format_size(file.size_bytes)}</span>
                <span>{format!("{}.{}", state.convert.output_name, state.convert.container)}</span>
            </div>
        }
    } else {
        html! { <div class="summary muted">{"No file selected"}</div> }
    }
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

/// Build the output-codec `<option>` list for a track: `Copy`/`Strip` plus
/// every detected browser encoder matching the stream kind. The currently
/// selected encoder is always included so a probe finishing later never drops
/// the user's choice.
fn encoder_options(
    browser_encoders: &[BrowserEncoder],
    kind: StreamKind,
    selected: &TrackOutput,
) -> Html {
    let selected_label = selected.label();

    let mut names: Vec<&str> = browser_encoders
        .iter()
        .filter(|encoder| encoder.kind == kind)
        .map(|encoder| encoder.name.as_str())
        .collect();

    if let TrackOutput::Encoder(name) = selected {
        if !names.contains(&name.as_str()) {
            names.push(name.as_str());
        }
    }

    html! {
        <>
            { option_selected("Copy", selected_label == "Copy") }
            { option_selected("Strip", selected_label == "Strip") }
            { for names.iter().map(|name| encoder_option(name, *name == selected_label)) }
        </>
    }
}

fn encoder_option(name: &str, selected: bool) -> Html {
    html! {
        <option value={name.to_owned()} selected={selected}>{friendly_encoder_name(name)}</option>
    }
}

fn friendly_encoder_name(name: &str) -> &str {
    match name {
        "libmp3lame" => "MP3 (libmp3lame)",
        "aac" => "AAC / M4A (aac)",
        "libopus" => "Opus (libopus)",
        "opus" => "Opus (opus)",
        "flac" => "FLAC (flac)",
        "pcm_s16le" => "WAV PCM 16-bit (pcm_s16le)",
        _ => name,
    }
}

fn set_tracks_checked(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    checked: bool,
) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let mut next = (*state).clone();
        if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
            for track in &mut file.tracks {
                track.enabled = checked;
            }
        }
        state.set(next);
    })
}

fn sort_tracks(state: &UseStateHandle<AppState>, file_id: usize) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let mut next = (*state).clone();
        if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
            file.tracks
                .sort_by_key(|track| (track.kind.label().to_owned(), track.language.clone()));
        }
        state.set(next);
    })
}

fn default_track_label(file: &MediaFile, kind: StreamKind) -> String {
    file.tracks
        .iter()
        .find(|track| track.kind == kind && track.enabled)
        .map(track_label)
        .unwrap_or_else(|| "None".into())
}

fn view_default_track_select(file: &MediaFile, kind: StreamKind) -> Html {
    let label = match kind {
        StreamKind::Audio => "Default Audio Track",
        StreamKind::Subtitle => "Default Subtitle Track",
        _ => "Default Track",
    };
    let selected = default_track_label(file, kind);
    html! {
        <label>
            <span>{label}</span>
            <select value={selected.clone()} onchange={noop_select()}>
                <option value="None" selected={selected == "None"}>{"None"}</option>
                { for file.tracks.iter().filter(|track| track.kind == kind).map(|track| {
                    let label = track_label(track);
                    html! { <option value={label.clone()} selected={label == selected}>{label}</option> }
                }) }
            </select>
        </label>
    }
}

fn track_label(track: &Track) -> String {
    format!("#{} {} {}", track.id, track.language, track.title)
}

fn stream_details(file: &MediaFile) -> String {
    file.tracks
        .iter()
        .map(|track| {
            format!(
                "#{} {} - Codec: {} - Language: {} - Title: {} - Output: {}",
                track.id,
                track.kind.label(),
                track.codec,
                track.language,
                track.title,
                track.choice.label()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn update_track_text(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
    update: fn(&mut Track, String),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        update_track(&state, file_id, track_id, |track| update(track, value));
    })
}

fn update_track_bool(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
    update: fn(&mut Track, bool),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlInputElement>().checked();
        update_track(&state, file_id, track_id, |track| update(track, value));
    })
}

fn update_track_codec(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let choice = TrackOutput::from_label(&value);
        update_track(&state, file_id, track_id, |track| {
            track.choice = choice.clone()
        });
    })
}

fn move_track(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
    direction: isize,
) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let mut next = (*state).clone();
        if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
            if let Some(index) = file.tracks.iter().position(|track| track.id == track_id) {
                let target = (index as isize + direction).clamp(0, file.tracks.len() as isize - 1);
                file.tracks.swap(index, target as usize);
            }
        }
        state.set(next);
    })
}

fn update_track<F>(state: &UseStateHandle<AppState>, file_id: usize, track_id: usize, update: F)
where
    F: FnOnce(&mut Track),
{
    let mut next = (**state).clone();
    if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
        if let Some(track) = file.tracks.iter_mut().find(|track| track.id == track_id) {
            update(track);
        }
    }
    state.set(next);
}

fn update_convert_text(
    state: &UseStateHandle<AppState>,
    update: fn(&mut ConvertSettings, String),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        let mut next = (*state).clone();
        update(&mut next.convert, value);
        state.set(next);
    })
}

fn update_convert_select(
    state: &UseStateHandle<AppState>,
    update: fn(&mut ConvertSettings, String),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let mut next = (*state).clone();
        update(&mut next.convert, value);
        state.set(next);
    })
}

fn update_convert_number(
    state: &UseStateHandle<AppState>,
    update: fn(&mut ConvertSettings, u32),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event
            .target_unchecked_into::<HtmlInputElement>()
            .value()
            .parse()
            .unwrap_or_default();
        let mut next = (*state).clone();
        update(&mut next.convert, value);
        state.set(next);
    })
}

fn update_convert_bool(
    state: &UseStateHandle<AppState>,
    update: fn(&mut ConvertSettings, bool),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlInputElement>().checked();
        let mut next = (*state).clone();
        update(&mut next.convert, value);
        state.set(next);
    })
}

fn update_quality_mode(state: &UseStateHandle<AppState>) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let mut next = (*state).clone();
        next.convert.quality_mode = match value.as_str() {
            "Target bitrate" => QualityMode::Bitrate,
            "Target file size" => QualityMode::FileSize,
            _ => QualityMode::ConstantQuality,
        };
        state.set(next);
    })
}

fn selected_option(value: &str, selected: &str) -> Html {
    option_selected(value, value == selected)
}

fn option_selected(value: &str, selected: bool) -> Html {
    html! { <option value={value.to_owned()} selected={selected}>{value}</option> }
}

fn icon(name: &str) -> Html {
    html! { <span class="material-symbols-rounded">{name}</span> }
}

fn noop_select() -> Callback<Event> {
    Callback::from(|_| {})
}

fn text_field(label: &str, value: String, oninput: Callback<InputEvent>) -> Html {
    html! {
        <label>
            <span>{label}</span>
            <input value={value} oninput={oninput} />
        </label>
    }
}

fn number_field(
    label: &str,
    value: u32,
    min: u32,
    max: u32,
    oninput: Callback<InputEvent>,
) -> Html {
    html! {
        <label>
            <span>{label}</span>
            <input type="number" min={min.to_string()} max={max.to_string()} value={value.to_string()} oninput={oninput} />
        </label>
    }
}

fn select_field(label: &str, value: String, values: &[&str], onchange: Callback<Event>) -> Html {
    let selected = value.clone();
    html! {
        <label>
            <span>{label}</span>
            <select value={value} onchange={onchange}>
                { for values.iter().map(|option_value| selected_option(option_value, &selected)) }
            </select>
        </label>
    }
}

fn empty_panel(message: &str) -> Html {
    html! {
        <section class="empty-panel">
            <strong>{message}</strong>
        </section>
    }
}

fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
}
