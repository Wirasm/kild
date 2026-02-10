use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;
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
        let _ = self.sender.unbounded_send(event);
    }
}

/// Core terminal state wrapping alacritty_terminal's Term with PTY lifecycle.
///
/// Manages:
/// - VT100 emulation via `alacritty_terminal::Term`
/// - PTY process (spawn, read, write)
/// - 4ms event batching for performance
pub struct Terminal {
    /// The terminal emulator state, protected by FairMutex to prevent
    /// lock starvation between the PTY reader and the GPUI renderer.
    term: Arc<FairMutex<Term<KildListener>>>,
    /// PTY stdin writer. Arc<Mutex<>> because take_writer() is one-shot.
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Background PTY reader task. Stored to prevent cancellation.
    _pty_reader_task: Task<()>,
    /// Event batching task. Stored to prevent cancellation.
    _event_task: Task<()>,
    /// Child process handle. Stored for shutdown.
    _child: Box<dyn Child + Send + Sync>,
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
            .map_err(|e| TerminalError::PtyCreation(format!("openpty: {}", e)))?;

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
            .map_err(|e| TerminalError::PtyCreation(format!("spawn: {}", e)))?;
        // Drop slave after spawning (important: frees the slave side)
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
            .map_err(|e| TerminalError::PtyIo(format!("take_writer: {}", e)))?;
        let pty_writer = Arc::new(Mutex::new(writer));

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::PtyIo(format!("clone_reader: {}", e)))?;

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
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!(event = "ui.terminal.pty_read_failed", error = %e);
                            break;
                        }
                    }
                }
                let _ = done_tx.send(());
            });
            let _ = done_rx.await;
        });

        // Spawn event batching task (4ms window, 100 event cap)
        // Uses background_executor since it doesn't need entity references.
        let term_batcher = term.clone();
        let pty_writer_for_events = pty_writer.clone();
        let executor = cx.background_executor().clone();
        let event_task = cx.background_executor().spawn(async move {
            use futures::StreamExt;
            let mut processor: Processor = Processor::new();
            let mut byte_rx = byte_rx;
            let mut event_rx = event_rx;

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
                    let mut term = term_batcher.lock();
                    for chunk in &batch {
                        processor.advance(&mut *term, chunk);
                    }
                }

                while let Ok(Some(event)) = event_rx.try_next() {
                    match event {
                        AlacEvent::Wakeup => {}
                        AlacEvent::PtyWrite(text) => {
                            if let Ok(mut writer) = pty_writer_for_events.lock() {
                                let _ = writer.write_all(text.as_bytes());
                                let _ = writer.flush();
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(Self {
            term,
            pty_writer,
            _pty_reader_task: pty_reader_task,
            _event_task: event_task,
            _child: child,
        })
    }

    /// Write bytes to the PTY stdin.
    pub fn write_to_pty(&self, data: &[u8]) {
        if let Ok(mut writer) = self.pty_writer.lock() {
            if let Err(e) = writer.write_all(data) {
                tracing::error!(event = "ui.terminal.pty_write_failed", error = %e);
            }
            let _ = writer.flush();
        }
    }

    /// Get access to the terminal emulator (locked).
    pub fn term(&self) -> &Arc<FairMutex<Term<KildListener>>> {
        &self.term
    }
}
