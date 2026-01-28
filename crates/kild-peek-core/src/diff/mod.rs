mod errors;
mod handler;
mod types;

pub use errors::DiffError;
pub use handler::compare_images;
pub use types::{DiffRequest, DiffResult};
