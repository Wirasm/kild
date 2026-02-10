use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("PTY creation failed: {0}")]
    PtyCreation(String),
    #[error("PTY I/O error: {0}")]
    PtyIo(String),
}
