//! Create and destroy dialog handlers for MainView.

use gpui::{Context, Window, prelude::*};
use gpui_component::input::InputState;

use crate::actions;
use crate::views::create_dialog;

use super::main_view_def::MainView;

impl MainView {
    /// Handle click on the Create button in header.
    pub(crate) fn on_create_button_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.opened");
        self.state.open_create_dialog();

        let branch_pattern =
            regex::Regex::new(r"^[a-zA-Z0-9\-_/]*$").expect("branch name regex is valid");
        let branch_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type branch name...")
                .pattern(branch_pattern)
        });
        self.branch_input = Some(branch_input);

        let note_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("What is this kild for?")
                .validate(|text, _| !text.chars().any(|c| c.is_control()))
        });
        self.note_input = Some(note_input);

        cx.notify();
    }

    /// Handle dialog cancel button click (create dialog).
    pub fn on_dialog_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.cancelled");
        self.clear_input_entities();
        self.mutate_state(cx, |s| s.close_dialog());
    }

    /// Handle dialog submit button click (create dialog).
    ///
    /// Spawns the blocking create_kild operation on the background executor
    /// so the UI remains responsive during git worktree creation and terminal spawn.
    pub fn on_dialog_submit(&mut self, cx: &mut Context<Self>) {
        if self.state.is_dialog_loading() {
            return;
        }

        // Extract agent from dialog state
        let crate::state::DialogState::Create { form, .. } = self.state.dialog() else {
            tracing::error!(
                event = "ui.dialog_submit.invalid_state",
                "on_dialog_submit called when Create dialog not open"
            );
            return;
        };
        let agent = form.selected_agent();

        // Read text values from InputState entities
        let branch = self
            .branch_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let branch = branch.trim().to_string();
        let note_text = self
            .note_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let note = if note_text.trim().is_empty() {
            None
        } else {
            Some(note_text.trim().to_string())
        };

        // Get active project path for kild creation context
        let project_path = self.state.active_project_path().map(|p| p.to_path_buf());

        // Warn if no project selected (shouldn't happen with current UI flow)
        if project_path.is_none() {
            tracing::warn!(
                event = "ui.dialog_submit.no_active_project",
                message = "Creating kild without active project - will will use cwd detection"
            );
        }

        self.state.set_dialog_loading();
        cx.notify();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { actions::create_kild(branch, agent, note, project_path) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_dialog_loading();
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.dialog_submit.error_displayed", error = %e);
                        view.state.set_dialog_error(e);
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.dialog_submit.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Cycle to the next agent in the list.
    pub fn on_agent_cycle(&mut self, cx: &mut Context<Self>) {
        let agents = create_dialog::agent_options();
        if agents.is_empty() {
            tracing::error!(event = "ui.create_dialog.no_agents_available");
            self.state.set_dialog_error(
                "No agents available. Check kild-core configuration.".to_string(),
            );
            cx.notify();
            return;
        }

        // Update selected agent index in dialog state
        if let crate::state::DialogState::Create { form, .. } = self.state.dialog_mut() {
            let next_index = (form.selected_agent_index() + 1) % agents.len();
            form.set_selected_agent_index(next_index);
            tracing::info!(
                event = "ui.create_dialog.agent_changed",
                agent = %form.selected_agent()
            );
        }
        cx.notify();
    }

    /// Handle click on the destroy button [×] in a kild row.
    #[allow(dead_code)]
    pub fn on_destroy_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.destroy_dialog.opened", branch = branch);
        let branch = branch.to_string();
        self.mutate_state(cx, |s| s.open_confirm_dialog(branch));
    }

    /// Handle confirm button click in destroy dialog.
    ///
    /// Spawns the blocking destroy_kild operation on the background executor
    /// so the UI remains responsive during worktree removal and process termination.
    pub fn on_confirm_destroy(&mut self, cx: &mut Context<Self>) {
        if self.state.is_dialog_loading() {
            return;
        }

        // Extract branch and safety_info from dialog state
        let crate::state::DialogState::Confirm {
            branch,
            safety_info,
            ..
        } = self.state.dialog()
        else {
            tracing::warn!(event = "ui.confirm_destroy.no_target");
            return;
        };
        let branch = branch.clone();

        // Use force=true if safety_info indicates blocking (user clicked "Force Destroy")
        let force = safety_info
            .as_ref()
            .map(|s| s.should_block())
            .unwrap_or(false);

        self.state.set_dialog_loading();
        cx.notify();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { actions::destroy_kild(branch, force) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_dialog_loading();
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.confirm_destroy.error_displayed", error = %e);
                        view.state.set_dialog_error(e);
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.confirm_destroy.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Handle cancel button click in destroy dialog.
    pub fn on_confirm_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.confirm_dialog.cancelled");
        self.mutate_state(cx, |s| s.close_dialog());
    }
}
