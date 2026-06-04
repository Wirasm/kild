use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use super::rpc_errors::RpcError;
use super::rpc_types::{PiOutput, RpcCommand};

/// How to launch the `pi --mode rpc` subprocess.
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    /// Working directory (the agent's worktree). `None` inherits the current dir.
    pub cwd: Option<PathBuf>,
    /// `--model <pattern>`. `None` uses pi's configured default.
    pub model: Option<String>,
    /// `--provider <name>`. `None` uses pi's configured default.
    pub provider: Option<String>,
}

/// A live `pi --mode rpc` session: write commands, read structured events.
///
/// One session per agent; a worktree (the `cwd`) may host several. Closing the
/// session (drop, or [`shutdown`]) closes stdin, which makes `pi` exit.
///
/// [`shutdown`]: PiRpcSession::shutdown
pub struct PiRpcSession {
    child: Child,
    stdin: ChildStdin,
    events: mpsc::Receiver<PiOutput>,
}

impl PiRpcSession {
    /// Spawn `pi --mode rpc --no-session` and start streaming its events.
    ///
    /// A background task reads pi's stdout line-by-line, parses each JSON line into
    /// a [`PiOutput`], and forwards it over an internal channel. A second task
    /// drains stderr so pi never blocks on a full pipe.
    pub fn spawn(opts: SpawnOptions) -> Result<Self, RpcError> {
        let mut cmd = Command::new("pi");
        cmd.arg("--mode").arg("rpc").arg("--no-session");
        if let Some(model) = &opts.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(provider) = &opts.provider {
            cmd.arg("--provider").arg(provider);
        }
        if let Some(cwd) = &opts.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(RpcError::Spawn)?;
        let stdin = child.stdin.take().ok_or(RpcError::StdinClosed)?;
        let stdout = child.stdout.take().ok_or(RpcError::StdinClosed)?;
        let stderr = child.stderr.take().ok_or(RpcError::StdinClosed)?;

        // Reader task: JSONL stdout -> parsed events. pi's RPC mode is strict LF
        // framing; tokio's `lines()` splits only on `\n` and strips a trailing
        // `\r`, which is exactly compliant (unlike Node `readline`, which also
        // splits on U+2028/U+2029).
        let (tx, rx) = mpsc::channel::<PiOutput>(256);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<PiOutput>(&line) {
                    Ok(event) => {
                        if tx.send(event).await.is_err() {
                            break; // receiver dropped — stop reading
                        }
                    }
                    Err(err) => {
                        // Malformed line: surface it, but keep streaming.
                        eprintln!("rpc: failed to parse pi event: {err}: {line}");
                    }
                }
            }
        });

        // Drain stderr; mirror to our stderr (dimmed) for visibility.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("\x1b[2m[pi] {line}\x1b[0m");
            }
        });

        Ok(Self {
            child,
            stdin,
            events: rx,
        })
    }

    /// Send a command to pi (one JSON line on stdin).
    pub async fn send(&mut self, command: &RpcCommand) -> Result<(), RpcError> {
        let mut line = serde_json::to_vec(command)?;
        line.push(b'\n');
        self.stdin.write_all(&line).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Await the next event from pi, or `None` once the stream ends.
    pub async fn next_event(&mut self) -> Option<PiOutput> {
        self.events.recv().await
    }

    /// Split into a write handle and the event receiver, so the send side and
    /// read side can be driven from separate tasks concurrently — e.g. a UI that
    /// streams events while still sending `prompt`/`steer` commands.
    pub fn split(self) -> (PiRpcWriter, mpsc::Receiver<PiOutput>) {
        (
            PiRpcWriter {
                child: self.child,
                stdin: self.stdin,
            },
            self.events,
        )
    }

    /// Close stdin and wait for pi to exit.
    pub async fn shutdown(self) -> Result<(), RpcError> {
        let PiRpcSession {
            mut child,
            stdin,
            mut events,
        } = self;
        // Closing stdin tells pi to exit; keep draining so the reader task
        // consumes pi's remaining stdout until EOF — otherwise pi can hit EPIPE
        // writing to a half-closed pipe.
        drop(stdin);
        while events.recv().await.is_some() {}
        child.wait().await?;
        Ok(())
    }
}

/// The write half of a [`PiRpcSession`] (see [`PiRpcSession::split`]) — owns the
/// child process and its stdin so commands can be sent while a separate task
/// drains events.
pub struct PiRpcWriter {
    child: Child,
    stdin: ChildStdin,
}

impl PiRpcWriter {
    /// Send a command to pi (one JSON line on stdin).
    pub async fn send(&mut self, command: &RpcCommand) -> Result<(), RpcError> {
        let mut line = serde_json::to_vec(command)?;
        line.push(b'\n');
        self.stdin.write_all(&line).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Close stdin and wait for pi to exit. The paired event receiver ends
    /// (`recv()` returns `None`) once pi's stdout closes.
    pub async fn shutdown(self) -> Result<(), RpcError> {
        drop(self.stdin);
        let mut child = self.child;
        child.wait().await?;
        Ok(())
    }
}
