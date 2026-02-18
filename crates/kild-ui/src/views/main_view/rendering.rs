//! Rendering, keyboard input, and GPUI trait implementations for MainView.

use gpui::{
    Context, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, SharedString, Window, div,
    prelude::*, px,
};
use tracing::warn;

use gpui_component::button::{Button, ButtonVariants};

use crate::theme;
use crate::views::{
    add_project_dialog, confirm_dialog, create_dialog, dashboard_view, detail_view, project_rail,
    sidebar, status_bar,
    terminal_tabs::{RenamingTab, TabBarContext, render_tab_bar},
};

use super::main_view_def::MainView;
use super::types::{ActiveView, FocusRegion};

impl MainView {
    #[allow(dead_code)]
    pub(crate) fn render_tab_bar(
        &self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(tabs) = self.terminal_tabs.get(session_id) else {
            return div().into_any_element();
        };

        let renaming = self.renaming_tab.as_ref().map(|(s, i, input)| RenamingTab {
            session_id: s.as_str(),
            tab_index: *i,
            input,
        });

        let ctx = TabBarContext {
            tabs,
            session_id,
            renaming_tab: renaming,
            show_add_menu: self.show_add_menu,
            daemon_available: self.daemon_available,
            daemon_starting: self.daemon_starting,
        };
        render_tab_bar(&ctx, cx)
    }

    /// Render the view tab bar: [Control 1] [Control 2] [+] [Dashboard] ...
    fn render_view_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_control = self.active_view == ActiveView::Control;
        let is_dashboard = matches!(self.active_view, ActiveView::Dashboard | ActiveView::Detail);
        let workspace_count = self.workspaces.len();
        let active_ws = self.active_workspace;

        let mut bar = div()
            .flex()
            .items_center()
            .px(px(theme::SPACE_3))
            .bg(theme::obsidian())
            .border_b_1()
            .border_color(theme::border_subtle());

