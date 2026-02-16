//! Terminal tab management for kild-ui.
//!
//! Extracted from `main_view.rs` — owns `TerminalBackend`, `TabEntry`,
//! `TerminalTabs` (per-kild tab set), tab bar rendering, and the
//! `adjust_active_after_close` helper.

use gpui::{App, Context, IntoElement, div, prelude::*, px};

use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};

use crate::terminal::TerminalView;
use crate::theme;
use crate::views::main_view::MainView;

/// Tracks how a terminal tab was created.
#[allow(dead_code)]
pub enum TerminalBackend {
    Local,
    Daemon {
        daemon_session_id: String,
    },
    Teammate {
        daemon_session_id: String,
        teammate_name: String,
        color: kild_teams::TeamColor,
    },
}

/// A single terminal tab within a kild's tab bar.
pub struct TabEntry {
    view: gpui::Entity<TerminalView>,
    label: String,
    backend: TerminalBackend,
}

impl TabEntry {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn backend(&self) -> &TerminalBackend {
        &self.backend
    }

    pub fn view(&self) -> &gpui::Entity<TerminalView> {
        &self.view
    }
}

/// Per-kild collection of terminal tabs with cycling and close logic.
pub struct TerminalTabs {
    tabs: Vec<TabEntry>,
    active: usize,
    next_id: usize,
}

impl Default for TerminalTabs {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }
}

impl TerminalTabs {
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Set the active tab index. No-op with warning if out of bounds.
    pub fn set_active(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
        } else {
            tracing::warn!(
                event = "ui.terminal_tabs.set_active_oob",
                idx = idx,
                len = self.tabs.len()
            );
        }
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn get(&self, idx: usize) -> Option<&TabEntry> {
        self.tabs.get(idx)
    }

    pub fn active_view(&self) -> Option<&gpui::Entity<TerminalView>> {
        self.tabs.get(self.active).map(|e| &e.view)
    }

    pub fn push(&mut self, view: gpui::Entity<TerminalView>, backend: TerminalBackend) {
        let base = format!("Shell {}", self.next_id);
        let label = match &backend {
            TerminalBackend::Local => base,
            TerminalBackend::Daemon { .. } => format!("D • {}", base),
            TerminalBackend::Teammate { teammate_name, .. } => teammate_name.clone(),
        };
        tracing::debug!(
            event = "ui.terminal_tabs.push",
            label = label,
            new_len = self.tabs.len() + 1
        );
        self.tabs.push(TabEntry {
            view,
            label,
            backend,
        });
        self.active = self.tabs.len() - 1;
        self.next_id += 1;
    }

    /// Push a teammate terminal tab. Does NOT steal focus (teammates appear in background).
    pub fn push_teammate(
        &mut self,
        view: gpui::Entity<TerminalView>,
        name: String,
        color: kild_teams::TeamColor,
        daemon_session_id: String,
    ) {
        tracing::debug!(
            event = "ui.terminal_tabs.push_teammate",
            name = name,
            daemon_session_id = daemon_session_id,
            new_len = self.tabs.len() + 1
        );
        self.tabs.push(TabEntry {
            view,
            label: name.clone(),
            backend: TerminalBackend::Teammate {
                daemon_session_id,
                teammate_name: name,
                color,
            },
        });
        // Don't change self.active — teammate tabs don't steal focus
        self.next_id += 1;
    }

    /// Check if a tab with the given daemon session ID already exists.
    pub fn has_daemon_session(&self, daemon_session_id: &str) -> bool {
        self.tabs.iter().any(|tab| match &tab.backend {
            TerminalBackend::Daemon {
                daemon_session_id: id,
            } => id == daemon_session_id,
            TerminalBackend::Teammate {
                daemon_session_id: id,
                ..
            } => id == daemon_session_id,
            TerminalBackend::Local => false,
        })
    }

    /// Close a tab at `idx`. Returns the daemon session ID if the closed tab
    /// was daemon-backed (so the caller can stop it asynchronously).
    pub fn close(&mut self, idx: usize) -> Option<String> {
        if idx >= self.tabs.len() {
            tracing::warn!(
                event = "ui.terminal_tabs.close_oob",
                idx = idx,
                len = self.tabs.len()
            );
            return None;
        }
        let entry = &self.tabs[idx];
        let daemon_id = match &entry.backend {
            TerminalBackend::Local => {
                tracing::debug!(
                    event = "ui.terminal_tabs.close",
                    idx = idx,
                    backend = "local",
                    remaining = self.tabs.len() - 1
                );
                None
            }
            TerminalBackend::Daemon { daemon_session_id } => {
                tracing::debug!(
                    event = "ui.terminal_tabs.close",
                    idx = idx,
                    backend = "daemon",
                    daemon_session_id = daemon_session_id,
                    remaining = self.tabs.len() - 1
                );
                Some(daemon_session_id.clone())
            }
            TerminalBackend::Teammate {
                teammate_name,
                daemon_session_id,
                ..
            } => {
                // Teammate tabs don't stop the daemon session — shim owns the lifecycle.
                tracing::debug!(
                    event = "ui.terminal_tabs.close",
                    idx = idx,
                    backend = "teammate",
                    teammate = teammate_name,
                    daemon_session_id = daemon_session_id,
                    remaining = self.tabs.len() - 1
                );
                None
            }
        };
        self.tabs.remove(idx);
        self.active = adjust_active_after_close(self.active, idx, self.tabs.len());
        daemon_id
    }

