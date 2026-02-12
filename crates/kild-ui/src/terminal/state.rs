use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;
use base64::Engine;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::Task;
use kild_protocol::DaemonMessage;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::errors::TerminalError;
use crate::daemon_client::{self, DaemonConnection};

/// Default PTY dimensions used until the first resize event.
/// ResizeHandle will update to actual window dimensions on prepaint.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

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

/// Commands sent from the sync DaemonPtyWriter to the async writer task.
pub(crate) enum DaemonWriteCommand {
    Stdin(Vec<u8>),
    Resize(u16, u16),
    Detach,
}

/// Bridges synchronous Write calls to async daemon IPC.
///
/// Keyboard input and AlacEvent::PtyWrite both call write_to_pty(),
/// which uses this writer. Bytes are buffered and sent as WriteStdin
/// IPC messages by the background writer task.
struct DaemonPtyWriter {
    tx: futures::channel::mpsc::UnboundedSender<DaemonWriteCommand>,
}

impl Write for DaemonPtyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        tracing::debug!(event = "ui.terminal.daemon_pty_write", bytes = buf.len(),);
        self.tx
            .unbounded_send(DaemonWriteCommand::Stdin(buf.to_vec()))
            .map_err(|e| {
                tracing::error!(
                    event = "ui.terminal.daemon_pty_write_failed",
                    error = %e,
                    bytes_lost = buf.len(),
                );
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "daemon writer task dropped")
            })?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Terminal mode — determines PTY backend and lifecycle behavior.
///
/// Encodes mode-specific resources as enum variants, making illegal states
/// unrepresentable. A Local terminal cannot have a daemon writer task,
/// and a Daemon terminal cannot have a child process.
enum TerminalMode {
    /// Local PTY with shell child process.
    Local {
        child: Box<dyn Child + Send + Sync>,
        pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    },
    /// Daemon-backed PTY via IPC.
    Daemon {
        /// Stored to prevent cancellation (dropping the Task cancels it).
        _writer_task: Task<()>,
        cmd_tx: futures::channel::mpsc::UnboundedSender<DaemonWriteCommand>,
    },
}

/// Core terminal state wrapping alacritty_terminal's Term with PTY lifecycle.
///
/// Manages:
/// - VT100 emulation via `alacritty_terminal::Term`
/// - PTY process (spawn, read, write) — local or daemon-backed
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
    /// Mode-specific resources (local child process or daemon IPC handles).
    mode: TerminalMode,
    /// Pending channels for event batching. Taken once by TerminalView.
    pending_byte_rx: Option<UnboundedReceiver<Vec<u8>>>,
    pending_event_rx: Option<UnboundedReceiver<AlacEvent>>,
    /// Shared error state set on critical failures (channel errors, write failures, lock poisoning).
    error_state: Arc<Mutex<Option<String>>>,
    /// Set to true when the batch loop exits (shell exited or PTY closed).
    exited: Arc<AtomicBool>,
    /// Current PTY dimensions (rows, cols). Compared in prepaint to detect changes.
    current_size: Arc<Mutex<(u16, u16)>>,
}

