//! Main view for shards-ui.
//!
//! Root view that composes header, shard list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Window, div,
    prelude::*, rgb,
};

use crate::actions;
use crate::state::AppState;
use crate::views::{confirm_dialog, create_dialog, shard_list};

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
                tracing::warn!(
                    event = "ui.dialog_submit.error_displayed",
                    branch = %branch,
                    agent = %agent,
                    error = %e
                );
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

    /// Handle click on the Refresh button in header.
    fn on_refresh_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.refresh_clicked");
        self.state.refresh_sessions();
        cx.notify();
    }

    /// Handle click on the destroy button [×] in a shard row.
    pub fn on_destroy_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.destroy_dialog.opened", branch = branch);
        self.state.confirm_target_branch = Some(branch.to_string());
        self.state.show_confirm_dialog = true;
        cx.notify();
    }

    /// Handle confirm button click in destroy dialog.
    pub fn on_confirm_destroy(&mut self, cx: &mut Context<Self>) {
        let Some(branch) = self.state.confirm_target_branch.clone() else {
            tracing::warn!(event = "ui.confirm_destroy.no_target");
            return;
        };

        match actions::destroy_shard(&branch) {
            Ok(()) => {
                self.state.reset_confirm_dialog();
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.confirm_destroy.error_displayed",
                    branch = %branch,
                    error = %e
                );
                self.state.confirm_error = Some(e);
            }
        }
        cx.notify();
    }

    /// Handle cancel button click in destroy dialog.
    pub fn on_confirm_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.confirm_dialog.cancelled");
        self.state.reset_confirm_dialog();
        cx.notify();
    }

    /// Handle click on the Open button [▶] in a shard row.
    pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_clicked", branch = branch);
        self.state.clear_open_error();

        match actions::open_shard(branch, None) {
            Ok(_session) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.open_click.error_displayed",
                    branch = branch,
                    error = %e
                );
                self.state.open_error = Some(crate::state::OperationError {
                    branch: branch.to_string(),
                    message: e,
                });
            }
        }
        cx.notify();
    }

    /// Handle click on the Stop button [⏹] in a shard row.
    pub fn on_stop_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_clicked", branch = branch);
        self.state.clear_stop_error();

        match actions::stop_shard(branch) {
            Ok(()) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.stop_click.error_displayed",
                    branch = branch,
                    error = %e
                );
                self.state.stop_error = Some(crate::state::OperationError {
                    branch: branch.to_string(),
                    message: e,
                });
            }
        }
        cx.notify();
    }

    /// Handle keyboard input for dialogs.
    ///
    /// When create dialog is open: handles branch name input (alphanumeric, -, _, /, space converts to hyphen),
    /// form submission (Enter), dialog dismissal (Escape), and agent cycling (Tab).
    /// When confirm dialog is open: handles dialog dismissal (Escape).
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key_str = event.keystroke.key.to_string();

        // Handle confirm dialog escape
        if self.state.show_confirm_dialog && key_str == "escape" {
            self.on_confirm_cancel(cx);
            return;
        }

        // Create dialog keyboard handling
        if !self.state.show_create_dialog {
            return;
        }

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
            // Header with title, Refresh button, and Create button
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
                            .flex()
                            .items_center()
                            .gap_2()
                            // Refresh button - TEXT label, gray background (secondary action)
                            .child(
                                div()
                                    .id("refresh-btn")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(0x444444))
                                    .hover(|style| style.bg(rgb(0x555555)))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|view, _, _, cx| {
                                            view.on_refresh_click(cx);
                                        }),
                                    )
                                    .child(div().text_color(rgb(0xffffff)).child("Refresh")),
                            )
                            // Create button - blue/accent background (primary action)
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
                    ),
            )
            // Shard list
            .child(shard_list::render_shard_list(&self.state, cx))
            // Create dialog (conditional)
            .when(self.state.show_create_dialog, |this| {
                this.child(create_dialog::render_create_dialog(&self.state, cx))
            })
            // Confirm dialog (conditional)
            .when(self.state.show_confirm_dialog, |this| {
                this.child(confirm_dialog::render_confirm_dialog(&self.state, cx))
            })
    }
}
