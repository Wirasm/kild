use thiserror::Error;

/// Errors from reading agent definitions.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not resolve kild home ($KILD_HOME and $HOME both unset)")]
    NoHome,
}
