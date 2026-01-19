pub mod errors;
pub mod handler;
mod operations;
pub mod types;

// Public API exports
pub use errors::CleanupError;
pub use handler::{
    cleanup_all, cleanup_orphaned_resources, scan_for_orphans,
};
pub use types::{CleanupStrategy, CleanupSummary, OrphanedResource, ResourceType};
