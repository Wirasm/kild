use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("Failed to open PTY: {message}")]
    PtyOpen { message: String },

    #[error("Failed to spawn shell '{shell}': {message}")]
    ShellSpawn { shell: String, message: String },

    #[error("PTY write failed")]
    PtyWrite(#[source] std::io::Error),

    #[error("PTY flush failed")]
    PtyFlush(#[source] std::io::Error),

    #[error("Failed to acquire PTY writer lock: mutex poisoned")]
    WriterLockPoisoned,

    #[error("Channels already taken (take_channels called more than once)")]
    ChannelsAlreadyTaken,

    #[error("PTY resize failed: {message}")]
    PtyResize { message: String },
}
