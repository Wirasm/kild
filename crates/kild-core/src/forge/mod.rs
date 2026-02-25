pub mod backends;
pub mod errors;
pub mod registry;
pub mod traits;
pub mod types;

// Re-export public API
pub use errors::ForgeError;
pub use registry::{detect_forge, get_backend, get_forge_backend};
pub use traits::ForgeBackend;
pub use types::{ForgeType, MergeReadiness};
