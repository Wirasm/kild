use thiserror::Error;

/// Errors from driving a `pi --mode rpc` subprocess.
#[derive(Debug, Error)]
pub enum RpcError {
    /// `pi` could not be spawned (not on `PATH`, not executable, …).
    #[error("failed to spawn `pi`: {0}")]
    Spawn(#[source] std::io::Error),

    /// I/O error writing to / reading from the child's pipes.
    #[error("pi stdin/stdout I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A command could not be serialized to JSON.
    #[error("failed to encode rpc command: {0}")]
    Encode(#[from] serde_json::Error),

    /// One of the child's stdio pipes (stdin/stdout/stderr) was not available.
    #[error("pi {0} pipe is unavailable")]
    PipeUnavailable(&'static str),
}
