mod errors;
mod handler;
mod keymap;
mod types;

pub use errors::InteractionError;
pub use handler::{click, send_key_combo, type_text};
pub use types::{ClickRequest, InteractionResult, InteractionTarget, KeyComboRequest, TypeRequest};
