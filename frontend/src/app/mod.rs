//! Root of the Yew UI: the `App` component owns the shared store (via
//! [`use_reducer`]) and the app-wide session state (encoder list, job-id map,
//! transient toast), hands the store down through a [`ContextProvider`], and
//! composes the side rail, command-preview pane, and the three tab views.
//!
//! The view logic is split by concern into submodules:
//! - [`state`] — the [`AppCtx`] store: `AppState` as a reducer plus its actions.
//! - [`bridge`] — the `assets/api.js` FFI plus JSON decode helpers.
//! - [`types`] — bridge response shapes and the navigation enum.
//! - [`widgets`] — small reusable form fields and presentational helpers.
//! - [`ingest`] — probing native file paths with the backend FFmpeg.
//! - [`side_rail`], [`command_preview`] — the rail and preview pane.
//! - [`media`], [`convert`], [`queue`] — the three tab views.

mod bridge;
mod command_preview;
mod convert;
mod ingest;
mod media;
mod queue;
mod side_rail;
mod state;
mod types;
mod widgets;

use std::collections::HashMap;

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::core::AppState;

use bridge::{get_encoders, js_error_text, listen_native_drop, parse_json};
use command_preview::CommandPreview;
use convert::ConvertTab;
use ingest::ingest_native_paths;
use media::MediaTab;
use queue::QueueTab;
use side_rail::SideRail;
use state::AppCtx;
use types::{ApiEncoder, BrowserEncoder, Tab, kind_from_str};

#[function_component(App)]
pub fn app() -> Html {
    let state = use_reducer(AppState::default);
    let tab = use_state(|| Tab::Media);
    // Transient snackbar message; auto-clears a few seconds after being set.
    let toast = use_state(|| Option::<String>::None);
    {
        let toast = toast.clone();
        use_effect_with((*toast).is_some(), move |shown| {
            let handle = if *shown {
                web_sys::window().and_then(|win| {
                    let toast = toast.clone();
                    let cb = Closure::once_into_js(move || toast.set(None));
                    win.set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.unchecked_ref(),
                        3000,
                    )
                    .ok()
                })
            } else {
                None
            };
            move || {
                if let (Some(id), Some(win)) = (handle, web_sys::window()) {
                    win.clear_timeout_with_handle(id);
                }
            }
        });
    }
    let show_command_preview = use_state(|| false);
    let job_log = use_state(|| "FFmpeg runtime idle.".to_owned());
    let browser_encoders = use_state(Vec::<BrowserEncoder>::new);
    // Maps a local file id to the server-side job id returned by the probe
    // upload, so the encode request can reference the already-uploaded input.
    // Shared by the Media tab (writes) and the Queue tab (reads).
    let job_ids = use_mut_ref(HashMap::<usize, String>::new);

    // Mirror of the current store, kept fresh every render, so the long-lived
    // native drag-drop listener can read the latest file list instead of the
    // (frozen) value captured when it was registered.
    let latest_state = use_mut_ref(AppState::default);
    *latest_state.borrow_mut() = (*state).clone();

    // Register the Tauri OS drag-drop listener once. The webview swallows HTML5
    // DnD, so OS drops arrive here as a forwarded app event carrying paths.
    {
        let state = state.clone();
        let job_ids = job_ids.clone();
        let job_log = job_log.clone();
        let latest_state = latest_state.clone();
        use_effect_with((), move |_| {
            let closure = Closure::wrap(Box::new(move |value: JsValue| {
                let paths = js_sys::Array::from(&value)
                    .iter()
                    .filter_map(|entry| entry.as_string())
                    .collect::<Vec<_>>();
                let base = latest_state.borrow().clone();
                ingest_native_paths(paths, base, state.clone(), job_ids.clone(), job_log.clone());
            }) as Box<dyn FnMut(JsValue)>);
            listen_native_drop(closure.as_ref().unchecked_ref());
            move || drop(closure)
        });
    }

    // Fetch the encoders the backend FFmpeg supports once on load so every
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

    let on_select_tab = {
        let tab = tab.clone();
        Callback::from(move |next: Tab| tab.set(next))
    };
    let on_toast = {
        let toast = toast.clone();
        Callback::from(move |message: String| toast.set(Some(message)))
    };
    let toggle_command_preview = {
        let show_command_preview = show_command_preview.clone();
        Callback::from(move |_| show_command_preview.set(!*show_command_preview))
    };

    html! {
        <ContextProvider<AppCtx> context={state.clone()}>
            <main class="app-shell">
                <SideRail tab={*tab} on_select={on_select_tab} file_count={state.files.len()} />

                <section class="workspace">
                    <header class="topbar">
                        <div>
                            <h1>{tab.title()}</h1>
                            <p>{tab.subtitle()}</p>
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
                                Tab::Media => html! {
                                    <MediaTab
                                        job_ids={job_ids.clone()}
                                        job_log={job_log.clone()}
                                        encoders={(*browser_encoders).clone()}
                                    />
                                },
                                Tab::Convert => html! { <ConvertTab /> },
                                Tab::Queue => html! {
                                    <QueueTab job_ids={job_ids.clone()} on_toast={on_toast.clone()} />
                                },
                            }}
                        </div>
                        {
                            if *show_command_preview {
                                html! { <CommandPreview /> }
                            } else {
                                Html::default()
                            }
                        }
                    </section>
                </section>
                {
                    if let Some(message) = (*toast).clone() {
                        html! { <div class="snackbar" role="status">{ message }</div> }
                    } else {
                        Html::default()
                    }
                }
            </main>
        </ContextProvider<AppCtx>>
    }
}