impl Terminal {
    /// Create a new terminal with a live shell session (local PTY).
    ///
    /// Spawns the user's default shell, starts a background reader task
    /// for PTY output, and sets up 4ms event batching.
    pub fn new(cx: &mut gpui::App) -> Result<Self, TerminalError> {
        let rows = DEFAULT_ROWS;
        let cols = DEFAULT_COLS;

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

        // Store master for resize operations. The reader thread gets a clone
        // of the Arc to keep the PTY alive (dropping master would close it).
        let pty_master = Arc::new(Mutex::new(pair.master));
        let master_keepalive = pty_master.clone();
        let current_size = Arc::new(Mutex::new((rows, cols)));

        // Spawn blocking PTY reader on a dedicated thread via std::thread.
        // GPUI's BackgroundExecutor is async/cooperative — blocking reads would
        // starve other tasks. Use a real OS thread instead.
        let (byte_tx, byte_rx) = futures::channel::mpsc::unbounded::<Vec<u8>>();
        let pty_reader_task = cx.background_executor().spawn(async move {
            // Move the blocking read loop to a dedicated OS thread
            let (done_tx, done_rx) = futures::channel::oneshot::channel::<()>();
            std::thread::spawn(move || {
                // Hold master Arc in reader thread to keep PTY alive
                let _master = master_keepalive;
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
            mode: TerminalMode::Local { child, pty_master },
            pending_byte_rx: Some(byte_rx),
            pending_event_rx: Some(event_rx),
            error_state: Arc::new(Mutex::new(None)),
            exited: Arc::new(AtomicBool::new(false)),
            current_size,
        })
    }

    /// Create a terminal backed by a daemon session via IPC.
    ///
    /// Connects to an already-attached daemon session via `DaemonConnection`.
    /// Spawns IPC reader/writer tasks for streaming PTY output and sending
    /// keystrokes. The rendering pipeline (batch loop, alacritty_terminal,
    /// TerminalElement) is completely unchanged — only the byte source differs.
    pub fn from_daemon(
        session_id: String,
        conn: DaemonConnection,
        cx: &mut gpui::App,
    ) -> Result<Self, TerminalError> {
        let rows = DEFAULT_ROWS;
        let cols = DEFAULT_COLS;

        // Create event channel for alacritty_terminal events
        let (event_tx, event_rx) = futures::channel::mpsc::unbounded();
        let listener = KildListener { sender: event_tx };

        // Create alacritty_terminal instance (same as local mode)
        let config = TermConfig::default();
        let dims = TermDimensions {
            cols: cols as usize,
            screen_lines: rows as usize,
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &dims, listener)));

        // Create DaemonWriteCommand channel (futures::channel for async-ready receive)
        let (cmd_tx, mut cmd_rx) = futures::channel::mpsc::unbounded::<DaemonWriteCommand>();

        // Create DaemonPtyWriter wrapping the sender
        let daemon_writer = DaemonPtyWriter { tx: cmd_tx.clone() };
        let pty_writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(daemon_writer)));

        // Create byte channel (same interface as local mode)
        let (byte_tx, byte_rx) = futures::channel::mpsc::unbounded::<Vec<u8>>();

        let current_size = Arc::new(Mutex::new((rows, cols)));
        let exited = Arc::new(AtomicBool::new(false));
        let error_state: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // Destructure connection into reader/writer halves
        let (reader, writer, _conn_session_id) = conn.into_parts();

        // Spawn IPC reader task: reads JSONL from daemon, base64 decodes, feeds byte channel
        let reader_exited = exited.clone();
        let reader_error = error_state.clone();
        let mut reader = reader;
        let reader_session_id = session_id.clone();
        let pty_reader_task = cx.background_executor().spawn(async move {
            tracing::info!(
                event = "ui.terminal.daemon_reader_started",
                session_id = reader_session_id
            );
            let mut line = String::new();
            loop {
                line.clear();
                match smol::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => {
                        tracing::info!(event = "ui.terminal.daemon_reader_eof");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<DaemonMessage>(trimmed) {
                            Ok(DaemonMessage::PtyOutput { data, .. }) => {
                                match base64::engine::general_purpose::STANDARD.decode(&data) {
                                    Ok(decoded) => {
                                        if byte_tx.unbounded_send(decoded).is_err() {
                                            tracing::error!(
                                                event = "ui.terminal.daemon_byte_send_failed"
                                            );
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            event = "ui.terminal.daemon_base64_decode_failed",
                                            error = %e,
                                        );
                                        if let Ok(mut err) = reader_error.lock()
                                            && err.is_none()
                                        {
                                            *err = Some(format!(
                                                "Terminal data corrupted (base64 decode): {e}"
                                            ));
                                        }
                                        break;
                                    }
                                }
                            }
                            Ok(DaemonMessage::PtyOutputDropped { bytes_dropped, .. }) => {
                                tracing::warn!(
                                    event = "ui.terminal.daemon_output_dropped",
                                    bytes_dropped = bytes_dropped
                                );
                            }
                            Ok(DaemonMessage::SessionEvent { event: ref ev, .. })
                                if ev == "stopped" =>
                            {
                                tracing::info!(
                                    event = "ui.terminal.daemon_session_stopped",
                                    session_id = reader_session_id
                                );
                                break;
                            }
                            Ok(other) => {
                                tracing::debug!(
                                    event = "ui.terminal.daemon_message_ignored",
                                    message = ?other
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    event = "ui.terminal.daemon_parse_failed",
                                    error = %e,
                                    line = trimmed
                                );
                                if let Ok(mut err) = reader_error.lock()
                                    && err.is_none()
                                {
                                    *err = Some(format!("Daemon protocol error: {e}"));
                                }
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            event = "ui.terminal.daemon_reader_failed",
                            error = %e
                        );
                        break;
                    }
                }
            }
            // Mark as exited
            reader_exited.store(true, Ordering::Release);
            if let Ok(mut err) = reader_error.lock()
                && err.is_none()
            {
                *err = Some("Daemon session ended".to_string());
            }
        });

        // Spawn IPC writer task: reads DaemonWriteCommand, sends to daemon via IPC
        let mut writer = writer;
        let writer_session_id = session_id.clone();
        let writer_error = error_state.clone();
        let writer_task = cx.background_executor().spawn(async move {
            use futures::StreamExt;
            tracing::info!(
                event = "ui.terminal.daemon_writer_started",
                session_id = writer_session_id
            );
            while let Some(cmd) = cmd_rx.next().await {
                match cmd {
                    DaemonWriteCommand::Stdin(data) => {
                        if let Err(e) =
                            daemon_client::send_write_stdin(&mut writer, &writer_session_id, &data)
                                .await
                        {
                            tracing::error!(
                                event = "ui.terminal.daemon_write_failed",
                                error = %e,
                                bytes_lost = data.len(),
                            );
                            if let Ok(mut err) = writer_error.lock()
                                && err.is_none()
                            {
                                *err = Some(format!("Daemon write failed: {e}"));
                            }
                            break;
                        }
                    }
                    DaemonWriteCommand::Resize(r, c) => {
                        if let Err(e) =
                            daemon_client::send_resize(&mut writer, &writer_session_id, r, c).await
                        {
                            tracing::warn!(
                                event = "ui.terminal.daemon_resize_failed",
                                error = %e,
                                rows = r,
                                cols = c,
                            );
                        }
                    }
                    DaemonWriteCommand::Detach => {
                        if let Err(e) =
                            daemon_client::send_detach(&mut writer, &writer_session_id).await
                        {
                            tracing::warn!(
                                event = "ui.terminal.daemon_detach_failed",
                                error = %e
                            );
                        }
                        break;
                    }
                }
            }
            tracing::info!(
                event = "ui.terminal.daemon_writer_stopped",
                session_id = writer_session_id
            );
        });

        tracing::info!(
            event = "ui.terminal.daemon_create_completed",
            session_id = session_id
        );

        Ok(Self {
            term,
            pty_writer,
            _pty_reader_task: pty_reader_task,
            mode: TerminalMode::Daemon {
                _writer_task: writer_task,
                cmd_tx,
            },
            pending_byte_rx: Some(byte_rx),
            pending_event_rx: Some(event_rx),
            error_state,
            exited,
            current_size,
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

    /// Get a resize handle for use by TerminalElement.
    pub(crate) fn resize_handle(&self) -> ResizeHandle {
        let resize_impl = match &self.mode {
            TerminalMode::Local { pty_master, .. } => ResizeImpl::Local(pty_master.clone()),
            TerminalMode::Daemon { cmd_tx, .. } => ResizeImpl::Daemon(cmd_tx.clone()),
        };
        ResizeHandle {
            terminal_term: self.term.clone(),
            resize_impl,
            current_size: self.current_size.clone(),
        }
    }
}

/// Resize implementation, determined by terminal mode (local PTY or daemon IPC).
pub(crate) enum ResizeImpl {
    Local(Arc<Mutex<Box<dyn MasterPty + Send>>>),
    Daemon(futures::channel::mpsc::UnboundedSender<DaemonWriteCommand>),
}

/// Shared references for resize operations, passed to TerminalElement.
///
/// Bundles the Arc refs needed to resize both the PTY (SIGWINCH) and the
/// terminal grid (reflow). Created via `Terminal::resize_handle()`.
pub(crate) struct ResizeHandle {
    terminal_term: Arc<FairMutex<Term<KildListener>>>,
    resize_impl: ResizeImpl,
    current_size: Arc<Mutex<(u16, u16)>>,
}

impl ResizeHandle {
    /// Resize PTY and terminal grid if dimensions changed.
    ///
    /// Called from TerminalElement::prepaint(). No-op if (rows, cols)
    /// match the stored size.
    ///
    /// Lock ordering: current_size → pty_master → term. Each lock is held
    /// only for its specific operation and released before acquiring the next,
    /// minimizing contention with the PTY reader thread and batch loop.
    pub fn resize_if_changed(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        // Check + update stored size
        {
            let mut size = self.current_size.lock().map_err(|e| {
                tracing::error!(
                    event = "ui.terminal.resize_size_lock_failed",
                    error = %e,
                );
                TerminalError::PtyResize {
                    message: format!("current_size lock poisoned: {e}"),
                }
            })?;
            if size.0 == rows && size.1 == cols {
                return Ok(());
            }
            *size = (rows, cols);
        }

        // Resize the underlying PTY/daemon
        match &self.resize_impl {
            ResizeImpl::Local(master) => {
                let master = master.lock().map_err(|e| {
                    tracing::error!(
                        event = "ui.terminal.resize_master_lock_failed",
                        error = %e,
                    );
                    TerminalError::PtyResize {
                        message: format!("pty_master lock poisoned: {e}"),
                    }
                })?;
                let new_size = PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                };
                if let Err(e) = master.resize(new_size) {
                    tracing::warn!(
                        event = "ui.terminal.pty_resize_failed",
                        rows = rows,
                        cols = cols,
                        error = %e,
                    );
                }
            }
            ResizeImpl::Daemon(tx) => {
                if let Err(e) = tx.unbounded_send(DaemonWriteCommand::Resize(rows, cols)) {
                    tracing::warn!(
                        event = "ui.terminal.daemon_resize_send_failed",
                        rows = rows,
                        cols = cols,
                        error = %e,
                    );
                }
            }
        }

        // Resize terminal grid (reflows content)
        {
            let mut term = self.terminal_term.lock();
            term.resize(TermDimensions {
                cols: cols as usize,
                screen_lines: rows as usize,
            });
        }

        tracing::debug!(
            event = "ui.terminal.resize_completed",
            rows = rows,
            cols = cols,
        );
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        tracing::info!(event = "ui.terminal.cleanup_started");
        match &mut self.mode {
            TerminalMode::Local { child, .. } => {
                // Local mode: child gets SIGHUP from PTY closure, but explicit kill is safer
                if let Err(e) = child.kill() {
                    tracing::warn!(event = "ui.terminal.kill_failed", error = %e);
                }
            }
            TerminalMode::Daemon { cmd_tx, .. } => {
                // Daemon mode: send Detach command (best-effort — writer task may have exited)
                if let Err(e) = cmd_tx.unbounded_send(DaemonWriteCommand::Detach) {
                    tracing::warn!(
                        event = "ui.terminal.cleanup_detach_send_failed",
                        error = %e,
                        reason = "writer task likely already exited"
                    );
                }
            }
        }
    }
}
