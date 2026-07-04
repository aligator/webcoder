use crate::core::{
    AppState, Av1anSettings, CodecChoice, ConvertSettings, MediaFile, Mode, QualityMode,
    StreamKind, Track, Utility, command_preview, ffmpeg_args, format_size, output_file_name,
    utility_command,
};
use js_sys::{Array, Reflect};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, File, HtmlInputElement, HtmlSelectElement, InputEvent};
use yew::TargetCast;
use yew::prelude::*;

#[wasm_bindgen(module = "/assets/ffmpeg_bridge.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = probeMedia)]
    async fn probe_media(file: File, input_name: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = runFfmpeg)]
    async fn run_ffmpeg(
        file: File,
        input_name: String,
        output_name: String,
        args: JsValue,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = getFfmpegCapabilities)]
    async fn get_ffmpeg_capabilities() -> Result<JsValue, JsValue>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tab {
    Input,
    Tracks,
    Convert,
    Av1an,
    Utilities,
    Queue,
}

impl Tab {
    const ALL: &'static [(Self, &'static str, &'static str)] = &[
        (Self::Input, "input", "Input"),
        (Self::Tracks, "checklist", "Tracks"),
        (Self::Convert, "tune", "Convert"),
        (Self::Av1an, "movie", "AV1AN"),
        (Self::Utilities, "construction", "Utilities"),
        (Self::Queue, "queue", "Queue"),
    ];
}

#[function_component(App)]
pub fn app() -> Html {
    let state = use_state(AppState::default);
    let tab = use_state(|| Tab::Input);
    let copied = use_state(|| false);
    let show_command_preview = use_state(|| false);
    let job_log = use_state(|| "Browser FFmpeg runtime idle.".to_owned());
    let browser_encoders = use_state(Vec::<String>::new);
    let browser_capability_status =
        use_state(|| "Browser FFmpeg capabilities not checked.".to_owned());
    let output_url = use_state(String::new);
    let output_name = use_state(String::new);
    let browser_files = use_mut_ref(HashMap::<usize, File>::new);

    let active_command = command_preview(&state);
    let utility = utility_command(&state);

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
                        <div class="mode-switch">
                            { mode_button(&state, Mode::Mux) }
                            { mode_button(&state, Mode::Batch) }
                        </div>
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
                            Tab::Input => view_input(&state, &browser_files, &job_log),
                            Tab::Tracks => view_tracks(&state),
                            Tab::Convert => view_convert(&state),
                            Tab::Av1an => view_av1an(&state),
                            Tab::Utilities => view_utilities(&state, &utility),
                            Tab::Queue => view_queue(
                                &state,
                                &browser_files,
                                &active_command,
                                &on_copy,
                                *copied,
                                &job_log,
                                &browser_encoders,
                                &browser_capability_status,
                                &output_url,
                                &output_name,
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
        Tab::Input => "Input",
        Tab::Tracks => "Track List",
        Tab::Convert => "Convert",
        Tab::Av1an => "AV1AN Chunking",
        Tab::Utilities => "Utilities",
        Tab::Queue => "Queue",
    }
}

fn page_subtitle(tab: Tab) -> &'static str {
    match tab {
        Tab::Input => "Add media files and choose muxing or batch processing.",
        Tab::Tracks => "Enable, rename, reorder, copy, strip, or transcode streams.",
        Tab::Convert => "Tune container, codecs, quality, resize, crop, and audio settings.",
        Tab::Av1an => "Prepare chunked encodes with worker, splitter, resume, and grain controls.",
        Tab::Utilities => "Build analysis, metadata, concat, and bitrate sampling commands.",
        Tab::Queue => "Review the generated job before sending it to your native toolchain.",
    }
}

