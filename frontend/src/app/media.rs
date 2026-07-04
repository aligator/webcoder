//! The Media tab: the two-column tracks/files layout, the drop zone and native
//! picker wiring, per-track editing, and the encoder dropdown.
//!
//! [`FileRow`] and [`TrackRow`] are real components keyed on their data, so a
//! row only re-renders when its own file/track changes.

use wasm_bindgen_futures::spawn_local;
use web_sys::{DragEvent, Event, HtmlInputElement, HtmlSelectElement, InputEvent};
use yew::TargetCast;
use yew::prelude::*;

use crate::core::{MediaFile, StreamKind, Track, TrackOutput, format_size};

use super::bridge::{js_error_text, native_app, parse_json, pick_native_files};
use super::ingest::{JobIds, ingest_native_paths, ingest_web_files};
use super::state::{AppAction, AppCtx, TrackPatch};
use super::types::BrowserEncoder;
use super::widgets::{empty_panel, icon, noop_select, option_selected};

/// Which media column is shown on very small screens, where the two-column
/// layout collapses to one column with a tab switch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MediaCol {
    Tracks,
    Files,
}

#[derive(Properties, PartialEq)]
pub(crate) struct MediaTabProps {
    pub(crate) job_ids: JobIds,
    pub(crate) job_log: UseStateHandle<String>,
    pub(crate) encoders: Vec<BrowserEncoder>,
}

#[function_component(MediaTab)]
pub(crate) fn media_tab(props: &MediaTabProps) -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    // Screen-size column toggle and drag hover are pure view state, so they
    // stay local instead of in the shared store.
    let media_col = use_state(|| MediaCol::Tracks);
    let dragging = use_state(|| false);

    let is_native = native_app();
    let job_ids = &props.job_ids;
    let job_log = &props.job_log;

    let on_files = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let job_log = job_log.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            if let Some(files) = input.files() {
                ingest_web_files(files, state.clone(), job_ids.clone(), job_log.clone());
            }
        })
    };
    let on_drop = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let job_log = job_log.clone();
        let dragging = dragging.clone();
        Callback::from(move |event: DragEvent| {
            event.prevent_default();
            dragging.set(false);
            if let Some(files) = event.data_transfer().and_then(|dt| dt.files()) {
                ingest_web_files(files, state.clone(), job_ids.clone(), job_log.clone());
            }
        })
    };
    let on_drag_over = Callback::from(|event: DragEvent| event.prevent_default());
    let on_drag_enter = {
        let dragging = dragging.clone();
        Callback::from(move |event: DragEvent| {
            event.prevent_default();
            dragging.set(true);
        })
    };
    let on_drag_leave = {
        let dragging = dragging.clone();
        Callback::from(move |event: DragEvent| {
            event.prevent_default();
            dragging.set(false);
        })
    };
    let on_native_files = {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let job_log = job_log.clone();
        Callback::from(move |_| {
            let state = state.clone();
            let job_ids = job_ids.clone();
            let job_log = job_log.clone();
            let base = (*state).clone();
            spawn_local(async move {
                match pick_native_files().await {
                    Ok(value) => match parse_json::<Vec<String>>(value) {
                        Ok(paths) => ingest_native_paths(paths, base, state, job_ids, job_log),
                        Err(error) => {
                            job_log.set(format!("Native picker parse failed: {error}"))
                        }
                    },
                    Err(error) => {
                        job_log.set(format!("Native picker failed: {}", js_error_text(error)))
                    }
                }
            });
        })
    };

    let show_tracks = *media_col == MediaCol::Tracks;
    let select_tracks = {
        let media_col = media_col.clone();
        Callback::from(move |_| media_col.set(MediaCol::Tracks))
    };
    let select_files = {
        let media_col = media_col.clone();
        Callback::from(move |_| media_col.set(MediaCol::Files))
    };

    let move_up = dispatch_click(&state, AppAction::MoveSelectedFile(-1));
    let move_down = dispatch_click(&state, AppAction::MoveSelectedFile(1));
    let sort = dispatch_click(&state, AppAction::SortFiles);

    html! {
        <div class={classes!("media-grid", if show_tracks { "show-tracks" } else { "show-files" })}>
            <div class="media-col-tabs">
                <button class={classes!("media-col-tab", show_tracks.then_some("active"))} onclick={select_tracks}>{"Tracks"}</button>
                <button class={classes!("media-col-tab", (!show_tracks).then_some("active"))} onclick={select_files}>{"Files"}</button>
            </div>
            <div class="media-main">
                { view_tracks(&state, &props.encoders) }
            </div>
            <aside class="media-files">
                <section
                    class={classes!("drop-zone", dragging.then_some("dragging"))}
                    ondragover={on_drag_over}
                    ondragenter={on_drag_enter}
                    ondragleave={on_drag_leave}
                    ondrop={on_drop}
                >
                    <span class="drop-icon material-symbols-rounded">{"cloud_upload"}</span>
                    <strong>{"Drop media here"}</strong>
                    <small>{
                        if is_native {
                            "Drag files in, or browse your disk."
                        } else {
                            "Drag files in, or browse to upload & probe."
                        }
                    }</small>
                    {
                        if is_native {
                            html! {
                                <button class="command-button accent drop-cta" type="button" onclick={on_native_files}>
                                    { icon("folder_open") }
                                    { "Browse files" }
                                </button>
                            }
                        } else {
                            html! {
                                <>
                                    <input id="file-picker" class="visually-hidden" type="file" multiple=true onchange={on_files} />
                                    <label class="command-button accent drop-cta" for="file-picker">
                                        { icon("folder_open") }
                                        { "Browse files" }
                                    </label>
                                </>
                            }
                        }
                    }
                </section>

                <div class="file-toolbar">
                    <button class="icon-button subtle material-symbols-rounded" title="Move selected file up" onclick={move_up}>{"keyboard_arrow_up"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Move selected file down" onclick={move_down}>{"keyboard_arrow_down"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Sort files by name" onclick={sort}>{"sort_by_alpha"}</button>
                </div>

                <section class="file-list">
                    { for state.files.iter().map(|file| html! {
                        <FileRow file={file.clone()} selected={state.selected_file == Some(file.id)} />
                    }) }
                </section>
            </aside>
        </div>
    }
}

