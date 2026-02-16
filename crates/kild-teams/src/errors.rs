//! Error types for kild-teams.

#[derive(Debug, thiserror::Error)]
pub enum TeamsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("teams error: {message}")]
    Teams { message: String },
}
