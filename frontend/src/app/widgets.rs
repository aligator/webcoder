//! Small presentational helpers reused across the views: form fields, the
//! icon span, the page header text, and the clipboard shim.

use web_sys::InputEvent;
use yew::prelude::*;

pub(crate) fn selected_option(value: &str, selected: &str) -> Html {
    option_selected(value, value == selected)
}

pub(crate) fn option_selected(value: &str, selected: bool) -> Html {
    html! { <option value={value.to_owned()} selected={selected}>{value}</option> }
}

pub(crate) fn icon(name: &str) -> Html {
    html! { <span class="material-symbols-rounded">{name}</span> }
}

pub(crate) fn noop_select() -> Callback<Event> {
    Callback::from(|_| {})
}

pub(crate) fn text_field(label: &str, value: String, oninput: Callback<InputEvent>) -> Html {
    html! {
        <label>
            <span>{label}</span>
            <input value={value} oninput={oninput} />
        </label>
    }
}

pub(crate) fn number_field(
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

pub(crate) fn select_field(
    label: &str,
    value: String,
    values: &[&str],
    onchange: Callback<Event>,
) -> Html {
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

pub(crate) fn empty_panel(message: &str) -> Html {
    html! {
        <section class="empty-panel">
            <strong>{message}</strong>
        </section>
    }
}

pub(crate) fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
    }
}