fn view_input(
    state: &UseStateHandle<AppState>,
    browser_files: &Rc<RefCell<HashMap<usize, File>>>,
    job_log: &UseStateHandle<String>,
) -> Html {
    let on_files = {
        let state = state.clone();
        let browser_files = browser_files.clone();
        let job_log = job_log.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            let Some(files) = input.files() else {
                return;
            };

            let mut next = (*state).clone();
            let mut new_files = Vec::new();
            let mut probes = Vec::new();
            for index in 0..files.length() {
                if let Some(file) = files.get(index) {
                    let id = next.files.len() + new_files.len() + 1;
                    browser_files.borrow_mut().insert(id, file.clone());
                    probes.push((id, file.clone(), file.name()));
                    new_files.push(file_to_media(id, &file));
                }
            }

            if !new_files.is_empty() {
                next.selected_file = new_files.first().map(|file| file.id);
                next.files.extend(new_files);
                state.set(next);
                job_log.set("Probing media streams with FFmpeg WASM...".into());

                for (id, file, name) in probes {
                    let state = state.clone();
                    let job_log = job_log.clone();
                    spawn_local(async move {
                        match probe_media(file, name.clone()).await {
                            Ok(result) => {
                                let tracks = tracks_from_probe(&result);
                                let log = Reflect::get(&result, &JsValue::from_str("log"))
                                    .ok()
                                    .and_then(|value| value.as_string())
                                    .unwrap_or_default();
                                if tracks.is_empty() {
                                    job_log.set(format!(
                                        "FFmpeg probe finished for {name}, but no streams were parsed.\n\n{log}"
                                    ));
                                } else {
                                    let mut next = (*state).clone();
                                    if let Some(media) =
                                        next.files.iter_mut().find(|media| media.id == id)
                                    {
                                        media.tracks = merge_probe_tracks(&media.tracks, tracks);
                                    }
                                    state.set(next);
                                    job_log.set(format!("Loaded stream metadata for {name}."));
                                }
                            }
                            Err(error) => {
                                job_log.set(format!(
                                    "FFmpeg probe failed for {name}: {}",
                                    js_error_text(error)
                                ));
                            }
                        }
                    });
                }
            }
        })
    };

    html! {
        <div class="stack">
            <section class="drop-zone">
                <input id="file-picker" type="file" multiple=true onchange={on_files} />
                <label for="file-picker">
                    { icon("add") }
                    <strong>{"Select media files"}</strong>
                    <small>{"Streams are probed locally in the browser."}</small>
                </label>
            </section>

            <div class="file-toolbar">
                <button class="icon-button subtle material-symbols-rounded" title="Move selected file up" onclick={move_selected_file(state, -1)}>{"keyboard_arrow_up"}</button>
                <button class="icon-button subtle material-symbols-rounded" title="Move selected file down" onclick={move_selected_file(state, 1)}>{"keyboard_arrow_down"}</button>
                <button class="icon-button subtle material-symbols-rounded" title="Sort files by name" onclick={sort_files(state)}>{"sort_by_alpha"}</button>
                <button class="icon-button subtle material-symbols-rounded" title="Load tracks from selected file" onclick={select_first_track_file(state)}>{"low_priority"}</button>
            </div>

            <section class="file-list">
                { for state.files.iter().map(|file| view_file_row(state, file)) }
            </section>
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

fn select_first_track_file(state: &UseStateHandle<AppState>) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| {
        let mut next = (*state).clone();
        if next.selected_file.is_none() {
            next.selected_file = next.files.first().map(|file| file.id);
        }
        state.set(next);
    })
}

fn view_tracks(state: &UseStateHandle<AppState>) -> Html {
    let Some(file) = state.selected_file() else {
        return empty_panel("No selected input");
    };

    let file_id = file.id;
    let add_track = {
        let state = state.clone();
        Callback::from(move |_| {
            let mut next = (*state).clone();
            if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
                let id = file.tracks.iter().map(|track| track.id).max().unwrap_or(0) + 1;
                file.tracks.push(Track {
                    id,
                    source_index: id.saturating_sub(1),
                    enabled: true,
                    kind: StreamKind::Audio,
                    codec: "Unknown".into(),
                    language: "und".into(),
                    title: "New track".into(),
                    choice: CodecChoice::Copy,
                });
            }
            state.set(next);
        })
    };

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
                    <button class="command-button" onclick={add_track}>{"Add track"}</button>
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
                { for file.tracks.iter().map(|track| view_track_row(state, file_id, track)) }
            </div>
            <textarea class="stream-details" readonly=true value={stream_details(file)} />
        </div>
    }
}

