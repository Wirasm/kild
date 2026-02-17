use std::sync::LazyLock;

use gpui::{Bounds, Font, FontWeight, Hitbox, Hsla, Pixels, font};

use crate::theme;

/// Prepared rendering data computed in prepaint, consumed in paint.
pub struct PrepaintState {
    pub(super) text_runs: Vec<PreparedLine>,
    pub(super) bg_regions: Vec<PreparedBgRegion>,
    pub(super) selection_rects: Vec<PreparedBgRegion>,
    pub(super) url_regions: Vec<PreparedUrlRegion>,
    pub(super) cursor: Option<PreparedCursor>,
    pub(super) cell_width: Pixels,
    pub(super) cell_height: Pixels,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) hitbox: Hitbox,
    pub(super) scrolled_up: bool,
}

pub(super) struct PreparedLine {
    pub(super) line_idx: usize,
    pub(super) runs: Vec<super::super::types::BatchedTextRun>,
}

pub(super) struct PreparedBgRegion {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) color: Hsla,
}

pub(super) struct PreparedCursor {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) color: Hsla,
}

/// Per-line text data for URL scanning: (line_index, line_text, col_offsets).
/// Each col_offset entry maps a char index to (grid_column, cell_width_in_columns).
pub(super) type LineText = (i32, String, Vec<(usize, usize)>);

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

/// Cached font objects — terminal uses a single monospace font family.
/// `LazyLock` avoids reconstructing these on every text run per frame.
pub(super) static FONT_NORMAL: LazyLock<Font> = LazyLock::new(|| font(theme::FONT_MONO));
pub(super) static FONT_BOLD: LazyLock<Font> = LazyLock::new(|| Font {
    weight: FontWeight::BOLD,
    ..font(theme::FONT_MONO)
});
pub(super) static FONT_ITALIC: LazyLock<Font> = LazyLock::new(|| Font {
    style: gpui::FontStyle::Italic,
    ..font(theme::FONT_MONO)
});
pub(super) static FONT_BOLD_ITALIC: LazyLock<Font> = LazyLock::new(|| Font {
    weight: FontWeight::BOLD,
    style: gpui::FontStyle::Italic,
    ..font(theme::FONT_MONO)
});
