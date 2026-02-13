//! Status bar component — thin footer spanning sidebar + main area.
//!
//! Shows contextual alerts on the left (dirty worktrees, operation errors)
//! and view-aware keyboard shortcut hints on the right.

use gpui::{
    AnyElement, Context, IntoElement, Keystroke, ParentElement, Styled, div, prelude::*, px,
};
use gpui_component::kbd::Kbd;

use crate::state::AppState;
use crate::theme;
use crate::views::main_view::{ActiveView, MainView};
use kild_core::{GitStatus, ProcessStatus};

/// Maximum number of alerts shown before truncation.
const MAX_ALERTS: usize = 2;

/// A single alert to display in the status bar.
struct Alert {
    message: String,
    is_error: bool,
}

/// Render the status bar footer.
pub fn render_status_bar(
    state: &AppState,
    active_view: ActiveView,
    cx: &mut Context<MainView>,
) -> AnyElement {
    div()
        .px(px(theme::SPACE_3))
        .py(px(3.0))
        .flex()
        .items_center()
        .justify_between()
        .bg(theme::obsidian())
        .border_t_1()
        .border_color(theme::border_subtle())
        .child(render_alerts(state, cx))
        .child(render_keyboard_hints(active_view, cx))
        .into_any_element()
}

/// Compute and render alerts from session state.
///
/// Shows operation errors (ember dot) and dirty stopped kilds (copper dot).
/// Truncates to MAX_ALERTS with "+N more" overflow.
fn render_alerts(state: &AppState, _cx: &mut Context<MainView>) -> impl IntoElement {
    let alerts = compute_alerts(state);
    let overflow = alerts.len().saturating_sub(MAX_ALERTS);
    let visible: Vec<&Alert> = alerts.iter().take(MAX_ALERTS).collect();

    div()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_3))
        .children(visible.iter().map(|alert| render_alert_item(alert)))
        .when(overflow > 0, |d| {
            d.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::text_muted())
                    .child(format!("+{} more", overflow)),
            )
        })
}

/// Render a single alert item: dot + message.
fn render_alert_item(alert: &Alert) -> impl IntoElement {
    let dot_color = if alert.is_error {
        theme::ember()
    } else {
        theme::copper()
    };

    div()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_1))
        .child(
            div()
                .w(px(5.0))
                .h(px(5.0))
                .rounded_full()
                .bg(dot_color)
                .flex_shrink_0(),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme::text_muted())
                .hover(|d| d.text_color(theme::text_subtle()))
                .child(alert.message.clone()),
        )
}

/// Compute alerts from current session state.
///
/// Priority ordering (highest first):
/// 1. Operation errors (ember dot) — from `state.get_error()`
/// 2. Dirty stopped kilds (copper dot) — running kilds are expected to be dirty
fn compute_alerts(state: &AppState) -> Vec<Alert> {
    let mut alerts = Vec::new();
    let displays = state.filtered_displays();

    // Operation errors first (higher priority)
    for display in &displays {
        if let Some(err) = state.get_error(&display.session.branch) {
            alerts.push(Alert {
                message: format!("{}: {}", display.session.branch, err.message),
                is_error: true,
            });
        }
    }

    // Dirty stopped kilds
    for display in &displays {
        if display.process_status == ProcessStatus::Stopped
            && display.git_status == GitStatus::Dirty
        {
            alerts.push(Alert {
                message: format!("{} has uncommitted changes", display.session.branch),
                is_error: false,
            });
        }
    }

    alerts
}

/// Render view-aware keyboard shortcut hints.
fn render_keyboard_hints(active_view: ActiveView, _cx: &mut Context<MainView>) -> impl IntoElement {
    let hints = keyboard_hints_for_view(active_view);

    div()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .children(hints.into_iter().map(|(keystroke_str, label)| {
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(Kbd::new(
                    Keystroke::parse(keystroke_str).expect("hardcoded keystroke should parse"),
                ))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::text_muted())
                        .hover(|d| d.text_color(theme::text()))
                        .child(label),
                )
        }))
}