fn view_track_row(state: &UseStateHandle<AppState>, file_id: usize, track: &Track) -> Html {
    let track_id = track.id;
    html! {
        <div class="track-row">
            <input
                type="checkbox"
                checked={track.enabled}
                onchange={update_track_bool(state, file_id, track_id, |track, value| track.enabled = value)}
            />
            <select
                value={track.kind.label()}
                onchange={update_track_kind(state, file_id, track_id)}
            >
                { selected_option("Video", track.kind.label()) }
                { selected_option("Audio", track.kind.label()) }
                { selected_option("Subtitle", track.kind.label()) }
                { selected_option("Attachment", track.kind.label()) }
            </select>
            <input
                value={track.codec.clone()}
                oninput={update_track_text(state, file_id, track_id, |track, value| track.codec = value)}
            />
            <input
                value={track.language.clone()}
                oninput={update_track_text(state, file_id, track_id, |track, value| track.language = value)}
            />
            <input
                value={track.title.clone()}
                oninput={update_track_text(state, file_id, track_id, |track, value| track.title = value)}
            />
            <select
                value={track.choice.label()}
                onchange={update_track_codec(state, file_id, track_id, track.kind)}
            >
                { for codecs_for(track.kind).iter().map(|choice| selected_option(choice.label(), track.choice.label())) }
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
                { text_field("Name", settings.output_name.clone(), update_convert_text(state, |settings, value| settings.output_name = value)) }
                { select_field("Container", settings.container.clone(), &["mkv", "mp4", "mov", "webm", "gif"], update_convert_select(state, |settings, value| settings.container = value)) }
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
                    { selected_option(QualityMode::Vmaf.label(), settings.quality_mode.label()) }
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
                    { select_field("Crop Mode", settings.crop_mode.clone(), &["Disable", "Manual", "Automatic"], update_convert_select(state, |settings, value| settings.crop_mode = value)) }
                    { text_field("Crop", settings.crop.clone(), update_convert_text(state, |settings, value| settings.crop = value)) }
                </div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={settings.burn_subtitles}
                        onchange={update_convert_bool(state, |settings, value| settings.burn_subtitles = value)}
                    />
                    <span>{"Burn subtitle track"}</span>
                </label>
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
                <div class="two-col">
                    { text_field("Custom Args In", settings.custom_args_in.clone(), update_convert_text(state, |settings, value| settings.custom_args_in = value)) }
                    { text_field("Custom Args Out", settings.custom_args_out.clone(), update_convert_text(state, |settings, value| settings.custom_args_out = value)) }
                </div>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("newspaper") }<h2>{"Metadata"}</h2></div>
                <div class="two-col">
                    { select_field("Copy Metadata From", settings.metadata_mode.clone(), &["Copy All From Input, Edit Titles/Languages", "Apply Titles/Languages, Strip Rest", "Strip All Metadata Including Titles/Languages"], update_convert_select(state, |settings, value| settings.metadata_mode = value)) }
                    { select_field("Copy Chapters From", settings.chapter_mode.clone(), &["Copy All From Input, Edit Titles/Languages", "Apply Titles/Languages, Strip Rest", "Strip All Metadata Including Titles/Languages"], update_convert_select(state, |settings, value| settings.chapter_mode = value)) }
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

fn view_av1an(state: &UseStateHandle<AppState>) -> Html {
    let av1an = &state.av1an;

    html! {
        <div class="settings-grid">
            <section class="settings-group wide">
                <div class="panel-title">{ icon("video_settings") }<h2>{"Chunked Encoding"}</h2></div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={av1an.enabled}
                        onchange={update_av1an_bool(state, |settings, value| settings.enabled = value)}
                    />
                    <span>{"Use AV1AN command builder"}</span>
                </label>
                <div class="three-col">
                    <label>
                        <span>{"Encoder"}</span>
                        <select value={av1an.encoder.label()} onchange={update_av1an_encoder(state)}>
                            { selected_option(CodecChoice::Av1Svt.label(), av1an.encoder.label()) }
                            { selected_option(CodecChoice::Av1Aom.label(), av1an.encoder.label()) }
                            { selected_option(CodecChoice::Vp9.label(), av1an.encoder.label()) }
                            { selected_option(CodecChoice::H265X265.label(), av1an.encoder.label()) }
                        </select>
                    </label>
                    { number_field("Workers", av1an.workers, 1, 64, update_av1an_number(state, |settings, value| settings.workers = value)) }
                    { number_field("Threads", av1an.threads, 0, 128, update_av1an_number(state, |settings, value| settings.threads = value)) }
                    { number_field("Target VMAF", av1an.target_vmaf, 1, 100, update_av1an_number(state, |settings, value| settings.target_vmaf = value)) }
                </div>
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("call_split") }<h2>{"Splitting"}</h2></div>
                { select_field("Split method", av1an.splitter.clone(), &["scenedetect", "ffms2", "none"], update_av1an_select(state, |settings, value| settings.splitter = value)) }
                { select_field("Chunk method", av1an.chunk_method.clone(), &["segment", "lsmash", "hybrid"], update_av1an_select(state, |settings, value| settings.chunk_method = value)) }
                { select_field("Concat mode", av1an.concat_mode.clone(), &["ffmpeg", "mkvmerge", "ivf"], update_av1an_select(state, |settings, value| settings.concat_mode = value)) }
                { select_field("Chunk order", av1an.chunk_order.clone(), &["long-to-short", "sequential", "random"], update_av1an_select(state, |settings, value| settings.chunk_order = value)) }
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("play_circle") }<h2>{"Runtime"}</h2></div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={av1an.resume}
                        onchange={update_av1an_bool(state, |settings, value| settings.resume = value)}
                    />
                    <span>{"Resume stopped encodes"}</span>
                </label>
                { number_field("Film grain", av1an.film_grain, 0, 50, update_av1an_number(state, |settings, value| settings.film_grain = value)) }
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={av1an.grain_denoise}
                        onchange={update_av1an_bool(state, |settings, value| settings.grain_denoise = value)}
                    />
                    <span>{"Denoise before grain synthesis"}</span>
                </label>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("merge") }<h2>{"Copy Streams"}</h2></div>
                <div class="three-col">
                    <label class="check-line">
                        <input
                            type="checkbox"
                            checked={av1an.copy_subtitles}
                            onchange={update_av1an_bool(state, |settings, value| settings.copy_subtitles = value)}
                        />
                        <span>{"Copy subtitles"}</span>
                    </label>
                    <label class="check-line">
                        <input
                            type="checkbox"
                            checked={av1an.copy_attachments}
                            onchange={update_av1an_bool(state, |settings, value| settings.copy_attachments = value)}
                        />
                        <span>{"Copy attachments"}</span>
                    </label>
                    <label class="check-line">
                        <input
                            type="checkbox"
                            checked={av1an.copy_data}
                            onchange={update_av1an_bool(state, |settings, value| settings.copy_data = value)}
                        />
                        <span>{"Copy data streams"}</span>
                    </label>
                </div>
                <div class="two-col">
                    { text_field("Custom Encoder Args", av1an.custom_encoder_args.clone(), update_av1an_text(state, |settings, value| settings.custom_encoder_args = value)) }
                    { text_field("Custom AV1AN Args", av1an.custom_av1an_args.clone(), update_av1an_text(state, |settings, value| settings.custom_av1an_args = value)) }
                </div>
            </section>
        </div>
    }
}

