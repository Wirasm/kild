use gpui::Hsla;

/// A run of adjacent same-style cells, batched for efficient text rendering.
pub struct BatchedTextRun {
    pub text: String,
    pub fg: Hsla,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub start_col: usize,
}
