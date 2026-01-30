mod errors;
mod handler;
mod operations;
mod types;

pub use errors::InteractionError;
pub use handler::{click, click_text, drag, hover, hover_text, scroll, send_key_combo, type_text};
pub use types::{
    ClickModifier, ClickRequest, ClickTextRequest, DragRequest, HoverRequest, HoverTextRequest,
    InteractionResult, InteractionTarget, KeyComboRequest, ScrollRequest, TypeRequest,
};
