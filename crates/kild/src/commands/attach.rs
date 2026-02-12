use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use clap::ArgMatches;
use nix::sys::termios;
use tracing::{error, info, warn};

use kild_core::events;

pub(crate) fn handle_attach_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.attach_started", branch = branch);

    // 1. Look up session to get daemon_session_id
    let session = match kild_core::session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("No kild found: {}", branch);
            eprintln!("  Use 'kild list' to see active sessions.");
            error!(event = "cli.attach_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    let daemon_session_id = match session.latest_agent().and_then(|a| a.daemon_session_id()) {
        Some(id) => id.to_string(),
        None => {
            let msg = format!(
                "'{}' is not daemon-managed. Use 'kild focus {}' for terminal sessions.",
                branch, branch
            );
            eprintln!("{}", msg);
            error!(
                event = "cli.attach_failed",
                branch = branch,
                error = msg.as_str()
            );
            return Err(msg.into());
        }
    };

    info!(
        event = "cli.attach_connecting",
        branch = branch,
        daemon_session_id = daemon_session_id.as_str()
    );

    // 2. Connect to daemon and attach
    if let Err(e) = attach_to_daemon_session(&daemon_session_id, branch) {
        eprintln!("{}", e);
        error!(event = "cli.attach_failed", branch = branch, error = %e);
        return Err(e);
    }

    info!(event = "cli.attach_completed", branch = branch);
    Ok(())
}

fn attach_to_daemon_session(
    daemon_session_id: &str,
    branch: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = kild_core::daemon::socket_path();
    let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
        format!(
            "Cannot connect to daemon at {}: {}\nStart the daemon: kild daemon start",
            socket_path.display(),
            e
        )
    })?;

    // Get terminal size
    let (cols, rows) = terminal_size();

    // Send attach request
    let attach_msg = serde_json::json!({
        "id": "attach-1",
        "type": "attach",
        "session_id": daemon_session_id,
        "cols": cols,
        "rows": rows,
    });
    writeln!(stream, "{}", serde_json::to_string(&attach_msg)?)?;
    stream.flush()?;

    // Read ack response
    let mut reader = std::io::BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut line)?;

    let ack: serde_json::Value = serde_json::from_str(line.trim())?;
    if ack.get("type").and_then(|t| t.as_str()) == Some("error") {
        let msg = match ack.get("message").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            None => {
                error!(event = "cli.attach.malformed_error_response", response = %ack);
                "Unknown error (daemon returned error with no message)".to_string()
            }
        };
        return Err(format!("Attach failed: {}", msg).into());
    }

    // Enter raw terminal mode
    let _raw_guard = enable_raw_mode()?;

    // Spawn stdin reader thread (owned String for 'static lifetime)
    let session_id_owned = daemon_session_id.to_string();
    let mut write_stream = stream.try_clone()?;
    let stdin_handle = std::thread::spawn(move || {
        forward_stdin_to_daemon(&mut write_stream, &session_id_owned);
    });

    // Main thread: read daemon output, write to stdout
    // Re-use the BufReader directly so we don't lose buffered data
    forward_daemon_to_stdout_buffered(reader)?;

    // Restore terminal
    drop(_raw_guard);
    eprintln!("\r\nDetached. Reconnect: kild attach {}", branch);

    if let Err(e) = stdin_handle.join() {
        error!(event = "cli.attach.stdin_thread_panicked", error = ?e);
    }
    Ok(())
}

fn terminal_size() -> (u16, u16) {
    use nix::libc;
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0 {
            (winsize.ws_col, winsize.ws_row)
        } else {
            (80, 24)
        }
    }
}

struct RawModeGuard {
    original: termios::Termios,
}

