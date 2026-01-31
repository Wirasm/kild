//! Terminal backend implementations.

mod alacritty;
mod ghostty;
mod iterm;
mod terminal_app;

pub use alacritty::AlacrittyBackend;
pub use ghostty::GhosttyBackend;
pub use iterm::ITermBackend;
pub use terminal_app::TerminalAppBackend;