fn view_utilities(state: &UseStateHandle<AppState>, command: &str) -> Html {
    html! {
        <div class="stack">
            <section class="utility-grid">
                { for Utility::ALL.iter().map(|utility| {
                    let item = *utility;
                    let active = state.utility == item;
                    let state = state.clone();
                    html! {
                        <button
                            class={classes!("utility-card", active.then_some("active"))}
                            onclick={Callback::from(move |_| {
                                let mut next = (*state).clone();
                                next.utility = item;
                                state.set(next);
                            })}
                        >
                            <strong>{item.label()}</strong>
                            <span>{item.description()}</span>
                        </button>
                    }
                })}
            </section>
            <section class="settings-group wide">
                <div class="panel-title">{ icon("construction") }<h2>{state.utility.label()}</h2></div>
                <textarea class="command-box tall" readonly=true value={command.to_owned()} />
            </section>
        </div>
    }
}

fn view_queue(
    state: &UseStateHandle<AppState>,
    browser_files: &Rc<RefCell<HashMap<usize, File>>>,
    command: &str,
    on_copy: &Callback<MouseEvent>,
    copied: bool,
    job_log: &UseStateHandle<String>,
    browser_encoders: &UseStateHandle<Vec<String>>,
    browser_capability_status: &UseStateHandle<String>,
    output_url: &UseStateHandle<String>,
    output_name: &UseStateHandle<String>,
) -> Html {
    let detect_capabilities = {
        let browser_encoders = browser_encoders.clone();
        let browser_capability_status = browser_capability_status.clone();
        let job_log = job_log.clone();
        Callback::from(move |_| {
            browser_capability_status.set("Checking bundled FFmpeg WASM encoders...".into());
            job_log.set("Checking bundled FFmpeg WASM encoders...".into());

            let browser_encoders = browser_encoders.clone();
            let browser_capability_status = browser_capability_status.clone();
            let job_log = job_log.clone();
            spawn_local(async move {
                match fetch_browser_encoders().await {
                    Ok((encoders, log)) => {
                        let summary = browser_codec_summary(&encoders);
                        browser_encoders.set(encoders);
                        browser_capability_status.set(summary.clone());
                        job_log.set(format!("{summary}\n\n{log}"));
                    }
                    Err(error) => {
                        let message =
                            format!("Browser capability check failed: {}", js_error_text(error));
                        browser_capability_status.set(message.clone());
                        job_log.set(message);
                    }
                }
            });
        })
    };

    let run_job = {
        let state = state.clone();
        let browser_files = browser_files.clone();
        let job_log = job_log.clone();
        let browser_encoders = browser_encoders.clone();
        let browser_capability_status = browser_capability_status.clone();
        let output_url = output_url.clone();
        let output_name = output_name.clone();
        Callback::from(move |_| {
            if state.av1an.enabled {
                job_log.set("AV1AN is a native CLI workflow; switch AV1AN off to run FFmpeg in the browser.".into());
                return;
            }

            let Some(file_id) = state.selected_file else {
                job_log.set("Select an input before running a browser encode.".into());
                return;
            };

            let Some(file) = browser_files.borrow().get(&file_id).cloned() else {
                job_log.set("Browser execution needs an input selected from this page.".into());
                return;
            };

            let Some(selected) = state.selected_file().cloned() else {
                job_log.set("Select an input before running a browser encode.".into());
                return;
            };

            let args = ffmpeg_args(&state);
            if args.is_empty() {
                job_log.set("No FFmpeg arguments were generated for this job.".into());
                return;
            }

            let output = output_file_name(&state);

            job_log.set("Checking browser FFmpeg encoder support...".into());
            output_url.set(String::new());
            output_name.set(output.clone());

            let job_log_async = job_log.clone();
            let browser_encoders_async = browser_encoders.clone();
            let browser_capability_status_async = browser_capability_status.clone();
            let output_url_async = output_url.clone();
            spawn_local(async move {
                let encoders = if browser_encoders_async.is_empty() {
                    match fetch_browser_encoders().await {
                        Ok((encoders, _log)) => {
                            browser_capability_status_async.set(browser_codec_summary(&encoders));
                            browser_encoders_async.set(encoders.clone());
                            encoders
                        }
                        Err(error) => {
                            job_log_async.set(format!(
                                "Browser capability check failed: {}",
                                js_error_text(error)
                            ));
                            return;
                        }
                    }
                } else {
                    (*browser_encoders_async).clone()
                };

                let missing = missing_requested_encoders(&args, &encoders);
                if !missing.is_empty() {
                    job_log_async.set(format!(
                        "This generated command is valid for native FFmpeg, but the bundled browser FFmpeg core does not expose these requested encoder(s): {}.\n\nNothing was rewritten. Copy the native command, choose codecs that appear in the browser capability list, or replace the bundled FFmpeg WASM core with one that includes those encoders.\n\n{}",
                        missing.join(", "),
                        browser_codec_summary(&encoders)
                    ));
                    return;
                }

                let js_args = Array::new();
                for arg in args {
                    js_args.push(&JsValue::from_str(&arg));
                }

                job_log_async.set("Loading FFmpeg WASM and starting job...".into());
                match run_ffmpeg(file, selected.name, output.clone(), js_args.into()).await {
                    Ok(result) => {
                        let url = Reflect::get(&result, &JsValue::from_str("url"))
                            .ok()
                            .and_then(|value| value.as_string())
                            .unwrap_or_default();
                        let log = Reflect::get(&result, &JsValue::from_str("log"))
                            .ok()
                            .and_then(|value| value.as_string())
                            .unwrap_or_else(|| "Browser encode complete.".into());
                        output_url_async.set(url);
                        job_log_async.set(log);
                    }
                    Err(error) => {
                        job_log_async
                            .set(format!("Browser encode failed: {}", js_error_text(error)));
                    }
                }
            });
        })
    };

    html! {
        <div class="stack">
            <section class="queue-board">
                <div class="queue-metric">
                    <span>{state.files.len()}</span>
                    <small>{"inputs"}</small>
                </div>
                <div class="queue-metric">
                    <span>{state.selected_file().map(|file| file.tracks.iter().filter(|track| track.enabled).count()).unwrap_or(0)}</span>
                    <small>{"streams"}</small>
                </div>
                <div class="queue-metric">
                    <span>{ if state.av1an.enabled { "AV1AN" } else { "FFmpeg" } }</span>
                    <small>{"backend"}</small>
                </div>
                <div class="queue-metric">
                    <span>{ if browser_encoders.is_empty() { "?".to_owned() } else { browser_encoders.len().to_string() } }</span>
                    <small>{"browser encoders"}</small>
                </div>
            </section>
            <section class="settings-group wide">
                <div class="panel-title">{ icon("terminal") }<h2>{"Generated Job"}</h2></div>
                <textarea class="command-box tall" readonly=true value={command.to_owned()} />
                <div class="preview-actions">
                    <button class="command-button" onclick={on_copy.clone()}>{ if copied { "Copied" } else { "Copy command" } }</button>
                    <button class="command-button" onclick={detect_capabilities}>{"Detect browser codecs"}</button>
                    <button class="command-button accent" onclick={run_job}>{"Run in browser"}</button>
                </div>
                <div class="codec-summary">{(**browser_capability_status).clone()}</div>
            </section>
            <section class="settings-group wide">
                <div class="panel-title">{ icon("play_circle") }<h2>{"Browser Runtime"}</h2></div>
                <textarea class="command-box log" readonly=true value={(**job_log).clone()} />
                {
                    if !output_url.is_empty() {
                        html! {
                            <a class="download-link" href={(**output_url).clone()} download={(**output_name).clone()}>
                                {format!("Download {}", **output_name)}
                            </a>
                        }
                    } else {
                        Html::default()
                    }
                }
            </section>
        </div>
    }
}

