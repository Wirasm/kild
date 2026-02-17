use std::sync::Arc;

use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, SharedString, Size, Style, TextRun, Window, px,
};
use linkify::{LinkFinder, LinkKind};

use super::super::state::{KildListener, ResizeHandle};
use super::types::{FONT_NORMAL, LineText, MouseState, PrepaintState, PreparedUrlRegion};
use crate::theme;

/// Custom GPUI Element that renders terminal cells as GPU draw calls.
pub struct TerminalElement {
    pub(super) term: Arc<FairMutex<Term<KildListener>>>,
    pub(super) has_focus: bool,
    pub(super) resize_handle: ResizeHandle,
    pub(super) cursor_visible: bool,
    pub(super) mouse_state: MouseState,
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

    /// Convert pixel position to terminal grid Point + Side.
    /// Clamps negative coordinates to 0. Does not clamp to terminal dimensions
    /// (alacritty's Selection API handles out-of-bounds points).
    pub(super) fn pixel_to_grid(
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
    pub(crate) fn measure_cell(window: &mut Window, _cx: &mut App) -> (Pixels, Pixels) {
        let font_size = px(theme::TEXT_BASE);
        let run = TextRun {
            len: 1,
            font: FONT_NORMAL.clone(),
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
        self.do_prepaint(bounds, window, cx)
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
        self.do_paint(bounds, prepaint, window, cx);
    }
}

/// Convert a GPUI pixel scroll delta to an alacritty line count.
///
/// Both GPUI and alacritty use the same sign convention: positive = scroll up
/// (toward history). The result is rounded to the nearest integer and returned
/// as-is — no negation needed.
pub(crate) fn scroll_delta_lines(pixel_delta_y: Pixels, cell_height: Pixels) -> i32 {
    (pixel_delta_y / cell_height).round() as i32
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
                bounds: Bounds::new(gpui::point(x, y), gpui::size(w, cell_height)),
                url: link.as_str().to_string(),
            });
        }
    }
    url_regions
}
