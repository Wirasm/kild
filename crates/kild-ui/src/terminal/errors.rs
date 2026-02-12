use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
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

    #[error("Daemon connection failed: {message}")]
    DaemonConnect { message: String },

    #[error("Daemon attach failed: {message}")]
    DaemonAttach { message: String },

    #[error("Daemon protocol error: {message}")]
    DaemonProtocol { message: String },

    #[error("Base64 decode failed")]
    Base64Decode(#[from] base64::DecodeError),
}
