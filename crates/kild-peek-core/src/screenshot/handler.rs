use std::io::Cursor;
use std::path::Path;

use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use tracing::info;

use super::errors::ScreenshotError;
use super::types::{CaptureRequest, CaptureResult, CaptureTarget, ImageFormat};

/// Capture a screenshot based on the request
pub fn capture(request: &CaptureRequest) -> Result<CaptureResult, ScreenshotError> {
    info!(event = "core.screenshot.capture_started", target = ?request.target);

    match &request.target {
        CaptureTarget::Window { title } => capture_window_by_title(title, &request.format),
        CaptureTarget::WindowId { id } => capture_window_by_id(*id, &request.format),
        CaptureTarget::Monitor { index } => capture_monitor(*index, &request.format),
        CaptureTarget::PrimaryMonitor => capture_primary_monitor(&request.format),
    }
}

/// Save a capture result to a file
pub fn save_to_file(result: &CaptureResult, path: &Path) -> Result<(), ScreenshotError> {
    info!(event = "core.screenshot.save_started", path = %path.display());

    std::fs::write(path, &result.data)?;

    info!(event = "core.screenshot.save_completed", path = %path.display());
    Ok(())
}

fn capture_window_by_title(
    title: &str,
    format: &ImageFormat,
) -> Result<CaptureResult, ScreenshotError> {
    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let title_lower = title.to_lowercase();
    let window = windows
        .into_iter()
        .find(|w| {
            w.title()
                .ok()
                .is_some_and(|t| t.to_lowercase().contains(&title_lower))
        })
        .ok_or_else(|| ScreenshotError::WindowNotFound {
            title: title.to_string(),
        })?;

    // Check if minimized
    if window.is_minimized().unwrap_or(false) {
        return Err(ScreenshotError::WindowMinimized {
            title: title.to_string(),
        });
    }

    let image = window
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}

fn capture_window_by_id(id: u32, format: &ImageFormat) -> Result<CaptureResult, ScreenshotError> {
    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let window = windows
        .into_iter()
        .find(|w| w.id().ok() == Some(id))
        .ok_or(ScreenshotError::WindowNotFoundById { id })?;

    // Check if minimized
    if window.is_minimized().unwrap_or(false) {
        let title = window.title().unwrap_or_else(|_| format!("Window {}", id));
        return Err(ScreenshotError::WindowMinimized { title });
    }

    let image = window
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}

fn capture_monitor(index: usize, format: &ImageFormat) -> Result<CaptureResult, ScreenshotError> {
    let monitors = xcap::Monitor::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let monitor = monitors
        .into_iter()
        .nth(index)
        .ok_or(ScreenshotError::MonitorNotFound { index })?;

    let image = monitor
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}

fn capture_primary_monitor(format: &ImageFormat) -> Result<CaptureResult, ScreenshotError> {
    let monitors = xcap::Monitor::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let monitor = monitors
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| xcap::Monitor::all().ok().and_then(|m| m.into_iter().next()))
        .ok_or(ScreenshotError::MonitorNotFound { index: 0 })?;

    let image = monitor
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}

fn encode_image(
    image: image::RgbaImage,
    format: &ImageFormat,
) -> Result<CaptureResult, ScreenshotError> {
    let width = image.width();
    let height = image.height();

    let mut buffer = Cursor::new(Vec::new());

    match format {
        ImageFormat::Png => {
            let encoder = PngEncoder::new(&mut buffer);
            encoder
                .write_image(&image, width, height, image::ExtendedColorType::Rgba8)
                .map_err(|e| ScreenshotError::EncodingError(e.to_string()))?;
        }
        ImageFormat::Jpeg { quality } => {
            let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
            let encoder = JpegEncoder::new_with_quality(&mut buffer, *quality);
            encoder
                .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
                .map_err(|e| ScreenshotError::EncodingError(e.to_string()))?;
        }
    }

    info!(
        event = "core.screenshot.capture_completed",
        width = width,
        height = height
    );

    Ok(CaptureResult {
        width,
        height,
        format: format.clone(),
        data: buffer.into_inner(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::PeekError;

    #[test]
    fn test_capture_nonexistent_window() {
        let request = CaptureRequest::window("NONEXISTENT_WINDOW_12345_UNIQUE");
        let result = capture(&request);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "SCREENSHOT_WINDOW_NOT_FOUND");
        }
    }

    #[test]
    fn test_capture_nonexistent_window_by_id() {
        let request = CaptureRequest::window_id(u32::MAX);
        let result = capture(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_capture_request_builder() {
        let request =
            CaptureRequest::window("Terminal").with_format(ImageFormat::Jpeg { quality: 85 });

        match &request.target {
            CaptureTarget::Window { title } => assert_eq!(title, "Terminal"),
            _ => panic!("Expected Window target"),
        }

        match &request.format {
            ImageFormat::Jpeg { quality } => assert_eq!(*quality, 85),
            _ => panic!("Expected JPEG format"),
        }
    }
}
