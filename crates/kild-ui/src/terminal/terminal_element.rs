use std::sync::Arc;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags as CellFlags;
use gpui::{
    App, Bounds, Element, ElementId, Font, FontWeight, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, LayoutId, Pixels, SharedString, Size, Style, TextRun, Window, fill, font, point,
    px, size,
};

use super::colors;
use super::state::KildListener;
use super::types::BatchedTextRun;
use crate::theme;

/// Prepared rendering data computed in prepaint, consumed in paint.
pub struct PrepaintState {
    text_runs: Vec<PreparedLine>,
    bg_regions: Vec<PreparedBgRegion>,
    cursor: Option<PreparedCursor>,
    cell_width: Pixels,
    cell_height: Pixels,
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

/// Custom GPUI Element that renders terminal cells as GPU draw calls.
pub struct TerminalElement {
    term: Arc<FairMutex<Term<KildListener>>>,
    has_focus: bool,
}

impl TerminalElement {
    pub fn new(term: Arc<FairMutex<Term<KildListener>>>, has_focus: bool) -> Self {
        Self { term, has_focus }
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

        if cell_width <= px(0.0) || cell_height <= px(0.0) {
            return PrepaintState {
                text_runs: vec![],
                bg_regions: vec![],
                cursor: None,
                cell_width,
                cell_height,
            };
        }

        let cols = (bounds.size.width / cell_width).floor() as usize;
        let rows = (bounds.size.height / cell_height).floor() as usize;
        if cols == 0 || rows == 0 {
            return PrepaintState {
                text_runs: vec![],
                bg_regions: vec![],
                cursor: None,
                cell_width,
                cell_height,
            };
        }

        // FairMutex (alacritty_terminal::sync) does not poison — it's not
        // std::sync::Mutex. lock() will always succeed (may block, never Err).
        let term = self.term.lock();
        let content = term.renderable_content();

        let terminal_bg = Hsla::from(theme::terminal_background());
        let terminal_fg = Hsla::from(theme::terminal_foreground());

        let mut text_lines: Vec<PreparedLine> = Vec::with_capacity(rows);
        let mut bg_regions: Vec<PreparedBgRegion> = Vec::with_capacity(rows * 2);
        let mut cursor: Option<PreparedCursor> = None;

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
            if ch != ' ' && ch != '\0' {
                run_text.push(ch);
            } else {
                run_text.push(' ');
            }
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

        // Cursor
        let cursor_point = content.cursor.point;
        let cursor_line = cursor_point.line.0;
        let cursor_col = cursor_point.column.0;
        if cursor_line >= 0 && (cursor_line as usize) < rows && cursor_col < cols {
            let cx_pos = bounds.origin.x + cursor_col as f32 * cell_width;
            let cy_pos = bounds.origin.y + cursor_line as f32 * cell_height;
            let cursor_color = Hsla::from(theme::terminal_cursor());

            cursor = Some(PreparedCursor {
                bounds: if self.has_focus {
                    Bounds::new(point(cx_pos, cy_pos), size(cell_width, cell_height))
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

        PrepaintState {
            text_runs: text_lines,
            bg_regions,
            cursor,
            cell_width,
            cell_height,
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
    }
}
