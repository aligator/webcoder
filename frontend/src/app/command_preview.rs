//! The collapsible command-preview pane: the assembled FFmpeg command line, a
//! copy button, and a summary of the selected input/output.

use yew::prelude::*;

use crate::core::{AppState, command_preview, format_size};

use super::state::AppCtx;
use super::widgets::{copy_to_clipboard, icon};

#[function_component(CommandPreview)]
pub(crate) fn command_preview_view() -> Html {
    let state = use_context::<AppCtx>().expect("AppCtx not found");
    // `copied` only matters to this pane, so it stays local rather than living
    // in the app-wide store.
    let copied = use_state(|| false);

    let active_command = command_preview(&state);

    let on_copy = {
        let active_command = active_command.clone();
        let copied = copied.clone();
        Callback::from(move |_| {
            copy_to_clipboard(&active_command);
            copied.set(true);
        })
    };

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
}

fn selected_summary(state: &AppState) -> Html {
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
