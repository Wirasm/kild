//! Reusable UI components for kild-ui.
//!
//! This module contains extracted, styled components that ensure
//! visual consistency across the application.

mod button;
mod status_indicator;

pub use button::{Button, ButtonVariant};
#[allow(unused_imports)]
pub use status_indicator::{Status, StatusIndicator, StatusMode};