        // Workspace tabs
        for i in 0..workspace_count {
            let is_active_ws = is_control && i == active_ws;
            let is_inactive_ws = is_control && i != active_ws;
            let label = if workspace_count == 1 {
                "Control".to_string()
            } else {
                format!("Control {}", i + 1)
            };
            let tab_id = SharedString::from(format!("view-tab-workspace-{}", i));

            bar = bar.child(
                div()
                    .id(tab_id)
                    .px(px(theme::SPACE_3))
                    .py(px(theme::SPACE_2))
                    .cursor_pointer()
                    .text_size(px(theme::TEXT_XS))
                    .font_weight(FontWeight::MEDIUM)
                    .border_b_2()
                    .when(is_active_ws, |d| {
                        d.text_color(theme::text()).border_color(theme::ice())
                    })
                    .when(is_inactive_ws, |d| {
                        d.text_color(theme::text_subtle())
                            .border_color(theme::transparent())
                            .hover(|d| d.text_color(theme::text()))
                    })
                    .when(!is_control, |d| {
                        d.text_color(theme::text_muted())
                            .border_color(theme::transparent())
                            .hover(|d| d.text_color(theme::text_subtle()))
                    })
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(move |view, _, window, cx| {
                            if i < view.workspaces.len() {
                                view.active_workspace = i;
                                view.active_view = ActiveView::Control;
                                if view.active_terminal_id.is_some() {
                                    view.focus_region = FocusRegion::Terminal;
                                    view.focus_active_terminal(window, cx);
                                }
                            }
                            cx.notify();
                        }),
                    )
                    .child(label),
            );
        }

        // "+" button to add workspace
        bar = bar.child(
            div()
                .id("workspace-add")
                .px(px(theme::SPACE_2))
                .py(px(theme::SPACE_2))
                .cursor_pointer()
                .text_size(px(theme::TEXT_XS))
                .text_color(theme::text_muted())
                .hover(|d| d.text_color(theme::text()))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        if view.workspaces.len() >= Self::MAX_WORKSPACES {
                            warn!(
                                event = "ui.workspace.max_limit_reached",
                                max = Self::MAX_WORKSPACES,
                            );
                            return;
                        }
                        view.workspaces
                            .push(super::super::pane_grid::PaneGrid::new());
                        view.active_workspace = view.workspaces.len() - 1;
                        view.active_view = ActiveView::Control;
                        cx.notify();
                    }),
                )
                .child("+"),
        );

        // Dashboard tab
        bar = bar.child(
            div()
                .id("view-tab-dashboard")
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_2))
                .cursor_pointer()
                .text_size(px(theme::TEXT_XS))
                .font_weight(FontWeight::MEDIUM)
                .border_b_2()
                .when(is_dashboard, |d| {
                    d.text_color(theme::text()).border_color(theme::ice())
                })
                .when(!is_dashboard, |d| {
                    d.text_color(theme::text_muted())
                        .border_color(theme::transparent())
                        .hover(|d| d.text_color(theme::text_subtle()))
                })
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        if view.active_view != ActiveView::Dashboard {
                            view.active_view = ActiveView::Dashboard;
                            view.focus_region = FocusRegion::Dashboard;
                            cx.notify();
                        }
                    }),
                )
                .child("Dashboard"),
        );

        // Spacer
        bar.child(div().flex_1())
    }

    /// Render the main content area based on active view.
    fn render_main_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.active_view {
            ActiveView::Control => super::super::pane_grid::render_pane_grid(
                self.active_pane_grid(),
                &self.terminal_tabs,
                cx,
            )
            .into_any_element(),
            ActiveView::Dashboard => dashboard_view::render_dashboard(
                &self.state,
                &self.terminal_tabs,
                &self.team_manager,
                cx,
            ),
            ActiveView::Detail => {
                detail_view::render_detail_view(&self.state, &self.terminal_tabs, cx)
            }
        }
    }

    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::state::DialogState;

        let key_str = event.keystroke.key.to_string();

        // Tab rename mode: Enter commits, Escape cancels, all other keys go to Input
        if self.renaming_tab.is_some() {
            if key_str == "enter" {
                self.commit_rename(window, cx);
            } else if key_str == "escape" {
                self.cancel_rename(window, cx);
            }
            return;
        }

        // focus_escape binding: move focus from terminal to sidebar (terminal stays rendered)
        if self
            .keybindings
            .terminal
            .focus_escape
            .matches(&event.keystroke)
            && self.focus_region == FocusRegion::Terminal
        {
            self.focus_region = FocusRegion::Dashboard;
            self.show_add_menu = false;
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Ctrl+Tab / Ctrl+Shift+Tab: cycle terminal tabs
        if key_str == "tab" && event.keystroke.modifiers.control {
            let should_focus = if let Some(id) = &self.active_terminal_id {
                if let Some(tabs) = self.terminal_tabs.get_mut(id) {
                    if tabs.len() > 1 {
                        if event.keystroke.modifiers.shift {
                            tabs.cycle_prev();
                        } else {
                            tabs.cycle_next();
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if should_focus {
                self.focus_active_terminal(window, cx);
                cx.notify();
            }
            return;
        }

        // Ctrl+T: toggle terminal active/inactive within Control view
        if key_str == "t" && event.keystroke.modifiers.control {
            if matches!(self.active_view, ActiveView::Dashboard | ActiveView::Detail) {
                self.active_view = ActiveView::Control;
                if let Some(id) = self.state.selected_id().map(|s| s.to_string()) {
                    self.on_kild_select(&id, window, cx);
                    return;
                }
            } else if self.active_terminal_view().is_some() {
                self.active_terminal_id = None;
                self.focus_region = FocusRegion::Dashboard;
                window.focus(&self.focus_handle);
            } else if let Some(id) = self.state.selected_id().map(|s| s.to_string()) {
                self.on_kild_select(&id, window, cx);
                return;
            }
            cx.notify();
            return;
        }

        // Configurable modifier + 1-9: jump to kild by index
        if let Some(digit) = key_str
            .parse::<usize>()
            .ok()
            .filter(|&d| (1..=9).contains(&d))
            && self
                .keybindings
                .navigation
                .jump_modifier
                .matches(&event.keystroke.modifiers)
        {
            self.navigate_to_kild_index(digit - 1, window, cx);
            cx.notify();
            return;
        }

        // Cmd+Shift+J/K: cycle between teammate terminals in the tab bar (not configurable)
        let cmd = event.keystroke.modifiers.platform;
        if cmd && event.keystroke.modifiers.shift && key_str == "j" {
            self.navigate_next_teammate_tab(window, cx);
            cx.notify();
            return;
        }

        if cmd && event.keystroke.modifiers.shift && key_str == "k" {
            self.navigate_prev_teammate_tab(window, cx);
            cx.notify();
            return;
        }

        // prev_workspace / next_workspace bindings: cycle workspaces
        if self
            .keybindings
            .navigation
            .prev_workspace
            .matches(&event.keystroke)
        {
            if self.workspaces.len() > 1 {
                self.active_workspace = if self.active_workspace == 0 {
                    self.workspaces.len() - 1
                } else {
                    self.active_workspace - 1
                };
                self.active_view = ActiveView::Control;
                tracing::debug!(
                    event = "ui.workspace.cycle_prev",
                    workspace = self.active_workspace,
                );
            }
            cx.notify();
            return;
        }

        if self
            .keybindings
            .navigation
            .next_workspace
            .matches(&event.keystroke)
        {
            if self.workspaces.len() > 1 {
                self.active_workspace = (self.active_workspace + 1) % self.workspaces.len();
                self.active_view = ActiveView::Control;
                tracing::debug!(
                    event = "ui.workspace.cycle_next",
                    workspace = self.active_workspace,
                );
            }
            cx.notify();
            return;
        }

        if self
            .keybindings
            .navigation
            .next_kild
            .matches(&event.keystroke)
        {
            self.navigate_next_kild(window, cx);
            cx.notify();
            return;
        }

        if self
            .keybindings
            .navigation
            .prev_kild
            .matches(&event.keystroke)
        {
            self.navigate_prev_kild(window, cx);
            cx.notify();
            return;
        }

        if self
            .keybindings
            .navigation
            .toggle_view
            .matches(&event.keystroke)
        {
            self.toggle_view(window, cx);
            return;
        }

        // Escape in Detail view: back to Dashboard
        if key_str == "escape" && self.active_view == ActiveView::Detail {
            self.active_view = ActiveView::Dashboard;
            self.focus_region = FocusRegion::Dashboard;
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Propagate keys to terminal only when Control view is active, terminal exists,
        // and terminal has focus. Without these guards, keys would reach a non-visible terminal.
        if self.active_view == ActiveView::Control
            && self.focus_region == FocusRegion::Terminal
            && self.active_terminal_view().is_some()
        {
            cx.propagate();
            return;
        }

        match self.state.dialog() {
            DialogState::None => {}
            DialogState::Confirm { .. } => {
                if key_str == "escape" {
                    self.on_confirm_cancel(cx);
                }
            }
            DialogState::AddProject { .. } => {
                if key_str == "escape" {
                    self.on_add_project_cancel(cx);
                }
            }
            DialogState::Create { .. } => match key_str.as_str() {
                "escape" => self.on_dialog_cancel(cx),
                "enter" => self.on_dialog_submit(cx),
                "tab" => self.on_agent_cycle(cx),
                _ => {}
            },
        }
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &gpui::App) -> gpui::FocusHandle {
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
            .bg(theme::void())
            // Error banner (shown for startup failures, project errors, state desync recovery)
            .when(self.state.has_banner_errors(), |this| {
                let errors = self.state.banner_errors();
                let error_count = errors.len();
                this.child(
                    div()
                        .mx(px(theme::SPACE_4))
                        .mt(px(theme::SPACE_2))
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_2))
                        .bg(theme::with_alpha(theme::ember(), 0.15))
                        .rounded(px(theme::RADIUS_MD))
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        // Header with dismiss button
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(theme::ember())
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "Error{}:",
                                            if error_count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(Button::new("dismiss-errors").label("×").ghost().on_click(
                                    cx.listener(|view, _, _, cx| {
                                        view.on_dismiss_errors(cx);
                                    }),
                                )),
                        )
                        // Error list
                        .children(errors.iter().map(|e| {
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::with_alpha(theme::ember(), 0.8))
                                .child(format!("• {}", e))
                        })),
                )
            })
            // Main content: Rail | Right section (Sidebar + Main + Status Bar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // Project rail (48px, spans full height)
                    .child(project_rail::render_project_rail(&self.state, cx))
                    // Right section: sidebar + main + status bar
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Content row: sidebar + main
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .overflow_hidden()
                                    // Sidebar (200px, kild navigation)
                                    .child(sidebar::render_sidebar(
                                        &self.state,
                                        &self.terminal_tabs,
                                        self.active_pane_grid(),
                                        cx,
                                    ))
                                    // Main area (flex-1)
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .overflow_hidden()
                                            // View tab bar: [Control] [Dashboard]
                                            .child(self.render_view_tab_bar(cx))
                                            // View content
                                            .child(self.render_main_content(cx)),
                                    ),
                            )
                            // Status bar (spans sidebar + main, NOT rail)
                            .child(status_bar::render_status_bar(
                                &self.state,
                                self.active_view,
                                &self.keybindings,
                                cx,
                            )),
                    ),
            )
            // Dialog rendering (based on current dialog state)
            .when(self.state.dialog().is_create(), |this| {
                this.child(create_dialog::render_create_dialog(
                    self.state.dialog(),
                    self.state.is_dialog_loading(),
                    self.branch_input.as_ref(),
                    self.note_input.as_ref(),
                    cx,
                ))
            })
            .when(self.state.dialog().is_confirm(), |this| {
                this.child(confirm_dialog::render_confirm_dialog(
                    self.state.dialog(),
                    self.state.is_dialog_loading(),
                    cx,
                ))
            })
            .when(self.state.dialog().is_add_project(), |this| {
                this.child(add_project_dialog::render_add_project_dialog(
                    self.state.dialog(),
                    self.path_input.as_ref(),
                    self.name_input.as_ref(),
                    cx,
                ))
            })
    }
}
