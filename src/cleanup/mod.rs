pub mod errors;
pub mod handler;
mod operations;
pub mod types;

// Public API exports
pub use errors::CleanupError;
pub use handler::{
    cleanup_all, cleanup_all_with_strategy, cleanup_orphaned_resources, 
    scan_for_orphans, scan_for_orphans_with_strategy,
};
pub use types::{CleanupStrategy, CleanupSummary, OrphanedResource, ResourceType};
