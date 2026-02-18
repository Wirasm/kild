use alacritty_terminal::grid::Indexed;
#[cfg(test)]
use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, TermMode};
#[cfg(test)]
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::Hsla;

/// A run of adjacent same-style cells, batched for efficient text rendering.
pub struct BatchedTextRun {
    text: String,
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    start_col: usize,
}

impl BatchedTextRun {
    pub fn new(
        text: String,
        fg: Hsla,
        start_col: usize,
        bold: bool,
        italic: bool,
        underline: bool,
        strikethrough: bool,
    ) -> Self {
        Self {
            text,
            fg,
            bold,
            italic,
            underline,
            strikethrough,
            start_col,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn fg(&self) -> Hsla {
        self.fg
    }

    pub fn start_col(&self) -> usize {
        self.start_col
    }

    pub fn bold(&self) -> bool {
        self.bold
    }

    pub fn italic(&self) -> bool {
        self.italic
    }

    pub fn underline(&self) -> bool {
        self.underline
    }

    pub fn strikethrough(&self) -> bool {
        self.strikethrough
    }
}

/// Convenience type for an owned terminal cell with its grid position.
/// `GridIterator` yields `Indexed<&Cell>` (borrowed); this stores `Indexed<Cell>` (owned).
pub type IndexedCell = Indexed<Cell>;

/// Owned snapshot of terminal display state, built under FairMutex then used
/// for rendering without holding the lock.
pub struct TerminalContent {
    /// Visible cells, cloned from the grid iterator while FairMutex is held.
    /// Cell::clone() is cheap: most cells have extra = None; Arc::clone() when Some.
    pub cells: Vec<IndexedCell>,
    /// Cursor position and shape. Copy type.
    pub cursor: RenderableCursor,
    /// Current text selection range, if any. Copy type.
    pub selection: Option<SelectionRange>,
    /// Terminal mode flags (SHOW_CURSOR, ALT_SCREEN, APP_CURSOR, etc.). Copy type.
    pub mode: TermMode,
    /// Grid scroll offset in lines. > 0 means scrolled into scrollback history.
    pub display_offset: usize,
}

impl TerminalContent {
    /// Empty snapshot for use in tests only. Not used in production code.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            cells: Vec::new(),
            cursor: RenderableCursor {
                shape: CursorShape::Block,
                point: Point::default(),
            },
            selection: None,
            mode: TermMode::empty(),
            display_offset: 0,
        }
    }

    /// Build a snapshot from a Term reference.
    ///
    /// The caller obtains the FairMutex guard before calling (e.g. `&*term.lock()`);
    /// this function takes a plain reference so the caller controls the lock scope.
    /// The cell clone loop is O(visible cells) — keep the guard scope tight.
    /// The result is fully owned and can outlive the lock.
    pub fn from_term<T>(term: &alacritty_terminal::term::Term<T>) -> Self
    where
        T: alacritty_terminal::event::EventListener,
    {
        let content = term.renderable_content();
        let cells = content
            .display_iter
            .map(|ic| Indexed {
                point: ic.point,
                cell: ic.cell.clone(),
            })
            .collect();
        Self {
            cells,
            cursor: content.cursor,
            selection: content.selection,
            mode: content.mode,
            display_offset: content.display_offset,
        }
    }
}

// Manual Clone impl: Indexed<Cell> doesn't derive Clone (only Debug/PartialEq/Eq),
// so we clone each cell individually while copying the Copy fields directly.
impl Clone for TerminalContent {
    fn clone(&self) -> Self {
        Self {
            cells: self
                .cells
                .iter()
                .map(|ic| Indexed {
                    point: ic.point,
                    cell: ic.cell.clone(),
                })
                .collect(),
            cursor: self.cursor,
            selection: self.selection,
            mode: self.mode,
            display_offset: self.display_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::grid::Indexed;
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::term::cell::Cell;
    use alacritty_terminal::term::{RenderableCursor, TermMode};
    use alacritty_terminal::vte::ansi::CursorShape;

    use super::TerminalContent;

    fn make_cell(ch: char, line: i32, col: usize) -> super::IndexedCell {
        let mut cell = Cell::default();
        cell.c = ch;
        Indexed {
            point: Point::new(Line(line), Column(col)),
            cell,
        }
    }

    // --- empty() contract ---

    #[test]
    fn empty_has_no_app_cursor() {
        let content = TerminalContent::empty();
        assert!(!content.mode.contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn empty_has_no_show_cursor() {
        let content = TerminalContent::empty();
        assert!(!content.mode.contains(TermMode::SHOW_CURSOR));
    }

    #[test]
    fn empty_has_empty_cells_and_zero_offset() {
        let content = TerminalContent::empty();
        assert!(content.cells.is_empty());
        assert_eq!(content.display_offset, 0);
    }

    // --- Clone roundtrip ---

    #[test]
    fn clone_roundtrip_preserves_all_fields() {
        let original = TerminalContent {
            cells: vec![make_cell('A', 0, 0), make_cell('B', 0, 1)],
            cursor: RenderableCursor {
                shape: CursorShape::Beam,
                point: Point::new(Line(1), Column(3)),
            },
            selection: None,
            mode: TermMode::APP_CURSOR | TermMode::SHOW_CURSOR,
            display_offset: 5,
        };

        let cloned = original.clone();

        assert_eq!(cloned.cells.len(), 2);
        assert_eq!(cloned.cells[0].cell.c, 'A');
        assert_eq!(cloned.cells[0].point, original.cells[0].point);
        assert_eq!(cloned.cells[1].cell.c, 'B');
        assert_eq!(cloned.cells[1].point, original.cells[1].point);
        assert_eq!(cloned.cursor.shape, original.cursor.shape);
        assert_eq!(cloned.cursor.point, original.cursor.point);
        assert_eq!(cloned.selection, original.selection);
        assert_eq!(cloned.mode, original.mode);
        assert_eq!(cloned.display_offset, original.display_offset);
    }

    #[test]
    fn clone_of_empty_content_is_empty() {
        let empty = TerminalContent::empty();
        let cloned = empty.clone();
        assert!(cloned.cells.is_empty());
        assert_eq!(cloned.display_offset, 0);
    }
}
