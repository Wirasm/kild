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

    #[error("Working directory not accessible '{path}': {message}")]
    InvalidCwd { path: String, message: String },

    #[allow(dead_code)]
    #[error("Daemon connection failed: {message}")]
    DaemonConnect { message: String },

    #[allow(dead_code)]
    #[error("Daemon attach failed: {message}")]
    DaemonAttach { message: String },

    #[allow(dead_code)]
    #[error("Daemon protocol error: {message}")]
    DaemonProtocol { message: String },

    #[error("Base64 decode failed")]
    Base64Decode(#[from] base64::DecodeError),
}