fn mode_button(state: &UseStateHandle<AppState>, mode: Mode) -> Html {
    let active = state.convert.mode == mode;
    let state = state.clone();
    html! {
        <button
            class={classes!("segmented-button", active.then_some("active"))}
            onclick={Callback::from(move |_| {
                let mut next = (*state).clone();
                next.convert.mode = mode;
                state.set(next);
            })}
        >
            {mode.label()}
        </button>
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
            choice: CodecChoice::Eac3,
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
            choice: CodecChoice::Av1Svt,
        });

        tracks.push(Track {
            id: 2,
            source_index: 1,
            enabled: true,
            kind: StreamKind::Audio,
            codec: "Audio".into(),
            language: "und".into(),
            title: "Main audio".into(),
            choice: CodecChoice::Eac3,
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
            choice: CodecChoice::Copy,
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

fn tracks_from_probe(result: &JsValue) -> Vec<Track> {
    let streams = Reflect::get(result, &JsValue::from_str("streams")).unwrap_or(JsValue::NULL);
    let array = Array::from(&streams);
    let mut tracks = Vec::new();

    for index in 0..array.length() {
        let stream = array.get(index);
        let kind = js_string(&stream, "kind");
        let stream_kind = match kind.as_str() {
            "Video" => StreamKind::Video,
            "Audio" => StreamKind::Audio,
            "Subtitle" => StreamKind::Subtitle,
            "Attachment" | "Data" => StreamKind::Attachment,
            _ => StreamKind::Video,
        };

        tracks.push(Track {
            id: index as usize + 1,
            source_index: js_number(&stream, "index").unwrap_or(index as usize),
            enabled: stream_kind != StreamKind::Attachment,
            kind: stream_kind,
            codec: joined_probe_codec(&stream),
            language: js_string(&stream, "language")
                .chars()
                .take(3)
                .collect::<String>()
                .to_ascii_lowercase(),
            title: js_string(&stream, "title"),
            choice: default_choice(stream_kind),
        });
    }

    tracks
}

fn merge_probe_tracks(existing: &[Track], probed: Vec<Track>) -> Vec<Track> {
    let mut used = vec![false; existing.len()];

    probed
        .into_iter()
        .map(|mut probed_track| {
            let exact_match = existing.iter().enumerate().position(|(index, track)| {
                !used[index]
                    && track.source_index == probed_track.source_index
                    && track.kind == probed_track.kind
            });

            let kind_match = exact_match.or_else(|| {
                existing
                    .iter()
                    .enumerate()
                    .position(|(index, track)| !used[index] && track.kind == probed_track.kind)
            });

            if let Some(existing_index) = kind_match {
                used[existing_index] = true;
                let existing_track = &existing[existing_index];
                probed_track.enabled = existing_track.enabled;
                probed_track.choice = existing_track.choice;
            }

            probed_track
        })
        .collect()
}

fn joined_probe_codec(stream: &JsValue) -> String {
    let codec = js_string(stream, "codec");
    let details = js_string(stream, "details");
    if details.is_empty() {
        codec
    } else {
        format!("{codec} ({details})")
    }
}

fn js_string(value: &JsValue, key: &str) -> String {
    Reflect::get(value, &JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

fn js_number(value: &JsValue, key: &str) -> Option<usize> {
    Reflect::get(value, &JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as usize)
}

async fn fetch_browser_encoders() -> Result<(Vec<String>, String), JsValue> {
    let result = get_ffmpeg_capabilities().await?;
    Ok((
        js_string_array(&result, "encoders"),
        js_string(&result, "log"),
    ))
}

fn js_string_array(value: &JsValue, key: &str) -> Vec<String> {
    let Ok(array_value) = Reflect::get(value, &JsValue::from_str(key)) else {
        return Vec::new();
    };
    if !Array::is_array(&array_value) {
        return Vec::new();
    }

    let array = Array::from(&array_value);
    (0..array.length())
        .filter_map(|index| array.get(index).as_string())
        .collect()
}

fn requested_encoders(args: &[String]) -> Vec<String> {
    let mut encoders = Vec::new();
    let mut index = 0;

    while index + 1 < args.len() {
        if is_codec_option(&args[index]) {
            let codec = args[index + 1].trim();
            if !codec.is_empty() && codec != "copy" {
                encoders.push(codec.to_owned());
            }
            index += 2;
        } else {
            index += 1;
        }
    }

    encoders
}

fn is_codec_option(arg: &str) -> bool {
    matches!(arg, "-c" | "-codec" | "-vcodec" | "-acodec" | "-scodec")
        || arg.starts_with("-c:")
        || arg.starts_with("-codec:")
}

fn missing_requested_encoders(args: &[String], browser_encoders: &[String]) -> Vec<String> {
    let available = browser_encoders
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut missing = Vec::new();

    for encoder in requested_encoders(args) {
        if !available.contains(encoder.as_str()) && !missing.contains(&encoder) {
            missing.push(encoder);
        }
    }

    missing
}

fn browser_codec_summary(encoders: &[String]) -> String {
    if encoders.is_empty() {
        return "No browser encoders detected yet.".into();
    }

    const HIGHLIGHTS: &[&str] = &[
        "libx264",
        "h264",
        "libx265",
        "hevc",
        "libvpx-vp9",
        "libaom-av1",
        "libsvtav1",
        "aac",
        "libopus",
        "libvorbis",
        "libmp3lame",
        "eac3",
        "flac",
        "mov_text",
        "srt",
        "webvtt",
        "png",
        "mjpeg",
        "gif",
    ];

    let available = encoders.iter().map(String::as_str).collect::<HashSet<_>>();
    let highlights = HIGHLIGHTS
        .iter()
        .filter(|encoder| available.contains(**encoder))
        .copied()
        .collect::<Vec<_>>();

    if highlights.is_empty() {
        format!(
            "Detected {} browser encoders. First available: {}",
            encoders.len(),
            encoders
                .iter()
                .take(16)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Detected {} browser encoders. Common available: {}",
            encoders.len(),
            highlights.join(", ")
        )
    }
}

fn default_choice(kind: StreamKind) -> CodecChoice {
    match kind {
        StreamKind::Video => CodecChoice::Av1Svt,
        StreamKind::Audio => CodecChoice::Eac3,
        StreamKind::Subtitle => CodecChoice::Copy,
        StreamKind::Attachment => CodecChoice::Copy,
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

fn update_track_kind(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let kind = match value.as_str() {
            "Video" => StreamKind::Video,
            "Audio" => StreamKind::Audio,
            "Subtitle" => StreamKind::Subtitle,
            "Attachment" => StreamKind::Attachment,
            _ => StreamKind::Video,
        };
        update_track(&state, file_id, track_id, |track| {
            track.kind = kind;
            if !codecs_for(kind).contains(&track.choice) {
                track.choice = default_choice(kind);
            }
        });
    })
}

fn update_track_codec(
    state: &UseStateHandle<AppState>,
    file_id: usize,
    track_id: usize,
    kind: StreamKind,
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let choice = codecs_for(kind)
            .iter()
            .copied()
            .find(|choice| choice.label() == value)
            .unwrap_or(CodecChoice::Copy);
        update_track(&state, file_id, track_id, |track| track.choice = choice);
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
            "Target VMAF" => QualityMode::Vmaf,
            _ => QualityMode::ConstantQuality,
        };
        state.set(next);
    })
}

fn update_av1an_select(
    state: &UseStateHandle<AppState>,
    update: fn(&mut Av1anSettings, String),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let mut next = (*state).clone();
        update(&mut next.av1an, value);
        state.set(next);
    })
}

