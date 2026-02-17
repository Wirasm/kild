use alacritty_terminal::term::cell::Flags as CellFlags;
use gpui::{App, Bounds, HitboxBehavior, Hsla, Pixels, Window, point, px, size};

use super::super::colors;
use super::super::types::BatchedTextRun;
use super::element::{TerminalElement, detect_urls};
use super::types::{LineText, PrepaintState, PreparedBgRegion, PreparedCursor, PreparedLine};
use crate::theme;

impl TerminalElement {
    pub(super) fn do_prepaint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
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
            tracing::warn!(
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
        // Only populated when Cmd is held (URLs are only displayed on Cmd+hover).
        let cmd_held = self.mouse_state.cmd_held;
        let mut line_texts: Vec<LineText> = Vec::new();
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
                if cmd_held {
                    current_col_offsets.push((col, 2));
                    current_line_text.push(wch);
                }
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
            if cmd_held {
                current_col_offsets.push((col, 1));
                current_line_text.push(display_ch);
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
        // Flush final line text
        if !current_line_text.is_empty() {
            line_texts.push((url_text_line, current_line_text, current_col_offsets));
        }

        // URL detection — only scan when Cmd is held (URLs are only displayed on Cmd+hover)
        let url_regions = if cmd_held {
            detect_urls(&line_texts, bounds, cell_width, cell_height)
        } else {
            Vec::new()
        };

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

        // Cursor (only when visible and terminal has cursor enabled via DECTCEM)
        if self.cursor_visible
            && content
                .mode
                .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
        {
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
}
