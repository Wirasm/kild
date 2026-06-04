use thiserror::Error;

/// Errors from the agent store.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent store I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not resolve kild home ($KILD_HOME and $HOME both unset)")]
    NoHome,

    #[error("agent name must not be empty")]
    EmptyName,

    #[error("'default' is a reserved agent name")]
    ReservedName,

    #[error("invalid agent name (no path separators or leading dot): {0}")]
    InvalidName(String),

    #[error("an agent named '{0}' already exists")]
    DuplicateName(String),
}
