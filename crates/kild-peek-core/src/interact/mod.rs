mod errors;
mod handler;
mod operations;
mod types;

pub use errors::InteractionError;
pub use handler::{click, click_text, send_key_combo, type_text};
pub use types::{
    ClickRequest, ClickTextRequest, InteractionResult, InteractionTarget, KeyComboRequest,
    TypeRequest,
};
