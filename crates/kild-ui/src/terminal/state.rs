use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::Task;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use super::errors::TerminalError;

/// Simple size implementation satisfying alacritty_terminal's Dimensions trait.
struct TermDimensions {
    cols: usize,
    screen_lines: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Event listener that forwards alacritty_terminal events via an mpsc channel.
pub(crate) struct KildListener {
    sender: futures::channel::mpsc::UnboundedSender<AlacEvent>,
}

impl EventListener for KildListener {
    fn send_event(&self, event: AlacEvent) {
        if let Err(e) = self.sender.unbounded_send(event) {
            tracing::error!(event = "ui.terminal.event_send_failed", error = %e);
        }
    }
}

/// Core terminal state wrapping alacritty_terminal's Term with PTY lifecycle.
///
/// Manages:
/// - VT100 emulation via `alacritty_terminal::Term`
/// - PTY process (spawn, read, write)
///
/// After construction, call `take_channels()` to get the byte/event receivers
/// needed for event batching. The caller (TerminalView) owns the batching task
/// so it can notify GPUI to repaint after each batch.
pub struct Terminal {
    /// The terminal emulator state, protected by FairMutex to prevent
    /// lock starvation between the PTY reader and the GPUI renderer.
    term: Arc<FairMutex<Term<KildListener>>>,
    /// PTY stdin writer. Arc<Mutex<>> because take_writer() is one-shot.
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Background PTY reader task. Stored to prevent cancellation.
    _pty_reader_task: Task<()>,
    /// Child process handle. Stored for shutdown.
    _child: Box<dyn Child + Send + Sync>,
    /// Pending channels for event batching. Taken once by TerminalView.
    pending_byte_rx: Option<UnboundedReceiver<Vec<u8>>>,
    pending_event_rx: Option<UnboundedReceiver<AlacEvent>>,
    /// Shared error state set on critical failures (channel errors, write failures, lock poisoning).
    error_state: Arc<Mutex<Option<String>>>,
    /// Set to true when the batch loop exits (shell exited or PTY closed).
    exited: Arc<AtomicBool>,
}

impl Terminal {
    /// Create a new terminal with a live shell session.
    ///
    /// Spawns the user's default shell, starts a background reader task
    /// for PTY output, and sets up 4ms event batching.
    pub fn new(cx: &mut gpui::App) -> Result<Self, TerminalError> {
        let rows: u16 = 24;
        let cols: u16 = 80;

        // Create event channel for alacritty_terminal events
        let (event_tx, event_rx) = futures::channel::mpsc::unbounded();
        let listener = KildListener { sender: event_tx };

        // Create alacritty_terminal instance
        let config = TermConfig::default();
        let dims = TermDimensions {
            cols: cols as usize,
            screen_lines: rows as usize,
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &dims, listener)));

        // Create PTY
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let pty_system = native_pty_system();
        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = pty_system
            .openpty(pty_size)
            .map_err(|e| TerminalError::PtyOpen {
                message: e.to_string(),
            })?;

