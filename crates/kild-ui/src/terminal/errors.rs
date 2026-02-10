use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum TerminalError {
    #[error("Failed to open PTY: {message}")]
    PtyOpen { message: String },

    #[error("Failed to spawn shell '{shell}': {message}")]
    ShellSpawn { shell: String, message: String },

    #[error("PTY read failed")]
    PtyRead(#[source] std::io::Error),

    #[error("PTY write failed")]
    PtyWrite(#[source] std::io::Error),

    #[error("PTY flush failed")]
    PtyFlush(#[source] std::io::Error),

    #[error("Failed to acquire PTY writer lock: mutex poisoned")]
    WriterLockPoisoned,

    #[error("Channel send failed: {0}")]
    ChannelSend(String),

    #[error("Channels already taken (take_channels called more than once)")]
    ChannelsAlreadyTaken,
}

#[allow(dead_code)]
impl TerminalError {
    pub fn error_code(&self) -> &'static str {
        match self {
            TerminalError::PtyOpen { .. } => "terminal.pty_open_failed",
            TerminalError::ShellSpawn { .. } => "terminal.shell_spawn_failed",
            TerminalError::PtyRead(_) => "terminal.pty_read_failed",
            TerminalError::PtyWrite(_) => "terminal.pty_write_failed",
            TerminalError::PtyFlush(_) => "terminal.pty_flush_failed",
            TerminalError::WriterLockPoisoned => "terminal.writer_lock_poisoned",
            TerminalError::ChannelSend(_) => "terminal.channel_send_failed",
            TerminalError::ChannelsAlreadyTaken => "terminal.channels_already_taken",
        }
    }

    pub fn is_user_error(&self) -> bool {
        false
    }
}
