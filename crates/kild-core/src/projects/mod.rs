pub mod errors;
pub mod manager;
pub mod persistence;
pub mod types;

// Re-export commonly used types at module level
pub use errors::ProjectError;
pub use manager::ProjectManager;
pub use persistence::{load_projects, migrate_projects_to_canonical, save_projects};
pub use types::{Project, ProjectsData};

// Re-export project ID generation from git module to eliminate duplication
pub use crate::git::operations::generate_project_id;
