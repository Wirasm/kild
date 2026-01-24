//! View components for shards-ui.
//!
//! This module contains the view layer of the application:
//! - `main_view` - Root view that composes header, list, and dialog
//! - `shard_list` - List of shards with status indicators
//! - `create_dialog` - Modal dialog for creating new shards
//! - `confirm_dialog` - Modal dialog for confirming destructive actions

pub mod confirm_dialog;
pub mod create_dialog;
pub mod main_view;
pub mod shard_list;

pub use main_view::MainView;