    pub fn cycle_next(&mut self) {
        assert!(
            self.tabs.is_empty() || self.active < self.tabs.len(),
            "invariant violated: active={}, len={}",
            self.active,
            self.tabs.len()
        );
        if self.tabs.len() > 1 {
            self.active = (self.active + 1) % self.tabs.len();
            tracing::debug!(event = "ui.terminal_tabs.cycle_next", active = self.active);
        }
    }

    pub fn cycle_prev(&mut self) {
        assert!(
            self.tabs.is_empty() || self.active < self.tabs.len(),
            "invariant violated: active={}, len={}",
            self.active,
            self.tabs.len()
        );
        if self.tabs.len() > 1 {
            self.active = self.active.checked_sub(1).unwrap_or(self.tabs.len() - 1);
            tracing::debug!(event = "ui.terminal_tabs.cycle_prev", active = self.active);
        }
    }

    pub fn rename(&mut self, idx: usize, name: String) {
        if let Some(entry) = self.tabs.get_mut(idx) {
            tracing::debug!(
                event = "ui.terminal_tabs.rename",
                idx = idx,
                old = entry.label,
                new = name
            );
            entry.label = name;
        } else {
            tracing::warn!(
                event = "ui.terminal_tabs.rename_oob",
                idx = idx,
                len = self.tabs.len()
            );
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn has_exited_active(&self, cx: &App) -> bool {
        self.active_view()
            .is_some_and(|v| v.read(cx).terminal().has_exited())
    }
}

/// State for an in-progress tab rename operation.
pub struct RenamingTab<'a> {
    pub session_id: &'a str,
    pub tab_index: usize,
    pub input: &'a gpui::Entity<InputState>,
}

/// Data needed to render the tab bar, extracted from MainView fields.
#[allow(dead_code)]
pub struct TabBarContext<'a> {
    pub tabs: &'a TerminalTabs,
    pub session_id: &'a str,
    pub renaming_tab: Option<RenamingTab<'a>>,
    pub show_add_menu: bool,
    pub daemon_available: Option<bool>,
    pub daemon_starting: bool,
}

/// Render the tab bar for a kild's terminal pane.
///
/// Uses `cx.listener()` closures that dispatch back to `MainView` methods.
#[allow(dead_code)]
pub fn render_tab_bar(ctx: &TabBarContext, cx: &mut Context<MainView>) -> gpui::AnyElement {
    let tabs = ctx.tabs;
    let session_id = ctx.session_id;
    let session_id_owned = session_id.to_string();

    div()
        .flex()
        .items_center()
        .px(px(theme::SPACE_2))
        .py(px(theme::SPACE_1))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::border_subtle())
        .gap(px(theme::SPACE_1))
        .children(tabs.tabs.iter().enumerate().map(|(idx, entry)| {
            let is_active = idx == tabs.active;
            let close_sid = session_id_owned.clone();
            let select_sid = session_id_owned.clone();

            let is_renaming = ctx
                .renaming_tab
                .as_ref()
                .is_some_and(|r| r.session_id == session_id && r.tab_index == idx);

            if is_renaming {
                let Some(renaming) = ctx.renaming_tab.as_ref() else {
                    // Invariant: is_renaming was true but renaming_tab is None — logic error.
                    // Fall through to render as normal tab instead of panicking.
                    tracing::error!(
                        event = "ui.terminal_tabs.renaming_state_inconsistent",
                        idx = idx,
                        "is_renaming was true but renaming_tab is None"
                    );
                    let label = entry.label.clone();
                    return div()
                        .text_size(px(theme::TEXT_SM))
                        .child(label)
                        .into_any_element();
                };
                let input_state = renaming.input.clone();
                return div()
                    .flex()
                    .items_center()
                    .min_w(px(120.0))
                    .px(px(theme::SPACE_2))
                    .py(px(2.0))
                    .rounded(px(theme::RADIUS_SM))
                    .bg(theme::elevated())
                    .border_b_2()
                    .border_color(theme::ice())
                    .text_size(px(theme::TEXT_SM))
                    .child(Input::new(&input_state).cleanable(false))
                    .into_any_element();
            }

            let label = entry.label.clone();

            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .px(px(theme::SPACE_2))
                .py(px(2.0))
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .when(is_active, |d| {
                    d.bg(theme::elevated())
                        .text_color(theme::text_bright())
                        .border_b_2()
                        .border_color(theme::ice())
                })
                .when(!is_active, |d| {
                    d.text_color(theme::text_muted())
                        .hover(|d| d.text_color(theme::text()))
                })
                .text_size(px(theme::TEXT_SM))
                .child(label)
                .child(
                    div()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::ember()))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, window, cx| {
                                view.on_close_tab(&close_sid, idx, window, cx);
                            }),
                        )
                        .child("×"),
                )
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        view.on_select_tab(&select_sid, idx, window, cx);
                    }),
                )
                .into_any_element()
        }))
        .child({
            let sid = session_id_owned.clone();
            let sid2 = session_id_owned.clone();
            let daemon_enabled = ctx.daemon_available.unwrap_or(false);
            let daemon_starting = ctx.daemon_starting;

            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .child(
                    div()
                        .text_size(px(theme::TEXT_SM))
                        .text_color(theme::text_muted())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::ice()))
                        .px(px(theme::SPACE_2))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, _, cx| {
                                view.show_add_menu = !view.show_add_menu;
                                if view.show_add_menu {
                                    view.refresh_daemon_available(cx);
                                }
                                cx.notify();
                            }),
                        )
                        .child("+"),
                )
                .when(ctx.show_add_menu, |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(theme::SPACE_1))
                            .px(px(theme::SPACE_2))
                            .py(px(2.0))
                            .rounded(px(theme::RADIUS_SM))
                            .bg(theme::elevated())
                            .child(
                                Button::new("add-local-tab")
                                    .label("Local")
                                    .ghost()
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.on_add_local_tab(&sid, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("add-daemon-tab")
                                    .label("Daemon")
                                    .ghost()
                                    .disabled(!daemon_enabled)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.on_add_daemon_tab(&sid2, cx);
                                    })),
                            )
                            .when(!daemon_enabled, |this| {
                                this.child(
                                    Button::new("start-daemon-menu")
                                        .label(if daemon_starting {
                                            "Starting…"
                                        } else {
                                            "Start Daemon"
                                        })
                                        .ghost()
                                        .disabled(daemon_starting)
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.on_start_daemon(cx);
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(theme::TEXT_XS))
                                    .text_color(theme::text_muted())
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(theme::ember()))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |view, _, _, cx| {
                                            view.show_add_menu = false;
                                            cx.notify();
                                        }),
                                    )
                                    .child("×"),
                            ),
                    )
                })
        })
        .into_any_element()
}

