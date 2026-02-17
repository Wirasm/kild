mod element;
mod paint;
mod prepaint;
#[cfg(test)]
mod tests;
mod types;

pub use element::TerminalElement;
pub(crate) use element::scroll_delta_lines;
pub(crate) use types::MouseState;
