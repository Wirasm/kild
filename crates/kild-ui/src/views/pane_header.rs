//! Thin header bar above each terminal pane in a split layout.
//!
//! Shows the pane name and truncated path, with a visual focus indicator
//! via border color.

// Allow dead_code — pane header is rendered when split-pane views are wired up.
#![allow(dead_code)]

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::theme;
use crate::views::main_view::MainView;

/// Truncate a filesystem path for compact display.
///
/// Shows the last two path components (e.g., "project/src" from "/Users/dev/project/src").
fn truncate_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    format!(".../{}", parts[parts.len() - 2..].join("/"))
}

/// Render a pane header bar.
///
/// Displays the pane name (left) and a truncated path (right).
/// The bottom border color indicates whether this pane is focused.
pub fn render_pane_header(
    name: &str,
    path: &str,
    is_focused: bool,
    _cx: &mut Context<MainView>,
) -> impl IntoElement {
    div()
        .w_full()
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_between()
        .px(px(theme::SPACE_2))
        .bg(theme::surface())
        .border_b_1()
        .border_color(if is_focused {
            theme::border()
        } else {
            theme::border_subtle()
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .child(
                    div()
                        .text_size(px(theme::TEXT_SM))
                        .text_color(theme::text_bright())
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .child(truncate_path(path)),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_path_short() {
        assert_eq!(truncate_path("/foo"), "/foo");
        assert_eq!(truncate_path("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn test_truncate_path_long() {
        assert_eq!(truncate_path("/Users/dev/project/src"), ".../project/src");
    }

    #[test]
    fn test_truncate_path_empty() {
        assert_eq!(truncate_path(""), "");
    }
}