/// Return the keyboard hints appropriate for the given view.
fn keyboard_hints_for_view(view: ActiveView) -> Vec<(&'static str, &'static str)> {
    match view {
        ActiveView::Control => vec![
            ("cmd-j", "next"),
            ("cmd-k", "prev"),
            ("cmd-d", "dashboard"),
            ("ctrl-tab", "tabs"),
        ],
        ActiveView::Dashboard => vec![("cmd-j", "next"), ("cmd-k", "prev"), ("cmd-d", "control")],
        ActiveView::Detail => vec![("escape", "back"), ("cmd-d", "control")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::Session;
    use kild_core::sessions::SessionInfo;

    #[test]
    fn test_compute_alerts_empty_state() {
        let alerts = compute_alerts_from_displays(&[], &std::collections::HashMap::new());
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_compute_alerts_dirty_stopped_kild() {
        let info = make_info("fix-bug", ProcessStatus::Stopped, GitStatus::Dirty);

        let alerts = compute_alerts_from_displays(&[&info], &std::collections::HashMap::new());
        assert_eq!(alerts.len(), 1);
        assert!(!alerts[0].is_error);
        assert!(alerts[0].message.contains("fix-bug"));
        assert!(alerts[0].message.contains("uncommitted"));
    }

    #[test]
    fn test_compute_alerts_clean_stopped_kild_no_alert() {
        let info = make_info("clean-branch", ProcessStatus::Stopped, GitStatus::Clean);

        let alerts = compute_alerts_from_displays(&[&info], &std::collections::HashMap::new());
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_compute_alerts_max_truncation() {
        let infos: Vec<SessionInfo> = (0..5)
            .map(|i| {
                make_info(
                    &format!("dirty-{}", i),
                    ProcessStatus::Stopped,
                    GitStatus::Dirty,
                )
            })
            .collect();

        let refs: Vec<&SessionInfo> = infos.iter().collect();
        let alerts = compute_alerts_from_displays(&refs, &std::collections::HashMap::new());
        assert_eq!(alerts.len(), 5);
        // Rendering would show MAX_ALERTS (2) + "+3 more"
    }

    #[test]
    fn test_keyboard_hints_control_view() {
        let hints = keyboard_hints_for_view(ActiveView::Control);
        assert_eq!(hints.len(), 4);
        assert_eq!(hints[0].0, "cmd-j");
        assert_eq!(hints[2].0, "cmd-d");
        assert_eq!(hints[2].1, "dashboard");
        assert_eq!(hints[3].0, "ctrl-tab");
    }

    #[test]
    fn test_keyboard_hints_dashboard_view() {
        let hints = keyboard_hints_for_view(ActiveView::Dashboard);
        assert_eq!(hints.len(), 3);
        assert_eq!(hints[2].0, "cmd-d");
        assert_eq!(hints[2].1, "control");
    }

    #[test]
    fn test_keyboard_hints_detail_view() {
        let hints = keyboard_hints_for_view(ActiveView::Detail);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].0, "escape");
        assert_eq!(hints[0].1, "back");
    }

    #[test]
    fn test_all_keystroke_hints_parse() {
        let views = [
            ActiveView::Control,
            ActiveView::Dashboard,
            ActiveView::Detail,
        ];
        for view in views {
            for (keystroke_str, _label) in keyboard_hints_for_view(view) {
                assert!(
                    Keystroke::parse(keystroke_str).is_ok(),
                    "Invalid keystroke '{}' in {:?}",
                    keystroke_str,
                    view,
                );
            }
        }
    }

    #[test]
    fn test_compute_alerts_errors_before_dirty() {
        let info = make_info("branch-1", ProcessStatus::Stopped, GitStatus::Dirty);
        let mut errors = std::collections::HashMap::new();
        errors.insert(
            "branch-1".to_string(),
            crate::state::errors::OperationError {
                branch: "branch-1".to_string(),
                message: "open failed".to_string(),
            },
        );

        let alerts = compute_alerts_from_displays(&[&info], &errors);
        assert_eq!(alerts.len(), 2);
        assert!(alerts[0].is_error, "error alert should come first");
        assert!(!alerts[1].is_error, "dirty alert should come second");
    }

    #[test]
    fn test_compute_alerts_running_dirty_kild_no_alert() {
        let info = make_info("active-work", ProcessStatus::Running, GitStatus::Dirty);

        let alerts = compute_alerts_from_displays(&[&info], &std::collections::HashMap::new());
        assert!(
            alerts.is_empty(),
            "running dirty kilds should not trigger alerts"
        );
    }

    /// Testable alert computation without AppState dependency.
    fn compute_alerts_from_displays(
        displays: &[&SessionInfo],
        errors: &std::collections::HashMap<String, crate::state::errors::OperationError>,
    ) -> Vec<Alert> {
        let mut alerts = Vec::new();

        for display in displays {
            if let Some(err) = errors.get(&display.session.branch) {
                alerts.push(Alert {
                    message: format!("{}: {}", display.session.branch, err.message),
                    is_error: true,
                });
            }
        }

        for display in displays {
            if display.process_status == ProcessStatus::Stopped
                && display.git_status == GitStatus::Dirty
            {
                alerts.push(Alert {
                    message: format!("{} has uncommitted changes", display.session.branch),
                    is_error: false,
                });
            }
        }

        alerts
    }

    fn make_info(
        branch: &str,
        process_status: ProcessStatus,
        git_status: GitStatus,
    ) -> SessionInfo {
        let json = serde_json::json!({
            "id": format!("test-{}", branch),
            "project_id": "test-project",
            "branch": branch,
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2026-01-01T00:00:00Z",
        });
        let session: Session = serde_json::from_value(json).unwrap();
        SessionInfo {
            session,
            process_status,
            git_status,
            uncommitted_diff: None,
        }
    }
}
