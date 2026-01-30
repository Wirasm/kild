//! peek-core: Core library for native application inspection
//!
//! This library provides functionality for:
//! - Window enumeration and lookup
//! - Screenshot capture
//! - Image comparison
//! - UI state assertions
//!
//! Designed for AI-assisted development workflows where Claude Code needs
//! "eyes" on native UI applications.

pub mod assert;
pub mod diff;
pub mod element;
pub mod errors;
pub mod events;
pub mod interact;
pub mod logging;
pub mod screenshot;
pub mod window;

// Re-export commonly used types at the crate root
pub use errors::{PeekError, PeekResult};
pub use logging::init_logging;

// Re-export window types
pub use window::{MonitorInfo, WindowInfo};

// Re-export screenshot types
pub use screenshot::{CaptureRequest, CaptureResult, CaptureTarget, ImageFormat};

// Re-export diff types
pub use diff::{DiffRequest, DiffResult};

// Re-export assert types
pub use assert::{Assertion, AssertionResult, ElementQuery};

// Re-export interact types
pub use interact::{
    ClickRequest, ClickTextRequest, InteractionResult, InteractionTarget, KeyComboRequest,
    TypeRequest,
};

// Re-export element types
pub use element::{ElementError, ElementInfo, ElementsRequest, ElementsResult, FindRequest};
