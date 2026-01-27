//! Reusable UI components for kild-ui.
//!
//! This module contains extracted, styled components that ensure
//! visual consistency across the application.

mod button;
mod status_indicator;
mod text_input;

pub use button::{Button, ButtonVariant};
#[allow(unused_imports)]
pub use status_indicator::{Status, StatusIndicator, StatusMode};

// Allow unused_imports - TextInput is defined ahead of usage in create_dialog.rs.
// Remove this attribute once Phase 9.6 integrates this component.
#[allow(unused_imports)]
pub use text_input::TextInput;
