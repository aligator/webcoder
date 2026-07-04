//! The left navigation rail: brand, tab buttons, and the input counter.

use yew::prelude::*;

use super::types::Tab;

#[derive(Properties, PartialEq)]
pub(crate) struct SideRailProps {
    pub(crate) tab: Tab,
    pub(crate) on_select: Callback<Tab>,
    pub(crate) file_count: usize,
}

#[function_component(SideRail)]
pub(crate) fn side_rail(props: &SideRailProps) -> Html {
    html! {
        <aside class="side-rail">
            <div class="brand">
                <span class="brand-mark material-symbols-rounded">{"movie_filter"}</span>
                <div>
                    <strong>{"Webcoder"}</strong>
                    <small>{"FFmpeg Transcoder"}</small>
                </div>
            </div>
            <nav class="tab-list">
                { for Tab::ALL.iter().map(|(item, icon, label)| {
                    let item = *item;
                    let on_select = props.on_select.clone();
                    let active = props.tab == item;
                    html! {
                        <button
                            class={classes!("tab-button", active.then_some("active"))}
                            title={*label}
                            onclick={Callback::from(move |_| on_select.emit(item))}
                        >
                            <span class="material-symbols-rounded">{*icon}</span>
                            <b>{*label}</b>
                        </button>
                    }
                })}
            </nav>
            <div class="rail-status">
                <span>{props.file_count}</span>
                <small>{"inputs"}</small>
            </div>
        </aside>
    }
}
