//! Terminal tab rename handlers for MainView.

use gpui::{Context, Focusable, Window, prelude::*};
use gpui_component::input::InputState;

use super::main_view_def::MainView;

impl MainView {
    #[allow(dead_code)]
    pub(super) fn start_rename(
        &mut self,
        session_id: &str,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_label = self
            .terminal_tabs
            .get(session_id)
            .and_then(|tabs| tabs.get(idx))
            .map(|e| e.label().to_string())
            .unwrap_or_default();

        let input = cx.new(|cx| InputState::new(window, cx).default_value(current_label));
        let handle = input.read(cx).focus_handle(cx).clone();
        self.renaming_tab = Some((session_id.to_string(), idx, input));
        window.focus(&handle);
        cx.notify();
    }

    pub(super) fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((session_id, idx, input)) = self.renaming_tab.take() {
            let new_name = input.read(cx).value().to_string();
            let new_name = new_name.trim();
            if !new_name.is_empty()
                && let Some(tabs) = self.terminal_tabs.get_mut(&session_id)
            {
                tabs.rename(idx, new_name.to_string());
            }
        }
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    pub(super) fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming_tab = None;
        self.focus_active_terminal(window, cx);
        cx.notify();
    }
}
