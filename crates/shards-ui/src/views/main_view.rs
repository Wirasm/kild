//! Main view for shards-ui.
//!
//! Root view that composes header, shard list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Task, Window,
    div, prelude::*, rgb,
};

use crate::actions;
use crate::state::{AppState, CreateDialogField};
use crate::views::{confirm_dialog, create_dialog, shard_list};

/// Main application view that composes the shard list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
pub struct MainView {
    state: AppState,
    focus_handle: FocusHandle,
    /// Handle to the background refresh task. Must be stored to prevent cancellation.
    _refresh_task: Task<()>,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let refresh_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            tracing::debug!(event = "ui.auto_refresh.started");

            loop {
                cx.background_executor()
                    .timer(crate::refresh::REFRESH_INTERVAL)
                    .await;

                if let Err(e) = this.update(cx, |view, cx| {
                    tracing::debug!(event = "ui.auto_refresh.tick");
                    view.state.update_statuses_only();
                    cx.notify();
                }) {
                    tracing::debug!(
                        event = "ui.auto_refresh.stopped",
                        reason = "view_dropped",
                        error = ?e
                    );
                    break;
                }
            }
        });

        Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
            _refresh_task: refresh_task,
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
        let agent = self.state.create_form.selected_agent();
        let note = if self.state.create_form.note.trim().is_empty() {
            None
        } else {
            Some(self.state.create_form.note.trim().to_string())
        };

        match actions::create_shard(&branch, &agent, note) {
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
        tracing::info!(
            event = "ui.create_dialog.agent_changed",
            agent = %self.state.create_form.selected_agent()
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

    /// Handle click on the Open All button.
    fn on_open_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_all_clicked");
        self.handle_bulk_operation(actions::open_all_stopped, "ui.open_all.partial_failure", cx);
    }

    /// Handle click on the Stop All button.
    fn on_stop_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_all_clicked");
        self.handle_bulk_operation(actions::stop_all_running, "ui.stop_all.partial_failure", cx);
    }

    /// Common handler for bulk operations (open all / stop all).
    fn handle_bulk_operation(
        &mut self,
        operation: impl Fn(&[crate::state::ShardDisplay]) -> (usize, Vec<crate::state::OperationError>),
        error_event: &str,
        cx: &mut Context<Self>,
    ) {
        self.state.clear_bulk_errors();

        let (count, errors) = operation(&self.state.displays);

        for error in &errors {
            tracing::warn!(
                event = error_event,
                branch = error.branch,
                error = error.message
            );
        }
        self.state.bulk_errors = errors;

        if count > 0 || !self.state.bulk_errors.is_empty() {
            self.state.refresh_sessions();
        }
        cx.notify();
    }

    /// Clear bulk operation errors (called when user dismisses the banner).
    fn on_dismiss_bulk_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.bulk_errors.dismissed");
        self.state.clear_bulk_errors();
        cx.notify();
    }

    /// Render a bulk operation button with consistent styling.
    fn render_bulk_button(
        &self,
        id: &'static str,
        label: &str,
        count: usize,
        enabled_bg: u32,
        enabled_hover: u32,
        on_click: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        let is_disabled = count == 0;
        let bg_color = if is_disabled {
            rgb(0x333333)
        } else {
            rgb(enabled_bg)
        };
        let hover_color = if is_disabled {
            rgb(0x333333)
        } else {
            rgb(enabled_hover)
        };
        let text_color = if is_disabled {
            rgb(0x666666)
        } else {
            rgb(0xffffff)
        };

        div()
            .id(id)
            .px_3()
            .py_1()
            .bg(bg_color)
            .when(!is_disabled, |d| d.hover(|style| style.bg(hover_color)))
            .rounded_md()
            .when(!is_disabled, |d| d.cursor_pointer())
            .when(!is_disabled, |d| {
                d.on_mouse_up(gpui::MouseButton::Left, on_click)
            })
            .child(
                div()
                    .text_color(text_color)
                    .child(format!("{} ({})", label, count)),
            )
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
                match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => {
                        self.state.create_form.branch_name.pop();
                    }
                    CreateDialogField::Note => {
                        self.state.create_form.note.pop();
                    }
                    CreateDialogField::Agent => {}
                }
                cx.notify();
            }
            "enter" => {
                self.on_dialog_submit(cx);
            }
            "escape" => {
                self.on_dialog_cancel(cx);
            }
            "space" => {
                match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => {
                        // Convert spaces to hyphens for branch names
                        self.state.create_form.branch_name.push('-');
                    }
                    CreateDialogField::Note => {
                        // Allow actual spaces in notes
                        self.state.create_form.note.push(' ');
                    }
                    CreateDialogField::Agent => {}
                }
                cx.notify();
            }
            "tab" => {
                // Cycle focus between fields
                self.state.create_form.focused_field = match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => CreateDialogField::Agent,
                    CreateDialogField::Agent => CreateDialogField::Note,
                    CreateDialogField::Note => CreateDialogField::BranchName,
                };
                cx.notify();
            }
            key if key.len() == 1 => {
                if let Some(c) = key.chars().next() {
                    match self.state.create_form.focused_field {
                        CreateDialogField::BranchName => {
                            // Branch names: alphanumeric, -, _, /
                            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                                self.state.create_form.branch_name.push(c);
                                cx.notify();
                            }
                        }
                        CreateDialogField::Note => {
                            // Notes: any non-control character
                            if !c.is_control() {
                                self.state.create_form.note.push(c);
                                cx.notify();
                            }
                        }
                        CreateDialogField::Agent => {
                            // Agent field uses click/tab to cycle, not typed input
                        }
                    }
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
                            // Open All button - green when enabled
                            .child(self.render_bulk_button(
                                "open-all-btn",
                                "Open All",
                                self.state.stopped_count(),
                                0x446644,
                                0x557755,
                                cx.listener(|view, _, _, cx| view.on_open_all_click(cx)),
                            ))
                            // Stop All button - red when enabled
                            .child(self.render_bulk_button(
                                "stop-all-btn",
                                "Stop All",
                                self.state.running_count(),
                                0x664444,
                                0x775555,
                                cx.listener(|view, _, _, cx| view.on_stop_all_click(cx)),
                            ))
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
            // Bulk operation errors banner (dismissible)
            .when(!self.state.bulk_errors.is_empty(), |this| {
                let error_count = self.state.bulk_errors.len();
                this.child(
                    div()
                        .mx_4()
                        .mt_2()
                        .px_4()
                        .py_2()
                        .bg(rgb(0x662222))
                        .rounded_md()
                        .flex()
                        .flex_col()
                        .gap_1()
                        // Header with dismiss button
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(rgb(0xff6b6b))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "{} operation{} failed:",
                                            error_count,
                                            if error_count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(
                                    div()
                                        .id("dismiss-bulk-errors")
                                        .px_2()
                                        .cursor_pointer()
                                        .text_color(rgb(0xaaaaaa))
                                        .hover(|style| style.text_color(rgb(0xffffff)))
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(|view, _, _, cx| {
                                                view.on_dismiss_bulk_errors(cx);
                                            }),
                                        )
                                        .child("×"),
                                ),
                        )
                        // Error list
                        .children(self.state.bulk_errors.iter().map(|e| {
                            div()
                                .text_sm()
                                .text_color(rgb(0xffaaaa))
                                .child(format!("• {}: {}", e.branch, e.message))
                        })),
                )
            })
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
