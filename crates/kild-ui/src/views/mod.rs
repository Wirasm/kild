//! View components for kild-ui.
//!
//! This module contains the view layer of the application:
//! - `main_view` - Root view that composes header, list, and dialog
//! - `kild_list` - List of kilds with status indicators
//! - `create_dialog` - Modal dialog for creating new kilds
//! - `confirm_dialog` - Modal dialog for confirming destructive actions
//! - `add_project_dialog` - Modal dialog for adding new projects
//! - `project_selector` - Dropdown for switching between projects

pub mod add_project_dialog;
pub mod confirm_dialog;
pub mod create_dialog;
pub mod detail_panel;
pub mod kild_list;
pub mod main_view;
pub mod project_selector;

pub use main_view::MainView;
