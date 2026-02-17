//! Project management handlers for MainView.

use std::path::PathBuf;

use gpui::{Context, Window, prelude::*};
use gpui_component::input::InputState;

use crate::actions;

use super::main_view_def::MainView;
use super::path_utils::normalize_project_path;

impl MainView {
    /// Handle click on Add Project button.
    pub fn on_add_project_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.opened");
        self.state.open_add_project_dialog();

        let path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/repository"));
        self.path_input = Some(path_input);

        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Defaults to directory name"));
        self.name_input = Some(name_input);

        cx.notify();
    }

    /// Handle add project dialog cancel.
    pub fn on_add_project_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.cancelled");
        self.clear_input_entities();
        self.mutate_state(cx, |s| s.close_dialog());
    }

    /// Handle add project dialog submit.
    pub fn on_add_project_submit(&mut self, cx: &mut Context<Self>) {
        if !self.state.dialog().is_add_project() {
            tracing::error!(
                event = "ui.add_project_submit.invalid_state",
                "on_add_project_submit called when AddProject dialog not open"
            );
            return;
        }

        // Read text values from InputState entities
        let path_str = self
            .path_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let path_str = path_str.trim().to_string();
        let name_str = self
            .name_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let name = if name_str.trim().is_empty() {
            None
        } else {
            Some(name_str.trim().to_string())
        };

        if path_str.is_empty() {
            self.state
                .set_dialog_error("Path cannot be empty".to_string());
            cx.notify();
            return;
        }

        // Normalize path: expand ~ and ensure absolute path
        let path = match normalize_project_path(&path_str) {
            Ok(p) => p,
            Err(e) => {
                self.state.set_dialog_error(e);
                cx.notify();
                return;
            }
        };

        match actions::dispatch_add_project(path.clone(), name) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.add_project.error_displayed",
                    path = %path.display(),
                    error = %e
                );
                self.state.set_dialog_error(e);
            }
        }
        cx.notify();
    }

    /// Handle project selection from sidebar.
    pub fn on_project_select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.project_selected",
            path = %path.display()
        );

        match actions::dispatch_set_active_project(Some(path)) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
                self.reset_pane_grid();
            }
            Err(e) => {
                tracing::error!(event = "ui.project_select.failed", error = %e);
                self.state
                    .push_error(format!("Failed to select project: {}", e));
            }
        }
        cx.notify();
    }

    /// Handle "All Projects" selection from sidebar.
    pub fn on_project_select_all(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.project_selected_all");

        match actions::dispatch_set_active_project(None) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
                self.reset_pane_grid();
            }
            Err(e) => {
                tracing::error!(event = "ui.project_select_all.failed", error = %e);
                self.state
                    .push_error(format!("Failed to update project selection: {}", e));
            }
        }
        cx.notify();
    }

    /// Handle remove project from list.
    #[allow(dead_code)]
    pub fn on_remove_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.remove_project.started",
            path = %path.display()
        );

        match actions::dispatch_remove_project(path) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
            }
            Err(e) => {
                tracing::error!(event = "ui.remove_project.failed", error = %e);
                self.state
                    .push_error(format!("Failed to remove project: {}", e));
            }
        }
        cx.notify();
    }
}
