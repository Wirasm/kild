//! View components for kild-ui.
//!
//! This module contains the view layer of the application:
//! - `main_view` - Root view that composes header, list, and dialog
//! - `dashboard_view` - Fleet overview with kild cards
//! - `detail_view` - Kild drill-down from dashboard
//! - `kild_list` - List of kilds with status indicators
//! - `detail_panel` - Right panel showing selected kild details
//! - `create_dialog` - Modal dialog for creating new kilds
//! - `confirm_dialog` - Modal dialog for confirming destructive actions
//! - `add_project_dialog` - Modal dialog for adding new projects
//! - `sidebar` - Fixed left sidebar for project navigation
//! - `helpers` - Shared view utilities (time formatting, etc.)

pub mod add_project_dialog;
pub mod confirm_dialog;
pub mod create_dialog;
pub mod dashboard_view;
pub mod detail_panel;
pub mod detail_view;
pub mod helpers;
pub mod kild_list;
pub mod main_view;
pub mod pane_grid;
pub mod project_rail;
pub mod sidebar;
pub mod terminal_tabs;

pub use main_view::MainView;
