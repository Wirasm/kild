use gpui::{
    Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render, Task, Window, div,
    prelude::*, px,
};

use super::input;
use super::state::Terminal;
use super::terminal_element::TerminalElement;
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

        Self {
            terminal,
            focus_handle,
            _event_task: event_task,
        }
    }

    /// Access the underlying terminal state (e.g. to check `has_exited()`).
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
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
        let error = self.terminal.error_message();

        let mut container = div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
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
                    .child(format!("Terminal error: {msg}. Press Ctrl+T to close.")),
            );
        }

        container.child(TerminalElement::new(term, has_focus))
    }
}
