//! The single shared store: [`AppState`] wrapped as a Yew [`Reducible`], driven
//! by explicit [`AppAction`]s. Every mutation of files/tracks/settings goes
//! through [`AppState::reduce`], so the mutation logic lives in one place and
//! components only dispatch named actions.
//!
//! The handle is handed down the tree via [`ContextProvider`], so components
//! read and mutate the store through [`use_context`] rather than threading a
//! state handle through every function argument.

use std::rc::Rc;

use yew::Reducible;
use yew::prelude::UseReducerHandle;

use crate::core::{AppState, ConvertSettings, Track, TrackOutput};

/// Context handle to the shared store. Obtain it in a component with
/// `use_context::<AppCtx>()`.
pub(crate) type AppCtx = UseReducerHandle<AppState>;

/// A single-field edit to one [`Track`], mapped from a DOM event by the view.
#[derive(Clone)]
pub enum TrackPatch {
    Enabled(bool),
    Language(String),
    Title(String),
    Choice(TrackOutput),
}

/// Every way the shared store can change. Dispatched by the views; applied
/// centrally in [`AppState::reduce`].
#[derive(Clone)]
pub enum AppAction {
    /// Replace the whole state (used by the async ingest paths, which rebuild
    /// the file list off a captured snapshot).
    Replace(AppState),
    SelectFile(usize),
    RemoveFile(usize),
    MoveSelectedFile(isize),
    SortFiles,
    SetTracksChecked { file_id: usize, checked: bool },
    SortTracks(usize),
    MoveTrack {
        file_id: usize,
        track_id: usize,
        direction: isize,
    },
    PatchTrack {
        file_id: usize,
        track_id: usize,
        patch: TrackPatch,
    },
    SetConvert(ConvertSettings),
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let mut next = (*self).clone();
        match action {
            AppAction::Replace(state) => return Rc::new(state),
            AppAction::SelectFile(file_id) => next.selected_file = Some(file_id),
            AppAction::RemoveFile(file_id) => {
                next.files.retain(|file| file.id != file_id);
                if next.selected_file == Some(file_id) {
                    next.selected_file = next.files.first().map(|file| file.id);
                }
            }
            AppAction::MoveSelectedFile(direction) => {
                if let Some(file_id) = next.selected_file {
                    if let Some(index) = next.files.iter().position(|file| file.id == file_id) {
                        let target =
                            (index as isize + direction).clamp(0, next.files.len() as isize - 1);
                        next.files.swap(index, target as usize);
                    }
                }
            }
            AppAction::SortFiles => next
                .files
                .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase())),
            AppAction::SetTracksChecked { file_id, checked } => {
                if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
                    for track in &mut file.tracks {
                        track.enabled = checked;
                    }
                }
            }
            AppAction::SortTracks(file_id) => {
                if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
                    file.tracks
                        .sort_by_key(|track| (track.kind.label().to_owned(), track.language.clone()));
                }
            }
            AppAction::MoveTrack {
                file_id,
                track_id,
                direction,
            } => {
                if let Some(file) = next.files.iter_mut().find(|file| file.id == file_id) {
                    if let Some(index) = file.tracks.iter().position(|track| track.id == track_id) {
                        let target =
                            (index as isize + direction).clamp(0, file.tracks.len() as isize - 1);
                        file.tracks.swap(index, target as usize);
                    }
                }
            }
            AppAction::PatchTrack {
                file_id,
                track_id,
                patch,
            } => {
                if let Some(track) = next
                    .files
                    .iter_mut()
                    .find(|file| file.id == file_id)
                    .and_then(|file| file.tracks.iter_mut().find(|track| track.id == track_id))
                {
                    apply_track_patch(track, patch);
                }
            }
            AppAction::SetConvert(settings) => next.convert = settings,
        }
        Rc::new(next)
    }
}

fn apply_track_patch(track: &mut Track, patch: TrackPatch) {
    match patch {
        TrackPatch::Enabled(value) => track.enabled = value,
        TrackPatch::Language(value) => track.language = value,
        TrackPatch::Title(value) => track.title = value,
        TrackPatch::Choice(value) => track.choice = value,
    }
}