fn update_av1an_number(
    state: &UseStateHandle<AppState>,
    update: fn(&mut Av1anSettings, u32),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event
            .target_unchecked_into::<HtmlInputElement>()
            .value()
            .parse()
            .unwrap_or_default();
        let mut next = (*state).clone();
        update(&mut next.av1an, value);
        state.set(next);
    })
}

fn update_av1an_bool(
    state: &UseStateHandle<AppState>,
    update: fn(&mut Av1anSettings, bool),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlInputElement>().checked();
        let mut next = (*state).clone();
        update(&mut next.av1an, value);
        state.set(next);
    })
}

fn update_av1an_text(
    state: &UseStateHandle<AppState>,
    update: fn(&mut Av1anSettings, String),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        let mut next = (*state).clone();
        update(&mut next.av1an, value);
        state.set(next);
    })
}

fn update_av1an_encoder(state: &UseStateHandle<AppState>) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let mut next = (*state).clone();
        next.av1an.encoder = [
            CodecChoice::Av1Svt,
            CodecChoice::Av1Aom,
            CodecChoice::Vp9,
            CodecChoice::H265X265,
        ]
        .into_iter()
        .find(|choice| choice.label() == value)
        .unwrap_or(CodecChoice::Av1Svt);
        state.set(next);
    })
}

fn codecs_for(kind: StreamKind) -> &'static [CodecChoice] {
    match kind {
        StreamKind::Video => CodecChoice::VIDEO,
        StreamKind::Audio => CodecChoice::AUDIO,
        StreamKind::Subtitle => CodecChoice::SUBTITLE,
        StreamKind::Attachment => &[CodecChoice::Copy, CodecChoice::Strip],
    }
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

fn js_error_text(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|value| value.as_string())
        })
        .unwrap_or_else(|| "unknown JavaScript error".into())
}
