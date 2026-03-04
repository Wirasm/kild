/// Errors produced by the kild-acp crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AcpError {
    #[error("failed to spawn ACP agent process: {message}")]
    SpawnFailed { message: String },

    #[error("ACP runtime thread failed: {message}")]
    RuntimeFailed { message: String },

    #[error("ACP command channel closed")]
    ChannelClosed,

    #[error("ACP agent process exited with code {code:?}")]
    ProcessExited { code: Option<i32> },

    #[error("ACP protocol error: {message}")]
    ProtocolError { message: String },

    #[error("ACP initialization failed: {message}")]
    InitFailed { message: String },
}
