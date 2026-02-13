use std::time::Duration;

use gpui::{
    ClipboardItem, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render, Task,
    Window, div, prelude::*, px,
};
use tracing::debug;

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

use super::input;
use super::state::Terminal;
use super::terminal_element::{MouseState, TerminalElement};
use crate::theme;

/// GPUI View wrapping TerminalElement with focus management and keyboard routing.
///
/// Owns the Terminal state and provides:
/// - Focus handling (keyboard events route here when terminal is visible)
/// - Key-to-escape translation via `input::keystroke_to_escape()`
/// - Event batching with repaint notification after each batch
pub struct TerminalView {
    terminal: Terminal,
    focus_handle: FocusHandle,
    /// Event batching task. Stored to prevent cancellation.
    _event_task: Task<()>,
    /// Whether the cursor is currently visible in the blink cycle.
    cursor_visible: bool,
    /// Mouse state passed to TerminalElement on each render.
    /// TerminalElement is reconstructed every frame -- do not cache instances.
    mouse_state: MouseState,
    /// Monotonic epoch incremented on each blink reset. Stale timers detect
    /// mismatched epochs and exit, preventing old timers from toggling state.
    blink_epoch: usize,
    /// Blink timer task. Stored to prevent cancellation.
    _blink_task: Task<()>,
}

impl TerminalView {
    /// Create a TerminalView from a pre-built Terminal.
    ///
    /// Terminal creation (fallible) happens outside `cx.new()` so errors can
    /// be handled before entering the infallible closure. Spawns the event
    /// batching task via `cx.spawn()` so it can notify GPUI to repaint.
    pub fn from_terminal(
        mut terminal: Terminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);

        // take_channels is called exactly once; a double-call is a logic bug
        let (byte_rx, event_rx) = terminal.take_channels().expect(
            "take_channels failed: channels already taken — this is a logic bug in TerminalView",
        );
        let term = terminal.term().clone();
        let pty_writer = terminal.pty_writer().clone();
        let error_state = terminal.error_state().clone();
        let exited = terminal.exited_flag().clone();
        let executor = cx.background_executor().clone();

