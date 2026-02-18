//! Main view for kild-ui.
//!
//! Root view that composes header, kild list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

mod dialog_handlers;
pub(crate) mod keybindings;
mod kild_handlers;
mod main_view_def;
mod navigation;
mod pane_grid_handlers;
mod path_utils;
mod project_handlers;
mod rendering;
mod tab_rename;
mod terminal_handlers;
mod types;

#[cfg(test)]
mod tests;

pub use main_view_def::MainView;
pub(crate) use types::ActiveView;
