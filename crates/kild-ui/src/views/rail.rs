//! Project icon rail component.
//!
//! 48px wide vertical strip on the far left with project icons.

// Allow dead_code — scaffolding for the project icon rail, wired up in later tasks.
#![allow(dead_code)]

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::path::PathBuf;

use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;

/// Width of the rail in pixels.
pub const RAIL_WIDTH: f32 = 48.0;

/// Icon circle size in pixels.
const ICON_SIZE: f32 = 32.0;

/// Data for rendering a project icon in the rail.
struct RailIconData {
    list_position: usize,
    path: PathBuf,
    first_char: String,
    is_active: bool,
    count: usize,
}

impl RailIconData {
    fn from_project(
        idx: usize,
        project: &kild_core::projects::Project,
        active_path: Option<&std::path::Path>,
        state: &AppState,
    ) -> Self {
        let path = project.path().to_path_buf();
        let is_active = active_path == Some(project.path());
        let count = state.kild_count_for_project(project.path());
        let first_char = project
            .name()
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());

        Self {
            list_position: idx,
            path,
            first_char,
            is_active,
            count,
        }
    }
}

/// Render the project icon rail.
pub fn render_rail(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let active_path = state.active_project_path();

    let icons: Vec<RailIconData> = state
        .projects_iter()
        .enumerate()
        .map(|(idx, project)| RailIconData::from_project(idx, project, active_path, state))
        .collect();

    div()
        .w(px(RAIL_WIDTH))
        .h_full()
        .flex_col()
        .items_center()
        .py(px(theme::SPACE_3))
        .gap(px(theme::SPACE_2))
        .bg(theme::void())
        .border_r_1()
        .border_color(theme::border_subtle())
        // Project icons
        .children(icons.into_iter().map(|data| {
            let RailIconData {
                list_position,
                path,
                first_char,
                is_active,
                count,
            } = data;

            div()
                .id(("rail-icon", list_position))
                .relative()
                .cursor_pointer()
                .on_mouse_up(gpui::MouseButton::Left, {
                    let path = path.clone();
                    cx.listener(move |view, _, _, cx| {
                        view.on_project_select(path.clone(), cx);
                    })
                })
                // Icon circle
                .child(
                    div()
                        .size(px(ICON_SIZE))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::TEXT_MD))
                        .when(is_active, |this| {
                            this.bg(theme::ice()).text_color(theme::void())
                        })
                        .when(!is_active, |this| {
                            this.bg(theme::border()).text_color(theme::text_muted())
                        })
                        .child(first_char),
                )
                // Badge (count > 0)
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(-2.0))
                            .right(px(-2.0))
                            .min_w(px(16.0))
                            .h(px(16.0))
                            .rounded_full()
                            .bg(theme::ice())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.0))
                            .text_color(theme::void())
                            .px(px(3.0))
                            .child(count.to_string()),
                    )
                })
        }))
        // Spacer to push add button to bottom
        .child(div().flex_1())
        // Add project button
        .child(
            div()
                .id("rail-add-project")
                .size(px(ICON_SIZE))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::TEXT_LG))
                .text_color(theme::text_muted())
                .bg(theme::border_subtle())
                .cursor_pointer()
                .hover(|style| style.bg(theme::border()))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, window, cx| {
                        view.on_add_project_click(window, cx);
                    }),
                )
                .child("+"),
        )
}