        let event_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            Terminal::run_batch_loop(
                term,
                pty_writer,
                error_state,
                exited,
                byte_rx,
                event_rx,
                executor,
                || {
                    let _ = this.update(cx, |_, cx| cx.notify());
                },
            )
            .await;
        });

        let blink_epoch: usize = 0;
        let blink_task = Self::spawn_blink_timer(cx, blink_epoch);

        Self {
            terminal,
            focus_handle,
            _event_task: event_task,
            cursor_visible: true,
            blink_epoch,
            _blink_task: blink_task,
            mouse_state: MouseState {
                position: None,
                cmd_held: false,
            },
        }
    }

    /// Create a TerminalView without initial focus.
    ///
    /// Used when creating terminals from async contexts (daemon attach) where
    /// `&mut Window` is not available. Focus is applied later by the caller
    /// via `focus_active_terminal()`.
    pub fn from_terminal_unfocused(mut terminal: Terminal, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let (byte_rx, event_rx) = terminal.take_channels().expect(
            "take_channels failed: channels already taken — this is a logic bug in TerminalView",
        );
        let term = terminal.term().clone();
        let pty_writer = terminal.pty_writer().clone();
        let error_state = terminal.error_state().clone();
        let exited = terminal.exited_flag().clone();
        let executor = cx.background_executor().clone();

        let event_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            Terminal::run_batch_loop(
                term,
                pty_writer,
                error_state,
                exited,
                byte_rx,
                event_rx,
                executor,
                || {
                    let _ = this.update(cx, |_, cx| cx.notify());
                },
            )
            .await;
        });

        let blink_epoch: usize = 0;
        let blink_task = Self::spawn_blink_timer(cx, blink_epoch);

        Self {
            terminal,
            focus_handle,
            _event_task: event_task,
            cursor_visible: true,
            blink_epoch,
            _blink_task: blink_task,
            mouse_state: MouseState {
                position: None,
                cmd_held: false,
            },
        }
    }

    /// Access the underlying terminal state (e.g. to check `has_exited()`).
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Spawn a blink timer that toggles `cursor_visible` every interval.
    /// The timer exits when its captured epoch no longer matches `self.blink_epoch`
    /// (i.e. a newer timer replaced it) or when the view is dropped.
    fn spawn_blink_timer(cx: &mut Context<Self>, epoch: usize) -> Task<()> {
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            loop {
                cx.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
                let should_continue = this.update(cx, |view, cx| {
                    if view.blink_epoch != epoch {
                        return false;
                    }
                    view.cursor_visible = !view.cursor_visible;
                    cx.notify();
                    true
                });
                match should_continue {
                    Ok(true) => continue,
                    Ok(false) => {
                        debug!(
                            event = "ui.terminal.blink_stopped",
                            reason = "stale_epoch",
                            epoch,
                        );
                        break;
                    }
                    Err(e) => {
                        debug!(
                            event = "ui.terminal.blink_stopped",
                            reason = "view_dropped",
                            error = ?e,
                            epoch,
                        );
                        break;
                    }
                }
            }
        })
    }

    /// Reset blink cycle: make cursor visible immediately and start a fresh
    /// timer. The old timer detects the incremented epoch and exits.
    fn reset_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_epoch = self.blink_epoch.wrapping_add(1);
        self._blink_task = Self::spawn_blink_timer(cx, self.blink_epoch);
        cx.notify();
    }

    fn set_error(&self, msg: String) {
        match self.terminal.error_state().lock() {
            Ok(mut err) => *err = Some(msg),
            Err(e) => tracing::error!(event = "ui.terminal.set_error_lock_poisoned", error = %e),
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_cmd = event.modifiers.platform;
        let new_pos = Some(event.position);
        if self.mouse_state.position != new_pos || self.mouse_state.cmd_held != new_cmd {
            self.mouse_state.position = new_pos;
            self.mouse_state.cmd_held = new_cmd;
            cx.notify();
        }
    }

    fn on_modifiers_changed(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_cmd = event.modifiers.platform;
        if self.mouse_state.cmd_held != new_cmd {
            self.mouse_state.cmd_held = new_cmd;
            cx.notify();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.reset_blink(cx);

        let key = event.keystroke.key.as_str();
        let cmd = event.keystroke.modifiers.platform;

        if event.keystroke.modifiers.control && key == "tab" {
            cx.propagate();
            return;
        }

        // Cmd+J/K/D: reserved for kild navigation — propagate to MainView
        if cmd && matches!(key, "j" | "k" | "d") {
            cx.propagate();
            return;
        }

        tracing::debug!(
            event = "ui.terminal.key_down_started",
            key = key,
            ctrl = event.keystroke.modifiers.control,
            alt = event.keystroke.modifiers.alt,
            cmd = cmd,
        );

        // Cmd+C: copy selection to clipboard and clear it, or send Ctrl+C (SIGINT) if no selection
        if cmd && key == "c" {
            let text = self.terminal.term().lock().selection_to_string();
            if let Some(text) = text {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.terminal.term().lock().selection = None;
                cx.notify();
            } else if let Err(e) = self.terminal.write_to_pty(&[0x03]) {
                tracing::error!(event = "ui.terminal.key_write_failed", error = %e);
                self.set_error(format!("Failed to send interrupt: {e}"));
                cx.notify();
            }
            return;
        }

        // Cmd+V: paste clipboard to PTY stdin
        if cmd && key == "v" {
            if let Some(clipboard) = cx.read_from_clipboard()
                && let Some(text) = clipboard.text()
                && let Err(e) = self.terminal.write_to_pty(text.as_bytes())
            {
                tracing::error!(event = "ui.terminal.paste_failed", error = %e);
                self.set_error(format!("Paste failed: {e}"));
                cx.notify();
            }
            return;
        }

        // Check app cursor mode from terminal state.
        // Must query on every keystroke since apps can change mode anytime.
        let app_cursor = {
            let term = self.terminal.term().lock();
            let content = term.renderable_content();
            content
                .mode
                .contains(alacritty_terminal::term::TermMode::APP_CURSOR)
        };

        match input::keystroke_to_escape(&event.keystroke, app_cursor) {
            Some(bytes) => {
                if let Err(e) = self.terminal.write_to_pty(&bytes) {
                    tracing::error!(event = "ui.terminal.key_write_failed", error = %e);
                    self.set_error(format!("Failed to send input: {e}"));
                    cx.notify();
                }
            }
            None => {
                // Unhandled key (e.g., Ctrl+T) — propagate to parent
                cx.propagate();
            }
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let term = self.terminal.term().clone();
        let has_focus = self.focus_handle.is_focused(window);
        let resize_handle = self.terminal.resize_handle();
        let error = self.terminal.error_message();

        let mut container = div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .size_full()
            .bg(theme::terminal_background());

        if let Some(msg) = error {
            container = container.child(
                div()
                    .w_full()
                    .px(px(theme::SPACE_3))
                    .py(px(theme::SPACE_2))
                    .bg(theme::ember())
                    .text_color(theme::text_white())
                    .text_size(px(theme::TEXT_SM))
                    .child(format!("Terminal error: {msg}. Ctrl+Esc to return.")),
            );
        }

        // Focused cursors blink; unfocused cursors are always visible (rendered as thin bar in element).
        let cursor_visible = if has_focus { self.cursor_visible } else { true };

        container.child(TerminalElement::new(
            term,
            has_focus,
            resize_handle,
            cursor_visible,
            MouseState {
                position: self.mouse_state.position,
                cmd_held: self.mouse_state.cmd_held,
            },
        ))
    }
}
