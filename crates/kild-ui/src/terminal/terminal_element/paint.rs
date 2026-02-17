use alacritty_terminal::selection::{Selection, SelectionType};
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, SharedString, TextRun, Window, fill, point, px, size,
};

use super::element::TerminalElement;
use super::types::{FONT_BOLD, FONT_BOLD_ITALIC, FONT_ITALIC, FONT_NORMAL, PrepaintState};
use crate::theme;

impl TerminalElement {
    pub(super) fn do_paint(
        &mut self,
        bounds: Bounds<Pixels>,
        prepaint: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let terminal_bg = Hsla::from(theme::terminal_background());
        let font_size = px(theme::TEXT_BASE);

        // Painter's algorithm — layers are painted back-to-front so later
        // draws occlude earlier ones without needing a depth buffer.

        // Layer 1: Terminal background (base layer, fills entire bounds)
        window.paint_quad(fill(bounds, terminal_bg));

        // Layer 2: Cell background regions (colored backgrounds on top of base)
        for region in &prepaint.bg_regions {
            window.paint_quad(fill(region.bounds, region.color));
        }

        // Layer 2.5: Selection highlight (between cell bg and text)
        for rect in &prepaint.selection_rects {
            window.paint_quad(fill(rect.bounds, rect.color));
        }

        // Layer 2.7: URL underline highlights (Cmd+hover)
        if self.mouse_state.cmd_held
            && let Some(mouse_pos) = self.mouse_state.position
        {
            let url_color = Hsla {
                a: 0.5,
                ..Hsla::from(theme::ice())
            };
            let mut hovering_url = false;
            for region in &prepaint.url_regions {
                if region.bounds.contains(&mouse_pos) {
                    hovering_url = true;
                    let underline_y = region.bounds.origin.y + prepaint.cell_height - px(1.5);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(region.bounds.origin.x, underline_y),
                            size(region.bounds.size.width, px(1.5)),
                        ),
                        url_color,
                    ));
                }
            }
            if hovering_url {
                window.set_cursor_style(CursorStyle::PointingHand, &prepaint.hitbox);
            }
        }

        // Layer 3: Text runs (glyphs on top of backgrounds)
        for line in &prepaint.text_runs {
            let y = bounds.origin.y + line.line_idx as f32 * prepaint.cell_height;

            for run in &line.runs {
                let x = bounds.origin.x + run.start_col() as f32 * prepaint.cell_width;

                let f = match (run.bold(), run.italic()) {
                    (true, true) => FONT_BOLD_ITALIC.clone(),
                    (true, false) => FONT_BOLD.clone(),
                    (false, true) => FONT_ITALIC.clone(),
                    (false, false) => FONT_NORMAL.clone(),
                };

                let underline = if run.underline() {
                    Some(gpui::UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(run.fg()),
                        wavy: false,
                    })
                } else {
                    None
                };

                let strikethrough = if run.strikethrough() {
                    Some(gpui::StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(run.fg()),
                    })
                } else {
                    None
                };

                let text_run = TextRun {
                    len: run.text().len(),
                    font: f,
                    color: run.fg(),
                    background_color: None,
                    underline,
                    strikethrough,
                };

                let shaped = window.text_system().shape_line(
                    SharedString::from(run.text().to_owned()),
                    font_size,
                    &[text_run],
                    None,
                );

                if let Err(e) = shaped.paint(point(x, y), prepaint.cell_height, window, cx) {
                    tracing::error!(
                        event = "ui.terminal.paint_failed",
                        error = %e,
                        "Text rendering failed — terminal output may be incomplete"
                    );
                }
            }
        }

        // Layer 4: Cursor (topmost, always visible over text)
        if let Some(cursor) = &prepaint.cursor {
            window.paint_quad(fill(cursor.bounds, cursor.color));
        }

        // Layer 5: Scrollback badge (when scrolled up from bottom)
        if prepaint.scrolled_up {
            let badge_text = "Scrollback";
            let badge_font_size = px(theme::TEXT_XS);
            let padding_h = px(theme::SPACE_1);
            let padding_v = px(2.0);
            let badge_bg = Hsla::from(theme::elevated());
            let badge_fg = Hsla::from(theme::text_muted());

            let badge_run = TextRun {
                len: badge_text.len(),
                font: FONT_NORMAL.clone(),
                color: badge_fg,
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            let shaped = window.text_system().shape_line(
                SharedString::from(badge_text),
                badge_font_size,
                &[badge_run],
                None,
            );

            let text_width = shaped.width;
            let badge_width = text_width + padding_h + padding_h;
            let badge_height = badge_font_size + padding_v + padding_v;
            let margin = px(theme::SPACE_2);

            let badge_x = bounds.origin.x + bounds.size.width - badge_width - margin;
            let badge_y = bounds.origin.y + margin;

            window.paint_quad(fill(
                Bounds::new(point(badge_x, badge_y), size(badge_width, badge_height)),
                badge_bg,
            ));

            if let Err(e) = shaped.paint(
                point(badge_x + padding_h, badge_y + padding_v),
                badge_font_size,
                window,
                cx,
            ) {
                tracing::error!(event = "ui.terminal.badge_paint_failed", error = %e);
                // Fallback: thin accent bar so the user still sees "scrolled up".
                window.paint_quad(fill(
                    Bounds::new(point(badge_x, badge_y), size(badge_width, px(2.0))),
                    badge_fg,
                ));
            }
        }

        // Mouse down handler — Cmd+click opens URL, otherwise starts selection.
        let hitbox = prepaint.hitbox.clone();
        let term = self.term.clone();
        let cell_width = prepaint.cell_width;
        let cell_height = prepaint.cell_height;
        let sel_bounds = prepaint.bounds;
        let click_url_regions: Vec<(Bounds<Pixels>, String)> = prepaint
            .url_regions
            .iter()
            .map(|r| (r.bounds, r.url.clone()))
            .collect();
        window.on_mouse_event::<MouseDownEvent>(move |event, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
            {
                // Cmd+click on URL → open in browser
                if event.modifiers.platform {
                    for (url_bounds, url) in &click_url_regions {
                        if url_bounds.contains(&event.position) {
                            // Only allow http/https schemes to prevent opening
                            // file://, javascript:, or other potentially dangerous URLs.
                            if !url.starts_with("http://") && !url.starts_with("https://") {
                                tracing::warn!(
                                    event = "ui.terminal.url_open_blocked",
                                    url = url,
                                    reason =
                                        "unsupported scheme, only http:// and https:// allowed",
                                );
                                return;
                            }
                            tracing::info!(event = "ui.terminal.url_open_started", url = url);
                            match open::that(url) {
                                Ok(()) => {
                                    tracing::info!(
                                        event = "ui.terminal.url_open_completed",
                                        url = url
                                    );
                                }
                                Err(e) => {
                                    // TODO: Surface to user via UI error state. Currently the
                                    // paint closure doesn't have access to TerminalView's
                                    // set_error(), so we can only log. The user sees nothing
                                    // on failure — this violates "No Silent Failures" but
                                    // requires an architecture change to fix (e.g., shared
                                    // error channel between Element and View).
                                    tracing::error!(
                                        event = "ui.terminal.url_open_failed",
                                        url = url,
                                        error = %e,
                                    );
                                }
                            }
                            return;
                        }
                    }
                }

                let (grid_point, side) =
                    Self::pixel_to_grid(event.position, sel_bounds, cell_width, cell_height);
                let ty = match event.click_count {
                    2 => SelectionType::Semantic,
                    3 => SelectionType::Lines,
                    _ => SelectionType::Simple,
                };
                term.lock().selection = Some(Selection::new(ty, grid_point, side));
            }
        });

        // Mouse move handler — extends selection during drag.
        let hitbox = prepaint.hitbox.clone();
        let term = self.term.clone();
        let cell_width = prepaint.cell_width;
        let cell_height = prepaint.cell_height;
        let sel_bounds = prepaint.bounds;
        window.on_mouse_event::<MouseMoveEvent>(move |event, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.pressed_button == Some(MouseButton::Left)
                && hitbox.is_hovered(window)
            {
                let (grid_point, side) =
                    Self::pixel_to_grid(event.position, sel_bounds, cell_width, cell_height);
                if let Some(sel) = &mut term.lock().selection {
                    sel.update(grid_point, side);
                }
            }
        });
    }
}
