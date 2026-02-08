pub mod backends;
pub mod errors;
pub mod registry;
pub mod traits;
pub mod types;

// Re-export public API
pub use errors::EditorError;
pub use registry::{detect_editor, get_backend, open_editor};
pub use traits::EditorBackend;
pub use types::EditorType;
