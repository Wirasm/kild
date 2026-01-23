//! Main view for shards-ui.
//!
//! Root view that composes header, shard list, and create dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Window, div,
    prelude::*, rgb,
};

use crate::actions;
use crate::state::AppState;
use crate::views::{create_dialog, shard_list};

/// Main application view that composes the shard list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
pub struct MainView {
    state: AppState,
    focus_handle: FocusHandle,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Handle click on the Create button in header.
    fn on_create_button_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.opened");
        self.state.show_create_dialog = true;
        cx.notify();
    }

    /// Handle dialog cancel button click.
    pub fn on_dialog_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.cancelled");
        self.state.show_create_dialog = false;
        self.state.reset_create_form();
        cx.notify();
    }

    /// Handle dialog submit button click.
    pub fn on_dialog_submit(&mut self, cx: &mut Context<Self>) {
        let branch = self.state.create_form.branch_name.trim().to_string();
        let agent = self.state.create_form.selected_agent.clone();

        match actions::create_shard(&branch, &agent) {
            Ok(_session) => {
                // Success - close dialog and refresh list
                self.state.show_create_dialog = false;
                self.state.reset_create_form();
                self.state.refresh_sessions();
            }
            Err(e) => {
                // Error - show in dialog
                self.state.create_error = Some(e);
            }
        }
        cx.notify();
    }

    /// Cycle to the next agent in the list.
    pub fn on_agent_cycle(&mut self, cx: &mut Context<Self>) {
        let agents = create_dialog::agent_options();
        if agents.is_empty() {
            tracing::error!(event = "ui.create_dialog.no_agents_available");
            self.state.create_error =
                Some("No agents available. Check shards-core configuration.".to_string());
            cx.notify();
            return;
        }
        let next_index = (self.state.create_form.selected_agent_index + 1) % agents.len();
        self.state.create_form.selected_agent_index = next_index;
        self.state.create_form.selected_agent = agents[next_index].to_string();
        tracing::info!(
            event = "ui.create_dialog.agent_changed",
            agent = self.state.create_form.selected_agent
        );
        cx.notify();
    }

    /// Handle keyboard input when the create dialog is open.
    ///
    /// Handles branch name input (alphanumeric, -, _, /, space converts to hyphen),
    /// form submission (Enter), dialog dismissal (Escape), and agent cycling (Tab).
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.show_create_dialog {
            return;
        }

        let key_str = event.keystroke.key.to_string();

        match key_str.as_str() {
            "backspace" => {
                self.state.create_form.branch_name.pop();
                cx.notify();
            }
            "enter" => {
                self.on_dialog_submit(cx);
            }
            "escape" => {
                self.on_dialog_cancel(cx);
            }
            "space" => {
                // Allow spaces but convert to hyphens for branch names
                self.state.create_form.branch_name.push('-');
                cx.notify();
            }
            "tab" => {
                // Cycle agent on tab
                self.on_agent_cycle(cx);
            }
            key if key.len() == 1 => {
                // Single character - add to branch name if valid for branch names
                if let Some(c) = key.chars().next()
                    && (c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
                {
                    self.state.create_form.branch_name.push(c);
                    cx.notify();
                }
            }
            _ => {
                // Ignore other keys
            }
        }
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            // Header with title and Create button
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xl()
                            .text_color(rgb(0xffffff))
                            .font_weight(FontWeight::BOLD)
                            .child("Shards"),
                    )
                    .child(
                        div()
                            .id("create-header-btn")
                            .px_3()
                            .py_1()
                            .bg(rgb(0x4a9eff))
                            .hover(|style| style.bg(rgb(0x5aafff)))
                            .rounded_md()
                            .cursor_pointer()
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|view, _, _, cx| {
                                    view.on_create_button_click(cx);
                                }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(div().text_color(rgb(0xffffff)).child("+"))
                                    .child(div().text_color(rgb(0xffffff)).child("Create")),
                            ),
                    ),
            )
            // Shard list
            .child(shard_list::render_shard_list(&self.state, cx))
            // Create dialog (conditional)
            .when(self.state.show_create_dialog, |this| {
                this.child(create_dialog::render_create_dialog(&self.state, cx))
            })
    }
}
