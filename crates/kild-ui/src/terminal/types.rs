use alacritty_terminal::grid::Indexed;
use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, TermMode};
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
    /// Empty snapshot used for initialization before the first sync().
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

    /// Build a snapshot from a locked Term. Lock must be held by the caller.
    /// Call this inside a short lock scope; the result is fully owned.
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
