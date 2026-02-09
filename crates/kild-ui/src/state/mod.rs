//! Application state for kild-ui.
//!
//! Centralized state management for the GUI. The main type is `AppState`,
//! which provides a facade over internal state modules. Use `AppState` methods
//! to interact with state; internal modules are implementation details.

pub mod app_state;
pub mod dialog;
pub mod errors;
pub mod loading;
pub mod selection;
pub mod sessions;

// Re-export all public types at module level so consumers use `crate::state::*`
pub use app_state::AppState;
pub use dialog::{CreateDialogField, DialogState};
pub use errors::OperationError;
