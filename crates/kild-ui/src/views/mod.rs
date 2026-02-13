//! View components for kild-ui.
//!
//! This module contains the view layer of the application:
//! - `main_view` - Root view that composes header, sidebar, and dialogs
//! - `dashboard_view` - Fleet overview with kild cards
//! - `detail_view` - Kild drill-down from dashboard
//! - `status_bar` - Contextual alerts and keyboard shortcut hints
//! - `create_dialog` - Modal dialog for creating new kilds
//! - `confirm_dialog` - Modal dialog for confirming destructive actions
//! - `add_project_dialog` - Modal dialog for adding new projects
//! - `sidebar` - Fixed left sidebar for kild navigation
//! - `project_rail` - Leftmost project switcher column
//! - `pane_grid` - 2x2 terminal pane grid for Control view
//! - `terminal_tabs` - Multi-terminal tab management
//! - `helpers` - Shared view utilities (time formatting, etc.)

pub mod add_project_dialog;
pub mod confirm_dialog;
pub mod create_dialog;
pub mod dashboard_view;
pub mod detail_view;
pub mod helpers;
pub mod main_view;
pub mod pane_grid;
pub mod project_rail;
pub mod sidebar;
pub mod status_bar;
pub mod terminal_tabs;

pub use main_view::MainView;
