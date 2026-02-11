use std::sync::Arc;

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags as CellFlags;
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, Font, FontWeight, GlobalElementId,
    Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, ScrollWheelEvent, SharedString, Size, Style, TextRun,
    Window, fill, font, point, px, size,
};
use linkify::{LinkFinder, LinkKind};

use super::colors;
use super::state::{KildListener, ResizeHandle};
use super::types::BatchedTextRun;
use crate::theme;

/// Prepared rendering data computed in prepaint, consumed in paint.
pub struct PrepaintState {
    text_runs: Vec<PreparedLine>,
    bg_regions: Vec<PreparedBgRegion>,
    selection_rects: Vec<PreparedBgRegion>,
    url_regions: Vec<PreparedUrlRegion>,
    cursor: Option<PreparedCursor>,
    cell_width: Pixels,
    cell_height: Pixels,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
    scrolled_up: bool,
}

struct PreparedLine {
    line_idx: usize,
    runs: Vec<BatchedTextRun>,
}

struct PreparedBgRegion {
    bounds: Bounds<Pixels>,
    color: Hsla,
}

struct PreparedCursor {
    bounds: Bounds<Pixels>,
    color: Hsla,
}

/// Per-line text data for URL scanning: (line_index, line_text, col_offsets).
/// Each col_offset entry maps a char index to (grid_column, cell_width_in_columns).
type LineText = (i32, String, Vec<(usize, usize)>);

/// A detected URL region in terminal output with precomputed pixel bounds.
pub(crate) struct PreparedUrlRegion {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) url: String,
}

/// Mouse interaction state for URL hover detection.
/// Both fields must be updated together to prevent stale highlights.
pub(crate) struct MouseState {
    pub(crate) position: Option<gpui::Point<Pixels>>,
    pub(crate) cmd_held: bool,
}

/// Custom GPUI Element that renders terminal cells as GPU draw calls.
pub struct TerminalElement {
    term: Arc<FairMutex<Term<KildListener>>>,
    has_focus: bool,
    resize_handle: ResizeHandle,
    cursor_visible: bool,
    mouse_state: MouseState,
}

impl TerminalElement {
    pub fn new(
        term: Arc<FairMutex<Term<KildListener>>>,
        has_focus: bool,
        resize_handle: ResizeHandle,
        cursor_visible: bool,
        mouse_state: MouseState,
    ) -> Self {
        Self {
            term,
            has_focus,
            resize_handle,
            cursor_visible,
            mouse_state,
        }
    }

    fn terminal_font() -> Font {
        font(theme::FONT_MONO)
    }

    fn bold_font() -> Font {
        Font {
            weight: FontWeight::BOLD,
            ..font(theme::FONT_MONO)
        }
    }

    fn italic_font() -> Font {
        Font {
            style: gpui::FontStyle::Italic,
            ..font(theme::FONT_MONO)
        }
    }

    fn bold_italic_font() -> Font {
        Font {
            weight: FontWeight::BOLD,
            style: gpui::FontStyle::Italic,
            ..font(theme::FONT_MONO)
        }
    }

    /// Convert pixel position to terminal grid Point + Side.
    /// Clamps negative coordinates to 0. Does not clamp to terminal dimensions
    /// (alacritty's Selection API handles out-of-bounds points).
    fn pixel_to_grid(
        position: gpui::Point<Pixels>,
        bounds: Bounds<Pixels>,
        cell_width: Pixels,
        cell_height: Pixels,
    ) -> (AlacPoint, Side) {
        let col = ((position.x - bounds.origin.x) / cell_width).max(0.0);
        let line = ((position.y - bounds.origin.y) / cell_height).max(0.0);
        let side = if col - col.floor() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        // Clamp before casting to prevent overflow on extreme values.
        let line_i32 = (line.floor() as f64).min(i32::MAX as f64) as i32;
        let col_usize = (col.floor() as f64).min(usize::MAX as f64) as usize;
        (AlacPoint::new(Line(line_i32), Column(col_usize)), side)
    }