fn enable_raw_mode() -> Result<RawModeGuard, Box<dyn std::error::Error>> {
    use std::os::fd::BorrowedFd;

    let stdin_fd = unsafe { BorrowedFd::borrow_raw(0) };
    let original = termios::tcgetattr(stdin_fd).map_err(|e| format!("tcgetattr failed: {}", e))?;

    let mut raw = original.clone();
    termios::cfmakeraw(&mut raw);
    // Re-enable ISIG so Ctrl+C generates SIGINT and kills the attach process.
    // This lets the user detach with Ctrl+C — the daemon keeps the session alive.
    raw.local_flags.insert(termios::LocalFlags::ISIG);
    termios::tcsetattr(stdin_fd, termios::SetArg::TCSANOW, &raw)
        .map_err(|e| format!("tcsetattr failed: {}", e))?;

    Ok(RawModeGuard { original })
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        use std::os::fd::BorrowedFd;
        let stdin_fd = unsafe { BorrowedFd::borrow_raw(0) };
        let _ = termios::tcsetattr(stdin_fd, termios::SetArg::TCSANOW, &self.original);
    }
}

/// Forwards stdin bytes to the daemon over IPC, base64-encoded.
/// Ctrl+C (0x03) detaches from the session without killing it.
/// The shell stays alive in the daemon — reattach with `kild attach`.
fn forward_stdin_to_daemon(stream: &mut UnixStream, session_id: &str) {
    use base64::Engine;

    let stdin = std::io::stdin();
    let mut buf = [0u8; 4096];

    loop {
        let n = match stdin.lock().read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                error!(event = "cli.attach.stdin_read_failed", error = %e);
                eprintln!("\r\nStdin read failed. Detaching.");
                break;
            }
        };

        let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
        let input_msg = serde_json::json!({
            "id": format!("write-{}", n),
            "type": "write_stdin",
            "session_id": session_id,
            "data": encoded,
        });
        let serialized = match serde_json::to_string(&input_msg) {
            Ok(s) => s,
            Err(e) => {
                error!(event = "cli.attach.stdin_serialize_failed", error = %e, session_id = %session_id);
                eprintln!("\r\nInput encoding failed. Detaching.");
                break;
            }
        };
        if let Err(e) = writeln!(stream, "{}", serialized) {
            error!(event = "cli.attach.stdin_write_failed", error = %e, session_id = %session_id);
            eprintln!("\r\nConnection to daemon lost. Detaching.");
            break;
        }
        if let Err(e) = stream.flush() {
            error!(event = "cli.attach.stdin_flush_failed", error = %e);
            eprintln!("\r\nConnection to daemon lost. Detaching.");
            break;
        }
    }
}

fn forward_daemon_to_stdout_buffered(
    mut reader: std::io::BufReader<UnixStream>,
) -> Result<(), Box<dyn std::error::Error>> {
    use base64::Engine;

    let mut line = String::new();
    let mut stdout = std::io::stdout();

    loop {
        line.clear();
        let n = std::io::BufRead::read_line(&mut reader, &mut line)?;
        if n == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                error!(event = "cli.attach.parse_failed", error = %e);
                eprintln!(
                    "\r\nMalformed daemon message. Try: kild daemon stop && kild daemon start"
                );
                continue;
            }
        };

        match msg.get("type").and_then(|t| t.as_str()) {
            Some("pty_output") => {
                if let Some(data) = msg.get("data").and_then(|d| d.as_str())
                    && let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(data)
                {
                    stdout.write_all(&decoded)?;
                    stdout.flush()?;
                }
            }
            Some("pty_output_dropped") => {}

            Some("session_event") => {
                if let Some(event) = msg.get("event").and_then(|e| e.as_str()) {
                    match event {
                        "stopped" => {
                            eprintln!("\r\nSession process exited.");
                            break;
                        }
                        "resize_failed" => {
                            let detail = match msg
                                .get("details")
                                .and_then(|d| d.get("message"))
                                .and_then(|m| m.as_str())
                            {
                                Some(m) => m.to_string(),
                                None => {
                                    warn!(event = "cli.attach.malformed_resize_warning", response = %msg);
                                    "Terminal resize failed. Display may be garbled.".to_string()
                                }
                            };
                            eprintln!("\r\n{}", detail);
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                // Ignore other messages (ack, etc.)
            }
        }
    }

    Ok(())
}