/// Compute the new active index after closing a tab.
pub fn adjust_active_after_close(active: usize, closed: usize, new_len: usize) -> usize {
    if new_len == 0 {
        0
    } else if active >= new_len {
        new_len - 1
    } else if active > closed {
        active - 1
    } else {
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- adjust_active_after_close tests ---

    #[test]
    fn test_adjust_active_close_only_tab() {
        assert_eq!(adjust_active_after_close(0, 0, 0), 0);
    }

    #[test]
    fn test_adjust_active_close_active_in_middle() {
        // [A, B, C], active=1, close=1 → active stays 1 (now pointing to former C)
        assert_eq!(adjust_active_after_close(1, 1, 2), 1);
    }

    #[test]
    fn test_adjust_active_close_before_active() {
        // [A, B, C], active=2, close=0 → active becomes 1
        assert_eq!(adjust_active_after_close(2, 0, 2), 1);
    }

    #[test]
    fn test_adjust_active_close_after_active() {
        // [A, B, C], active=0, close=2 → active stays 0
        assert_eq!(adjust_active_after_close(0, 2, 2), 0);
    }

    #[test]
    fn test_adjust_active_close_last_when_active_is_last() {
        // [A, B, C], active=2, close=2 → active becomes 1 (new last)
        assert_eq!(adjust_active_after_close(2, 2, 2), 1);
    }

    #[test]
    fn test_adjust_active_close_first_of_two() {
        // [A, B], active=0, close=0 → active stays 0
        assert_eq!(adjust_active_after_close(0, 0, 1), 0);
    }

    #[test]
    fn test_adjust_active_close_second_of_two_when_active() {
        // [A, B], active=1, close=1 → active becomes 0
        assert_eq!(adjust_active_after_close(1, 1, 1), 0);
    }

    // --- TerminalTabs unit tests (pure logic, no GPUI entities) ---

    #[test]
    fn test_terminal_tabs_default_is_empty() {
        let tabs = TerminalTabs::default();
        assert!(tabs.is_empty());
        assert_eq!(tabs.active_index(), 0);
        assert_eq!(tabs.len(), 0);
        assert!(tabs.active_view().is_none());
    }

    #[test]
    fn test_set_active_oob_is_noop() {
        let mut tabs = TerminalTabs::default();
        tabs.set_active(5); // out of bounds on empty tabs
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn test_cycle_next_empty_is_noop() {
        let mut tabs = TerminalTabs::default();
        tabs.cycle_next(); // should not panic
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn test_cycle_prev_empty_is_noop() {
        let mut tabs = TerminalTabs::default();
        tabs.cycle_prev(); // should not panic
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn test_close_oob_returns_none() {
        let mut tabs = TerminalTabs::default();
        assert_eq!(tabs.close(99), None);
    }

    #[test]
    fn test_get_oob_returns_none() {
        let tabs = TerminalTabs::default();
        assert!(tabs.get(0).is_none());
        assert!(tabs.get(99).is_none());
    }
}
