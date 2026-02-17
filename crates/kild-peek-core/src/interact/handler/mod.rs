mod click;
mod helpers;
mod keyboard;
mod mouse;

pub use click::{click, click_text};
pub use keyboard::{send_key_combo, type_text};
pub use mouse::{drag, hover, hover_text, scroll};

// Private imports for test access via `use super::*`
#[cfg(test)]
use crate::interact::errors::InteractionError;
#[cfg(test)]
use crate::interact::types::{ClickTextRequest, InteractionTarget};
#[cfg(test)]
use crate::window::{WindowError, WindowInfo};
#[cfg(test)]
use helpers::{map_window_error, to_screen_coordinates, validate_coordinates};

#[cfg(test)]
mod tests;
