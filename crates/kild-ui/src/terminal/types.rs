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