        let mut cmd = CommandBuilder::new(&shell);
        // Set TERM for proper escape sequence support
        cmd.env("TERM", "xterm-256color");
        // Set working directory to user's home
        if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TerminalError::ShellSpawn {
                shell: shell.clone(),
                message: e.to_string(),
            })?;
        // Drop our copy of the slave fd. The child process inherited it during
        // spawn, so it remains open there. If we kept ours, the kernel would never
        // deliver EOF on the master when the child exits (two open references).
        drop(pair.slave);

        tracing::info!(
            event = "ui.terminal.create_started",
            shell = shell,
            rows = rows,
            cols = cols,
        );

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerminalError::PtyOpen {
                message: format!("take_writer: {}", e),
            })?;
        let pty_writer = Arc::new(Mutex::new(writer));

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::PtyOpen {
                message: format!("clone_reader: {}", e),
            })?;

        // Keep master alive — dropping it would close the PTY
        let _pty_master = pair.master;

        // Spawn blocking PTY reader on a dedicated thread via std::thread.
        // GPUI's BackgroundExecutor is async/cooperative — blocking reads would
        // starve other tasks. Use a real OS thread instead.
        let (byte_tx, byte_rx) = futures::channel::mpsc::unbounded::<Vec<u8>>();
        let pty_reader_task = cx.background_executor().spawn(async move {
            // Move the blocking read loop to a dedicated OS thread
            let (done_tx, done_rx) = futures::channel::oneshot::channel::<()>();
            std::thread::spawn(move || {
                // Hold _pty_master in reader thread to keep PTY alive
                let _master = _pty_master;
                let mut reader = reader;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            tracing::info!(event = "ui.terminal.pty_eof");
                            break;
                        }
                        Ok(n) => {
                            if byte_tx.unbounded_send(buf[..n].to_vec()).is_err() {
                                tracing::error!(event = "ui.terminal.byte_send_failed", bytes = n);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!(event = "ui.terminal.pty_read_failed", error = %e);
                            break;
                        }
                    }
                }
                if done_tx.send(()).is_err() {
                    tracing::debug!(event = "ui.terminal.done_send_failed");
                }
            });
            if done_rx.await.is_err() {
                tracing::warn!(
                    event = "ui.terminal.done_recv_failed",
                    "PTY reader thread dropped done_tx without sending — thread may have panicked"
                );
            }
        });

        tracing::info!(event = "ui.terminal.create_completed");

        Ok(Self {
            term,
            pty_writer,
            _pty_reader_task: pty_reader_task,
            _child: child,
            pending_byte_rx: Some(byte_rx),
            pending_event_rx: Some(event_rx),
            error_state: Arc::new(Mutex::new(None)),
            exited: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Take the byte and event channels for event batching.
    ///
    /// Must be called exactly once after construction. The caller (TerminalView)
    /// uses these to run the batching loop where it can notify GPUI to repaint.
    pub fn take_channels(
        &mut self,
    ) -> Result<(UnboundedReceiver<Vec<u8>>, UnboundedReceiver<AlacEvent>), TerminalError> {
        let byte_rx = self.pending_byte_rx.take().ok_or_else(|| {
            tracing::error!(event = "ui.terminal.take_channels_double_call");
            TerminalError::ChannelsAlreadyTaken
        })?;
        let event_rx = self.pending_event_rx.take().ok_or_else(|| {
            tracing::error!(event = "ui.terminal.take_channels_double_call");
            TerminalError::ChannelsAlreadyTaken
        })?;
        Ok((byte_rx, event_rx))
    }

    /// Run the event batching loop. Called from TerminalView's cx.spawn() task.
    ///
    /// Batches PTY output in 4ms windows (250Hz max, 100 event cap), processes
    /// bytes through alacritty_terminal, and returns after each batch so the
    /// caller can notify GPUI to repaint.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_batch_loop(
        term: Arc<FairMutex<Term<KildListener>>>,
        pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
        error_state: Arc<Mutex<Option<String>>>,
        exited: Arc<AtomicBool>,
        mut byte_rx: UnboundedReceiver<Vec<u8>>,
        mut event_rx: UnboundedReceiver<AlacEvent>,
        executor: gpui::BackgroundExecutor,
        mut notify: impl FnMut(),
    ) {
        use futures::StreamExt;
        let mut processor: Processor = Processor::new();

        // Batching: 4ms ≈ 250 Hz max refresh — fast enough for smooth terminal
        // output while leaving CPU headroom. 100 event cap prevents unbounded
        // memory growth during output bursts (e.g. `cat` on a large file).
        while let Some(first_chunk) = byte_rx.next().await {
            let mut batch = vec![first_chunk];
            let batch_start = std::time::Instant::now();
            let batch_duration = std::time::Duration::from_millis(4);

            while batch.len() < 100 {
                match byte_rx.try_next() {
                    Ok(Some(chunk)) => batch.push(chunk),
                    Ok(None) => break,
                    Err(_) => {
                        if batch_start.elapsed() >= batch_duration {
                            break;
                        }
                        executor.timer(std::time::Duration::from_micros(500)).await;
                    }
                }
            }

            {
                let mut term = term.lock();
                for chunk in &batch {
                    processor.advance(&mut *term, chunk);
                }
            }

            while let Ok(Some(event)) = event_rx.try_next() {
                match event {
                    AlacEvent::Wakeup => {}
                    AlacEvent::PtyWrite(text) => match pty_writer.lock() {
                        Ok(mut writer) => {
                            if let Err(e) = writer.write_all(text.as_bytes()) {
                                tracing::error!(
                                    event = "ui.terminal.pty_write_loop_failed",
                                    error = %e
                                );
                                if let Ok(mut err) = error_state.lock() {
                                    *err = Some(format!("PTY write failed: {e}"));
                                }
                            }
                            if let Err(e) = writer.flush() {
                                tracing::error!(
                                    event = "ui.terminal.pty_flush_loop_failed",
                                    error = %e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                event = "ui.terminal.writer_lock_poisoned",
                                error = %e
                            );
                            if let Ok(mut err) = error_state.lock() {
                                *err = Some("PTY writer lock poisoned".to_string());
                            }
                            break;
                        }
                    },
                    _ => {}
                }
            }

            // Signal GPUI to repaint — this is the critical line that was missing.
            notify();
        }

        // Batch loop ended — shell exited or PTY closed.
        tracing::info!(event = "ui.terminal.batch_loop_ended");
        exited.store(true, Ordering::Release);
        if let Ok(mut err) = error_state.lock()
            && err.is_none()
        {
            *err = Some("Shell exited".to_string());
        }
        // Final repaint to show the exit state to the user.
        notify();
    }

    /// Write bytes to the PTY stdin.
    pub fn write_to_pty(&self, data: &[u8]) -> Result<(), TerminalError> {
        let mut writer = self.pty_writer.lock().map_err(|e| {
            tracing::error!(event = "ui.terminal.writer_lock_failed", error = %e);
            TerminalError::WriterLockPoisoned
        })?;
        writer.write_all(data).map_err(TerminalError::PtyWrite)?;
        writer.flush().map_err(TerminalError::PtyFlush)?;
        Ok(())
    }

    /// Get access to the terminal emulator (locked).
    pub(super) fn term(&self) -> &Arc<FairMutex<Term<KildListener>>> {
        &self.term
    }

    /// Get access to the PTY writer (for event batching loop).
    pub(super) fn pty_writer(&self) -> &Arc<Mutex<Box<dyn Write + Send>>> {
        &self.pty_writer
    }

    /// Get the shared error state for use in the batch loop.
    pub(super) fn error_state(&self) -> &Arc<Mutex<Option<String>>> {
        &self.error_state
    }

    /// Read the current error message, if any critical failure has occurred.
    pub fn error_message(&self) -> Option<String> {
        self.error_state.lock().ok().and_then(|guard| guard.clone())
    }

    /// Returns true if the shell has exited and the terminal is no longer active.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    /// Get the shared exited flag for use in the batch loop.
    pub(super) fn exited_flag(&self) -> &Arc<AtomicBool> {
        &self.exited
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        tracing::info!(event = "ui.terminal.cleanup_started");
        // Child gets SIGHUP from PTY closure, but explicit kill is safer
        if let Err(e) = self._child.kill() {
            tracing::warn!(event = "ui.terminal.kill_failed", error = %e);
        }
    }
}
