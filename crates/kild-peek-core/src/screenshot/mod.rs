mod errors;
mod handler;
mod types;

pub use errors::ScreenshotError;
pub use handler::{capture, save_to_file};
pub use types::{CaptureRequest, CaptureResult, CaptureTarget, CropArea, ImageFormat};