/// A `Callback` that dispatches a fixed action, for the toolbar/header buttons.
fn dispatch_click(state: &AppCtx, action: AppAction) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| state.dispatch(action.clone()))
}

#[derive(Properties, PartialEq)]
struct FileRowProps {
    file: MediaFile,
    selected: bool,
}

#[function_component(FileRow)]
fn file_row(props: &FileRowProps) -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    let file = &props.file;
    let file_id = file.id;

    let select_file = {
        let state = state.clone();
        Callback::from(move |_| state.dispatch(AppAction::SelectFile(file_id)))
    };
    let remove_file = {
        let state = state.clone();
        Callback::from(move |_| state.dispatch(AppAction::RemoveFile(file_id)))
    };

    html! {
        <article class={classes!("file-row", props.selected.then_some("selected"))}>
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

fn view_tracks(state: &AppCtx, encoders: &[BrowserEncoder]) -> Html {
    let Some(file) = state.selected_file() else {
        return empty_panel("No selected input");
    };

    let file_id = file.id;
    let check_all = dispatch_click(state, AppAction::SetTracksChecked { file_id, checked: true });
    let check_none = dispatch_click(state, AppAction::SetTracksChecked { file_id, checked: false });
    let sort = dispatch_click(state, AppAction::SortTracks(file_id));

    html! {
        <div class="stack">
            <div class="section-head">
                <div>
                    <span>{"STREAMS"}</span>
                    <h2>{&file.name}</h2>
                </div>
                <div class="preview-actions">
                    <button class="icon-button subtle material-symbols-rounded" title="Check all" onclick={check_all}>{"select_check_box"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Check none" onclick={check_none}>{"disabled_by_default"}</button>
                    <button class="icon-button subtle material-symbols-rounded" title="Sort tracks" onclick={sort}>{"sort"}</button>
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
                { for file.tracks.iter().map(|track| html! {
                    <TrackRow file_id={file_id} track={track.clone()} encoders={encoders.to_vec()} />
                }) }
            </div>
            <textarea class="stream-details" readonly=true value={stream_details(file)} />
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct TrackRowProps {
    file_id: usize,
    track: Track,
    encoders: Vec<BrowserEncoder>,
}

#[function_component(TrackRow)]
fn track_row(props: &TrackRowProps) -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    let track = &props.track;
    let file_id = props.file_id;
    let track_id = track.id;

    let on_enabled = patch_bool(&state, file_id, track_id, TrackPatch::Enabled);
    let on_language = patch_text(&state, file_id, track_id, TrackPatch::Language);
    let on_title = patch_text(&state, file_id, track_id, TrackPatch::Title);
    let on_codec = {
        let state = state.clone();
        Callback::from(move |event: Event| {
            let value = event.target_unchecked_into::<HtmlSelectElement>().value();
            state.dispatch(AppAction::PatchTrack {
                file_id,
                track_id,
                patch: TrackPatch::Choice(TrackOutput::from_label(&value)),
            });
        })
    };
    let move_up = dispatch_click(
        &state,
        AppAction::MoveTrack {
            file_id,
            track_id,
            direction: -1,
        },
    );
    let move_down = dispatch_click(
        &state,
        AppAction::MoveTrack {
            file_id,
            track_id,
            direction: 1,
        },
    );

    html! {
        <div class="track-row">
            <input type="checkbox" checked={track.enabled} onchange={on_enabled} />
            <span class="track-fact">{track.kind.label()}</span>
            <span class="track-fact" title={track.codec.clone()}>{&track.codec}</span>
            <input value={track.language.clone()} oninput={on_language} />
            <input value={track.title.clone()} oninput={on_title} />
            <select value={track.choice.label().to_owned()} onchange={on_codec}>
                { encoder_options(&props.encoders, track.kind, &track.choice) }
            </select>
            <div class="row-actions">
                <button class="icon-button subtle material-symbols-rounded" title="Move up" onclick={move_up}>{"keyboard_arrow_up"}</button>
                <button class="icon-button subtle material-symbols-rounded" title="Move down" onclick={move_down}>{"keyboard_arrow_down"}</button>
            </div>
        </div>
    }
}

fn patch_text(
    state: &AppCtx,
    file_id: usize,
    track_id: usize,
    make: fn(String) -> TrackPatch,
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        state.dispatch(AppAction::PatchTrack {
            file_id,
            track_id,
            patch: make(value),
        });
    })
}

fn patch_bool(
    state: &AppCtx,
    file_id: usize,
    track_id: usize,
    make: fn(bool) -> TrackPatch,
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlInputElement>().checked();
        state.dispatch(AppAction::PatchTrack {
            file_id,
            track_id,
            patch: make(value),
        });
    })
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
