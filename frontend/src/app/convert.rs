//! The Convert tab: the settings form (output, quality, video, audio,
//! subtitles, advanced, metadata). Every field edit dispatches a
//! [`AppAction::SetConvert`] with the updated settings.

use web_sys::{Event, HtmlInputElement, HtmlSelectElement, InputEvent};
use yew::TargetCast;
use yew::prelude::*;

use crate::core::{
    ALLOWED_CONTAINERS, CHAPTERS_COPY, CHAPTERS_STRIP, ConvertSettings, METADATA_COPY,
    METADATA_STRIP_ALL, METADATA_STRIP_KEEP_TRACKS, QualityMode,
};

use super::state::{AppAction, AppCtx};
use super::widgets::{icon, number_field, select_field, selected_option, text_field};

#[function_component(ConvertTab)]
pub(crate) fn convert_tab() -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    let settings = &state.convert;

    html! {
        <div class="settings-grid">
            <section class="settings-group">
                <div class="panel-title">{ icon("output") }<h2>{"Output"}</h2></div>
                { select_field("Container", settings.container.clone(), ALLOWED_CONTAINERS, update_convert_select(&state, |settings, value| settings.container = value)) }
                { select_field("Preset", settings.preset.clone(), &["ultrafast", "veryfast", "fast", "medium", "slow", "slower"], update_convert_select(&state, |settings, value| settings.preset = value)) }
                { select_field("Color Format", settings.color_format.clone(), &["source", "yuv420p", "yuv420p10le", "yuv444p", "rgb24"], update_convert_select(&state, |settings, value| settings.color_format = value)) }
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("speed") }<h2>{"Quality"}</h2></div>
                <select
                    value={settings.quality_mode.label()}
                    onchange={update_quality_mode(&state)}
                >
                    { selected_option(QualityMode::ConstantQuality.label(), settings.quality_mode.label()) }
                    { selected_option(QualityMode::Bitrate.label(), settings.quality_mode.label()) }
                    { selected_option(QualityMode::FileSize.label(), settings.quality_mode.label()) }
                </select>
                { number_field("CRF / CQ", settings.quality_value, 1, 63, update_convert_number(&state, |settings, value| settings.quality_value = value)) }
                { number_field("Bitrate kbps", settings.bitrate_kbps, 64, 250000, update_convert_number(&state, |settings, value| settings.bitrate_kbps = value)) }
                { number_field("Target MB", settings.target_size_mb, 1, 500000, update_convert_number(&state, |settings, value| settings.target_size_mb = value)) }
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("movie") }<h2>{"Video"}</h2></div>
                <div class="three-col">
                    { text_field("FPS", settings.fps.clone(), update_convert_text(&state, |settings, value| settings.fps = value)) }
                    { text_field("Scale", settings.resize.clone(), update_convert_text(&state, |settings, value| settings.resize = value)) }
                    { select_field("Crop Mode", settings.crop_mode.clone(), &["Disable", "Manual"], update_convert_select(&state, |settings, value| settings.crop_mode = value)) }
                    { text_field("Crop", settings.crop.clone(), update_convert_text(&state, |settings, value| settings.crop = value)) }
                </div>
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("graphic_eq") }<h2>{"Audio"}</h2></div>
                { select_field("Channels", settings.audio_channels.clone(), &["source", "1", "2", "6", "8"], update_convert_select(&state, |settings, value| settings.audio_channels = value)) }
                { number_field("Stereo kbps", settings.audio_bitrate_kbps, 0, 6400, update_convert_number(&state, |settings, value| settings.audio_bitrate_kbps = value)) }
            </section>

            <section class="settings-group">
                <div class="panel-title">{ icon("subtitles") }<h2>{"Subtitles"}</h2></div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={settings.burn_subtitles}
                        onchange={update_convert_bool(&state, |settings, value| settings.burn_subtitles = value)}
                    />
                    <span>{"Burn in selected subtitle stream"}</span>
                </label>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("tune") }<h2>{"Advanced"}</h2></div>
                <div class="three-col">
                    { text_field("Trim Start", settings.trim_start.clone(), update_convert_text(&state, |settings, value| settings.trim_start = value)) }
                    { text_field("Trim End", settings.trim_end.clone(), update_convert_text(&state, |settings, value| settings.trim_end = value)) }
                    { text_field("Duration", settings.trim_duration.clone(), update_convert_text(&state, |settings, value| settings.trim_duration = value)) }
                </div>
            </section>

            <section class="settings-group wide">
                <div class="panel-title">{ icon("newspaper") }<h2>{"Metadata"}</h2></div>
                <div class="two-col">
                    { select_field("Metadata", settings.metadata_mode.clone(), &[METADATA_COPY, METADATA_STRIP_KEEP_TRACKS, METADATA_STRIP_ALL], update_convert_select(&state, |settings, value| settings.metadata_mode = value)) }
                    { select_field("Chapters", settings.chapter_mode.clone(), &[CHAPTERS_COPY, CHAPTERS_STRIP], update_convert_select(&state, |settings, value| settings.chapter_mode = value)) }
                </div>
                <label class="check-line">
                    <input
                        type="checkbox"
                        checked={settings.apply_track_metadata}
                        onchange={update_convert_bool(&state, |settings, value| settings.apply_track_metadata = value)}
                    />
                    <span>{"Apply track titles and languages from Track List"}</span>
                </label>
            </section>
        </div>
    }
}

/// Build a fresh `ConvertSettings` from the current store, apply `update`, and
/// dispatch it — the shared helper behind every field callback below.
fn dispatch_convert(state: &AppCtx, update: impl FnOnce(&mut ConvertSettings)) {
    let mut next = state.convert.clone();
    update(&mut next);
    state.dispatch(AppAction::SetConvert(next));
}

fn update_convert_text(
    state: &AppCtx,
    update: fn(&mut ConvertSettings, String),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        dispatch_convert(&state, |settings| update(settings, value));
    })
}

fn update_convert_select(
    state: &AppCtx,
    update: fn(&mut ConvertSettings, String),
) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        dispatch_convert(&state, |settings| update(settings, value));
    })
}

fn update_convert_number(
    state: &AppCtx,
    update: fn(&mut ConvertSettings, u32),
) -> Callback<InputEvent> {
    let state = state.clone();
    Callback::from(move |event: InputEvent| {
        let value = event
            .target_unchecked_into::<HtmlInputElement>()
            .value()
            .parse()
            .unwrap_or_default();
        dispatch_convert(&state, |settings| update(settings, value));
    })
}

fn update_convert_bool(state: &AppCtx, update: fn(&mut ConvertSettings, bool)) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlInputElement>().checked();
        dispatch_convert(&state, |settings| update(settings, value));
    })
}

fn update_quality_mode(state: &AppCtx) -> Callback<Event> {
    let state = state.clone();
    Callback::from(move |event: Event| {
        let value = event.target_unchecked_into::<HtmlSelectElement>().value();
        let mode = match value.as_str() {
            "Target bitrate" => QualityMode::Bitrate,
            "Target file size" => QualityMode::FileSize,
            _ => QualityMode::ConstantQuality,
        };
        dispatch_convert(&state, |settings| settings.quality_mode = mode);
    })
}