    /// Measure cell dimensions using a reference character.
    fn measure_cell(window: &mut Window, _cx: &mut App) -> (Pixels, Pixels) {
        let font_size = px(theme::TEXT_BASE);
        let run = TextRun {
            len: 1,
            font: Self::terminal_font(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(SharedString::from("M"), font_size, &[run], None);
        let cell_width = line.width;
        let cell_height = window.line_height();
        (cell_width, cell_height)
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size {
                width: gpui::relative(1.).into(),
                height: gpui::relative(1.).into(),
            },
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let (cell_width, cell_height) = Self::measure_cell(window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::default());

        if cell_width <= px(0.0) || cell_height <= px(0.0) {
            return PrepaintState {
                text_runs: vec![],
                bg_regions: vec![],
                selection_rects: vec![],
                url_regions: vec![],
                cursor: None,
                cell_width,
                cell_height,
                bounds,
                hitbox,
                scrolled_up: false,
            };
        }

        let cols = (bounds.size.width / cell_width).floor() as usize;
        let rows = (bounds.size.height / cell_height).floor() as usize;
        if cols == 0 || rows == 0 {
            return PrepaintState {
                text_runs: vec![],
                bg_regions: vec![],
                selection_rects: vec![],
                url_regions: vec![],
                cursor: None,
                cell_width,
                cell_height,
                bounds,
                hitbox,
                scrolled_up: false,
            };
        }

        // Resize PTY and terminal grid if dimensions changed.
        // Must happen before term.lock() so the snapshot reflects the new size.
        if let Err(e) = self
            .resize_handle
            .resize_if_changed(rows as u16, cols as u16)
        {
            tracing::error!(
                event = "ui.terminal.resize_failed",
                rows = rows,
                cols = cols,
                error = %e,
            );
        }

        // FairMutex (alacritty_terminal::sync) does not poison — it's not
        // std::sync::Mutex. lock() will always succeed (may block, never Err).
        let term = self.term.lock();
        let scrolled_up = term.grid().display_offset() > 0;
        let content = term.renderable_content();

        let terminal_bg = Hsla::from(theme::terminal_background());
        let terminal_fg = Hsla::from(theme::terminal_foreground());

        let mut text_lines: Vec<PreparedLine> = Vec::with_capacity(rows);
        let mut bg_regions: Vec<PreparedBgRegion> = Vec::with_capacity(rows * 2);
        let mut cursor: Option<PreparedCursor> = None;
        let cursor_point = content.cursor.point;
        let mut cursor_is_wide = false;

        // Text run state
        let mut current_line: i32 = -1;
        let mut current_runs: Vec<BatchedTextRun> = Vec::new();
        let mut run_text = String::new();
        let mut run_fg = terminal_fg;
        let mut run_bold = false;
        let mut run_italic = false;
        let mut run_underline = false;
        let mut run_strikethrough = false;
        let mut run_start_col: usize = 0;

        // Background merging state — runs of identical bg color are merged into
        // single rectangles. Backgrounds matching terminal_bg are skipped entirely
        // since the default background is already painted as the first layer.
        let mut bg_start_col: usize = 0;
        let mut bg_color: Option<Hsla> = None;
        let mut bg_line: i32 = -1;

        // Per-line text accumulation for URL scanning.
        // Each entry: (line_index, line_text, col_offsets).
        // col_offsets maps char index to (grid_column, cell_width) to handle wide chars.
        let mut line_texts: Vec<LineText> = Vec::with_capacity(rows);
        let mut current_line_text = String::new();
        let mut current_col_offsets: Vec<(usize, usize)> = Vec::new();
        let mut url_text_line: i32 = -1;

        let flush_bg = |bg_color: Option<Hsla>,
                        bg_line: i32,
                        bg_start_col: usize,
                        end_col: usize,
                        regions: &mut Vec<PreparedBgRegion>,
                        terminal_bg: Hsla,
                        bounds: Bounds<Pixels>,
                        cw: Pixels,
                        ch: Pixels| {
            if let Some(color) = bg_color
                && color != terminal_bg
                && end_col > bg_start_col
            {
                let y = bounds.origin.y + bg_line as f32 * ch;
                let x = bounds.origin.x + bg_start_col as f32 * cw;
                let w = (end_col - bg_start_col) as f32 * cw;
                regions.push(PreparedBgRegion {
                    bounds: Bounds::new(point(x, y), size(w, ch)),
                    color,
                });
            }
        };

        for indexed in content.display_iter {
            let line_idx = indexed.point.line.0;
            let col = indexed.point.column.0;
            let cell = &indexed.cell;

            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                continue;
            }

            // Line changed — flush
            if line_idx != current_line {
                // Flush text run
                if !run_text.is_empty() {
                    current_runs.push(BatchedTextRun::new(
                        std::mem::take(&mut run_text),
                        run_fg,
                        run_start_col,
                        run_bold,
                        run_italic,
                        run_underline,
                        run_strikethrough,
                    ));
                }
                // Flush bg
                flush_bg(
                    bg_color.take(),
                    bg_line,
                    bg_start_col,
                    if bg_line >= 0 { cols } else { 0 },
                    &mut bg_regions,
                    terminal_bg,
                    bounds,
                    cell_width,
                    cell_height,
                );

                if !current_runs.is_empty() {
                    text_lines.push(PreparedLine {
                        line_idx: current_line as usize,
                        runs: std::mem::take(&mut current_runs),
                    });
                }
                // Flush line text for URL scanning
                if !current_line_text.is_empty() {
                    line_texts.push((
                        url_text_line,
                        std::mem::take(&mut current_line_text),
                        std::mem::take(&mut current_col_offsets),
                    ));
                }
                url_text_line = line_idx;
                current_line = line_idx;
                run_start_col = col;
                bg_line = line_idx;
                bg_start_col = col;
            }

            // Resolve colors
            let mut fg = if cell.flags.contains(CellFlags::INVERSE) {
                colors::resolve_color(&cell.bg)
            } else {
                colors::resolve_color(&cell.fg)
            };
            let bg = if cell.flags.contains(CellFlags::INVERSE) {
                colors::resolve_color(&cell.fg)
            } else {
                colors::resolve_color(&cell.bg)
            };

            if cell.flags.contains(CellFlags::DIM) {
                fg = Hsla {
                    l: fg.l * 0.67,
                    ..fg
                };
            }
            if cell.flags.contains(CellFlags::HIDDEN) {
                fg = bg;
            }

            let bold = cell.flags.contains(CellFlags::BOLD);
            let italic = cell.flags.contains(CellFlags::ITALIC);
            let underline = cell.flags.intersects(
                CellFlags::UNDERLINE
                    | CellFlags::DOUBLE_UNDERLINE
                    | CellFlags::UNDERCURL
                    | CellFlags::DOTTED_UNDERLINE
                    | CellFlags::DASHED_UNDERLINE,
            );
            let strikethrough = cell.flags.contains(CellFlags::STRIKEOUT);

            // Background merging
            if bg_color != Some(bg) {
                flush_bg(
                    bg_color.take(),
                    bg_line,
                    bg_start_col,
                    col,
                    &mut bg_regions,
                    terminal_bg,
                    bounds,
                    cell_width,
                    cell_height,
                );
                bg_start_col = col;
                bg_color = Some(bg);
            }

            // Wide characters get their own text run so each is positioned at
            // its exact grid column. Batching them with normal chars causes the
            // text shaper to misplace subsequent glyphs because it doesn't know
            // about the 2-cell grid width.
            let is_wide = cell.flags.contains(CellFlags::WIDE_CHAR);

            if is_wide && indexed.point == cursor_point {
                cursor_is_wide = true;
            }

            if is_wide {
                // Flush pending normal-width run
                if !run_text.is_empty() {
                    current_runs.push(BatchedTextRun::new(
                        std::mem::take(&mut run_text),
                        run_fg,
                        run_start_col,
                        run_bold,
                        run_italic,
                        run_underline,
                        run_strikethrough,
                    ));
                }
                // Push wide char as its own run
                let ch = cell.c;
                let wch = if ch != ' ' && ch != '\0' { ch } else { ' ' };
                current_runs.push(BatchedTextRun::new(
                    String::from(wch),
                    fg,
                    col,
                    bold,
                    italic,
                    underline,
                    strikethrough,
                ));
                // Track for URL scanning (wide char = 1 char, 2 grid columns)
                current_col_offsets.push((col, 2));
                current_line_text.push(wch);
                continue;
            }

            // Text run batching
            let same_style = fg == run_fg
                && bold == run_bold
                && italic == run_italic
                && underline == run_underline
                && strikethrough == run_strikethrough;

            if !same_style && !run_text.is_empty() {
                current_runs.push(BatchedTextRun::new(
                    std::mem::take(&mut run_text),
                    run_fg,
                    run_start_col,
                    run_bold,
                    run_italic,
                    run_underline,
                    run_strikethrough,
                ));
                run_start_col = col;
            }

            if run_text.is_empty() {
                run_fg = fg;
                run_bold = bold;
                run_italic = italic;
                run_underline = underline;
                run_strikethrough = strikethrough;
                run_start_col = col;
            }

            let ch = cell.c;
            let display_ch = if ch != ' ' && ch != '\0' { ch } else { ' ' };
            run_text.push(display_ch);
            // Track for URL scanning (normal char = 1 grid column)
            current_col_offsets.push((col, 1));
            current_line_text.push(display_ch);
        }

        // Flush final run/line/bg
        if !run_text.is_empty() {
            current_runs.push(BatchedTextRun::new(
                std::mem::take(&mut run_text),
                run_fg,
                run_start_col,
                run_bold,
                run_italic,
                run_underline,
                run_strikethrough,
            ));
        }
        if !current_runs.is_empty() {
            text_lines.push(PreparedLine {
                line_idx: current_line as usize,
                runs: std::mem::take(&mut current_runs),
            });
        }
        flush_bg(
            bg_color.take(),
            bg_line,
            bg_start_col,
            cols,
            &mut bg_regions,
            terminal_bg,
            bounds,
            cell_width,
            cell_height,
        );
        // Flush final line text
        if !current_line_text.is_empty() {
            line_texts.push((url_text_line, current_line_text, current_col_offsets));
        }

        // URL detection — scan accumulated line texts with linkify
        let url_regions = detect_urls(&line_texts, bounds, cell_width, cell_height);

        // Selection highlight rectangles
        let selection_color = Hsla::from(theme::terminal_selection());
        let mut selection_rects: Vec<PreparedBgRegion> = Vec::new();
        if let Some(sel) = &content.selection {
            if sel.start.line.0 < 0 || sel.end.line.0 < 0 || (sel.end.line.0 as usize) >= rows {
                tracing::debug!(
                    event = "ui.terminal.selection_clamped",
                    start_line = sel.start.line.0,
                    end_line = sel.end.line.0,
                    rows = rows,
                );
            }
            let start_line = sel.start.line.0.max(0) as usize;
            let end_line = sel.end.line.0.max(0) as usize;
            for line_idx in start_line..=end_line.min(rows.saturating_sub(1)) {
                let start_col = if line_idx == start_line {
                    sel.start.column.0
                } else {
                    0
                };
                let end_col = if line_idx == end_line {
                    sel.end.column.0 + 1
                } else {
                    cols
                };
                if end_col > start_col {
                    let x = bounds.origin.x + start_col as f32 * cell_width;
                    let y = bounds.origin.y + line_idx as f32 * cell_height;
                    let w = (end_col - start_col) as f32 * cell_width;
                    selection_rects.push(PreparedBgRegion {
                        bounds: Bounds::new(point(x, y), size(w, cell_height)),
                        color: selection_color,
                    });
                }
            }
        }

        // Cursor (only when visible — blink state from TerminalView)
        if self.cursor_visible {
            let cursor_point = content.cursor.point;
            let cursor_line = cursor_point.line.0;
            let cursor_col = cursor_point.column.0;
            if cursor_line >= 0 && (cursor_line as usize) < rows && cursor_col < cols {
                let cx_pos = bounds.origin.x + cursor_col as f32 * cell_width;
                let cy_pos = bounds.origin.y + cursor_line as f32 * cell_height;
                let cursor_color = Hsla::from(theme::terminal_cursor());

                cursor = Some(PreparedCursor {
                    bounds: if self.has_focus {
                        let cursor_w = if cursor_is_wide {
                            cell_width + cell_width
                        } else {
                            cell_width
                        };
                        Bounds::new(point(cx_pos, cy_pos), size(cursor_w, cell_height))
                    } else {
                        Bounds::new(point(cx_pos, cy_pos), size(px(2.0), cell_height))
                    },
                    color: if self.has_focus {
                        cursor_color
                    } else {
                        Hsla {
                            a: 0.5,
                            ..cursor_color
                        }
                    },
                });
            }
        }

        PrepaintState {
            text_runs: text_lines,
            bg_regions,
            selection_rects,
            url_regions,
            cursor,
            cell_width,
            cell_height,
            bounds,
            hitbox,
            scrolled_up,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
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
                    (true, true) => Self::bold_italic_font(),
                    (true, false) => Self::bold_font(),
                    (false, true) => Self::italic_font(),
                    (false, false) => Self::terminal_font(),
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
                font: Self::terminal_font(),
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

        // Scroll wheel handler — translates GPUI scroll events to alacritty display offset.
        let hitbox = prepaint.hitbox.clone();
        let term = self.term.clone();
        let cell_height = prepaint.cell_height;
        window.on_mouse_event::<ScrollWheelEvent>(move |event, phase, window, _cx| {
            if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                let pixel_delta = event.delta.pixel_delta(cell_height);
                // GPUI: negative y = scroll up. Alacritty: positive Delta = scroll up.
                let lines = -(pixel_delta.y / cell_height).round() as i32;
                if lines != 0 {
                    term.lock().scroll_display(Scroll::Delta(lines));
                }
            }
        });

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

/// Extract URLs from terminal line texts and compute their pixel bounds.
///
/// Each entry in `line_texts` is `(line_index, line_text, col_offsets)` where
/// `col_offsets` maps each char index to `(grid_column, cell_width_in_columns)`.
/// Wide characters have `cell_width_in_columns = 2`, normal characters `1`.
pub(crate) fn detect_urls(
    line_texts: &[LineText],
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
) -> Vec<PreparedUrlRegion> {
    let mut url_regions: Vec<PreparedUrlRegion> = Vec::new();
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    for (line_idx, text, col_offsets) in line_texts {
        // Build byte-offset → char-index lookup for multi-byte char support.
        // linkify returns byte offsets, but col_offsets is indexed by char position.
        let byte_to_char: Vec<usize> = {
            let mut map = vec![0usize; text.len() + 1];
            for (char_idx, (byte_idx, _)) in text.char_indices().enumerate() {
                map[byte_idx] = char_idx;
            }
            // Sentinel for exclusive end offset
            map[text.len()] = text.chars().count();
            map
        };

        for link in finder.links(text) {
            let start_byte = link.start();
            let end_byte = link.end();
            let start_char = byte_to_char[start_byte];
            let end_char = byte_to_char[end_byte]; // exclusive char index
            if start_char >= col_offsets.len() || end_char == 0 {
                continue;
            }
            let (start_col, _) = col_offsets[start_char];
            // end_char is exclusive, so subtract 1 to get the last inclusive
            // char index, then clamp to valid range. The end grid column is
            // last_col + last_width to account for wide chars occupying 2
            // grid columns.
            let unclamped = end_char - 1;
            let last_idx = unclamped.min(col_offsets.len() - 1);
            if unclamped != last_idx {
                tracing::debug!(
                    event = "ui.terminal.url_bounds_clamped",
                    url = link.as_str(),
                    end_char = end_char,
                    col_offsets_len = col_offsets.len(),
                );
            }
            let (last_col, last_width) = col_offsets[last_idx];
            let end_col = last_col + last_width;
            debug_assert!(
                end_col > start_col,
                "URL region width must be positive: start_col={start_col}, end_col={end_col}, url={}",
                link.as_str()
            );
            let x = bounds.origin.x + start_col as f32 * cell_width;
            let y = bounds.origin.y + *line_idx as f32 * cell_height;
            let w = (end_col - start_col) as f32 * cell_width;
            url_regions.push(PreparedUrlRegion {
                bounds: Bounds::new(point(x, y), size(w, cell_height)),
                url: link.as_str().to_string(),
            });
        }
    }
    url_regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point, px, size};

    /// Build col_offsets for a line where each char is either wide (2 cols) or normal (1 col).
    /// `wide_indices` lists the char indices that are wide (2 grid columns).
    fn build_col_offsets(text: &str, wide_indices: &[usize]) -> Vec<(usize, usize)> {
        let mut offsets = Vec::new();
        let mut grid_col = 0usize;
        for (i, _ch) in text.chars().enumerate() {
            let width = if wide_indices.contains(&i) { 2 } else { 1 };
            offsets.push((grid_col, width));
            grid_col += width;
        }
        offsets
    }

    fn test_bounds() -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)))
    }

    fn cell_w() -> Pixels {
        px(10.0)
    }

    fn cell_h() -> Pixels {
        px(20.0)
    }

    #[test]
    fn single_url_basic() {
        let text = "Check https://example.com here".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, "https://example.com");
        // "Check " = 6 chars, so URL starts at col 6
        assert_eq!(regions[0].bounds.origin.x, px(60.0));
        assert_eq!(regions[0].bounds.origin.y, px(0.0));
        // "https://example.com" = 19 chars = 19 cols wide
        assert_eq!(regions[0].bounds.size.width, px(190.0));
        assert_eq!(regions[0].bounds.size.height, cell_h());
    }

    #[test]
    fn url_at_column_zero() {
        let text = "https://example.com rest".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, "https://example.com");
        assert_eq!(regions[0].bounds.origin.x, px(0.0));
    }

    #[test]
    fn url_at_end_of_line() {
        let text = "prefix https://example.com".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, "https://example.com");
        // "prefix " = 7 chars
        assert_eq!(regions[0].bounds.origin.x, px(70.0));
        // 19 cols wide
        assert_eq!(regions[0].bounds.size.width, px(190.0));
    }

    #[test]
    fn empty_line_no_panic() {
        let text = "".to_string();
        let col_offsets: Vec<(usize, usize)> = vec![];
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert!(regions.is_empty());
    }

    #[test]
    fn no_urls_in_plain_text() {
        let text = "just plain text with no links".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert!(regions.is_empty());
    }

    #[test]
    fn empty_input_vec() {
        let line_texts: Vec<(i32, String, Vec<(usize, usize)>)> = vec![];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert!(regions.is_empty());
    }

    #[test]
    fn url_with_query_string() {
        let text = "https://example.com/path?q=1&b=2".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text.clone(), col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, text);
        assert_eq!(regions[0].bounds.origin.x, px(0.0));
        // 32 chars = 32 cols
        assert_eq!(regions[0].bounds.size.width, px(320.0));
    }

    #[test]
    fn multiple_urls_non_overlapping() {
        let text = "Visit https://a.com or https://b.com end".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].url, "https://a.com");
        assert_eq!(regions[1].url, "https://b.com");

        // Verify non-overlapping: first region ends before second starts
        let first_end_x = regions[0].bounds.origin.x + regions[0].bounds.size.width;
        let second_start_x = regions[1].bounds.origin.x;
        assert!(
            first_end_x <= second_start_x,
            "URL regions overlap: first ends at {first_end_x:?}, second starts at {second_start_x:?}"
        );

        // Same line, same y coordinate
        assert_eq!(regions[0].bounds.origin.y, regions[1].bounds.origin.y);
    }

    #[test]
    fn emoji_before_url_wide_char_bounds() {
        // Rocket emoji is a wide char (2 grid cols), then space (1 col), then URL
        // Grid layout: cols 0-1 = rocket, col 2 = space, cols 3+ = URL
        let text = "\u{1F680} https://example.com".to_string();
        let col_offsets = build_col_offsets(&text, &[0]); // index 0 is the emoji (wide)
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, "https://example.com");
        // Emoji=2 cols + space=1 col = URL starts at grid col 3
        assert_eq!(regions[0].bounds.origin.x, px(30.0));
        // 19 chars, all normal width = 19 cols
        assert_eq!(regions[0].bounds.size.width, px(190.0));
    }

    #[test]
    fn cjk_before_url_wide_char_bounds() {
        // Two CJK chars (each 2 grid cols), then space, then URL
        // Grid layout: cols 0-1 = first CJK, cols 2-3 = second CJK, col 4 = space, cols 5+ = URL
        let text = "\u{4F60}\u{597D} https://example.com".to_string();
        let col_offsets = build_col_offsets(&text, &[0, 1]); // indices 0,1 are CJK (wide)
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].url, "https://example.com");
        // 2 wide chars = 4 cols + space = 1 col → URL starts at grid col 5
        assert_eq!(regions[0].bounds.origin.x, px(50.0));
        assert_eq!(regions[0].bounds.size.width, px(190.0));
    }

    #[test]
    fn url_on_different_line_index() {
        let text = "https://example.com".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        // Line index 3 means y offset = 3 * cell_height
        let line_texts = vec![(3i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].bounds.origin.y, px(60.0)); // 3 * 20.0
    }

    #[test]
    fn bounds_origin_offset() {
        let text = "https://example.com".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let offset_bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(800.0), px(600.0)));
        let regions = detect_urls(&line_texts, offset_bounds, cell_w(), cell_h());

        assert_eq!(regions.len(), 1);
        // x should be offset by bounds origin
        assert_eq!(regions[0].bounds.origin.x, px(100.0));
        assert_eq!(regions[0].bounds.origin.y, px(50.0));
    }

    #[test]
    fn bare_domain_without_scheme_not_matched() {
        // linkify with LinkKind::Url should not match bare domains without a scheme
        let text = "not a url example.com something".to_string();
        let col_offsets = build_col_offsets(&text, &[]);
        let line_texts = vec![(0i32, text, col_offsets)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert!(regions.is_empty());
    }

    #[test]
    fn multiple_lines_with_urls() {
        let text1 = "line1 https://a.com".to_string();
        let offsets1 = build_col_offsets(&text1, &[]);
        let text2 = "line2 https://b.com".to_string();
        let offsets2 = build_col_offsets(&text2, &[]);
        let line_texts = vec![(0i32, text1, offsets1), (2i32, text2, offsets2)];
        let regions = detect_urls(&line_texts, test_bounds(), cell_w(), cell_h());

        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].url, "https://a.com");
        assert_eq!(regions[0].bounds.origin.y, px(0.0)); // line 0
        assert_eq!(regions[1].url, "https://b.com");
        assert_eq!(regions[1].bounds.origin.y, px(40.0)); // line 2 * 20.0
    }
}
