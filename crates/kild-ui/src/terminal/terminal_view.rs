use gpui::{
    Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render, Window, div, prelude::*,
};

use super::errors::TerminalError;
use super::input;
use super::state::Terminal;
use super::terminal_element::TerminalElement;
use crate::theme;

/// GPUI View wrapping TerminalElement with focus management and keyboard routing.
///
/// Owns the Terminal state and provides:
/// - Focus handling (keyboard events route here when terminal is visible)
/// - Key-to-escape translation via `input::keystroke_to_escape()`
/// - Resize detection (compares bounds between frames)
pub struct TerminalView {
    terminal: Terminal,
    focus_handle: FocusHandle,
}

impl TerminalView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self, TerminalError> {
        let terminal = Terminal::new(cx)?;
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);

        Ok(Self {
            terminal,
            focus_handle,
        })
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Check app cursor mode from terminal state
        let app_cursor = {
            let term = self.terminal.term().lock();
            let content = term.renderable_content();
            content
                .mode
                .contains(alacritty_terminal::term::TermMode::APP_CURSOR)
        };

        match input::keystroke_to_escape(&event.keystroke, app_cursor) {
            Some(bytes) => {
                self.terminal.write_to_pty(&bytes);
                cx.notify();
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

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .bg(theme::terminal_background())
            .child(TerminalElement::new(term, has_focus))
    }
}
