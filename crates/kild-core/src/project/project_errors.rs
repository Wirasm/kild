use thiserror::Error;

/// Errors from the project store.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project store I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("project store is corrupt: {0}")]
    Json(#[from] serde_json::Error),

    #[error("could not resolve home directory ($HOME unset)")]
    NoHome,

    #[error("path is not an existing directory: {0}")]
    NotADirectory(String),

    #[error("a project named '{0}' already exists")]
    DuplicateName(String),

    #[error("project name must not be empty")]
    EmptyName,
}
