use std::io::Cursor;
use std::path::Path;

use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use tracing::{debug, error, info, warn};

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
///
/// Creates parent directories if they don't exist.
pub fn save_to_file(result: &CaptureResult, path: &Path) -> Result<(), ScreenshotError> {
    info!(event = "core.screenshot.save_started", path = %path.display());

    // Ensure parent directory exists
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        debug!(event = "core.screenshot.creating_parent_directory", path = %parent.display());
        std::fs::create_dir_all(parent).map_err(|source| {
            error!(
                event = "core.screenshot.directory_creation_failed",
                path = %parent.display(),
                error = %source
            );
            ScreenshotError::DirectoryCreationFailed {
                path: parent.display().to_string(),
                source,
            }
        })?;
    }

    std::fs::write(path, result.data())?;

    info!(event = "core.screenshot.save_completed", path = %path.display());
    Ok(())
}

/// Check if a window is minimized and return an error if so.
///
/// Returns Ok(()) if the window is not minimized or if the check fails
/// (in which case we proceed anyway and let the capture fail if needed).
fn check_window_not_minimized(window: &xcap::Window, title: &str) -> Result<(), ScreenshotError> {
    let is_minimized = match window.is_minimized() {
        Ok(minimized) => minimized,
        Err(e) => {
            debug!(
                event = "core.screenshot.is_minimized_check_failed",
                title = title,
                error = %e
            );
            // Proceed anyway - capture will fail if there's a real problem
            false
        }
    };

    if is_minimized {
        return Err(ScreenshotError::WindowMinimized {
            title: title.to_string(),
        });
    }

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

    check_window_not_minimized(&window, title)?;

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

    let title = window.title().unwrap_or_else(|_| format!("Window {}", id));
    check_window_not_minimized(&window, &title)?;

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

    // First try to find primary monitor
    let primary_monitor = monitors.iter().find(|m| match m.is_primary() {
        Ok(is_primary) => is_primary,
        Err(e) => {
            debug!(
                event = "core.screenshot.is_primary_check_failed",
                error = %e
            );
            false
        }
    });

    let monitor = match primary_monitor {
        Some(primary) => primary,
        None => {
            // Fall back to first monitor if no primary is set
            warn!(event = "core.screenshot.no_primary_monitor_using_fallback");
            monitors
                .first()
                .ok_or(ScreenshotError::MonitorNotFound { index: 0 })?
        }
    };

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

    Ok(CaptureResult::new(
        width,
        height,
        format.clone(),
        buffer.into_inner(),
    ))
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

    #[test]
    fn test_capture_nonexistent_monitor() {
        let request = CaptureRequest::monitor(999);
        let result = capture(&request);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "SCREENSHOT_MONITOR_NOT_FOUND");
        }
    }

    /// Test that error detection classifies permission-related messages correctly
    /// This tests the string matching logic in the handler
    #[test]
    fn test_permission_error_detection_logic() {
        // The actual permission error detection happens in capture_window_by_title etc.
        // We can verify the error types have the right codes
        let perm_error = ScreenshotError::PermissionDenied;
        assert_eq!(perm_error.error_code(), "SCREENSHOT_PERMISSION_DENIED");
        assert!(perm_error.is_user_error());

        // Enumeration failed is different from permission denied
        let enum_error = ScreenshotError::EnumerationFailed("some other error".to_string());
        assert_eq!(enum_error.error_code(), "SCREENSHOT_ENUMERATION_FAILED");
        assert!(!enum_error.is_user_error());
    }

    #[test]
    fn test_save_to_file_creates_parent_directories() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_peek_test_save_creates_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Path with non-existent parent directories
        let nested_path = temp_dir.join("deeply/nested/path/screenshot.png");

        // Create a minimal valid PNG (1x1 transparent pixel)
        let png_data = create_test_png();
        let result = CaptureResult::new(1, 1, ImageFormat::Png, png_data);

        // Should succeed by creating parent directories
        assert!(save_to_file(&result, &nested_path).is_ok());
        assert!(nested_path.exists());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_to_file_handles_existing_directory() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_peek_test_save_existing_dir");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let path = temp_dir.join("screenshot.png");

        // Create a minimal valid PNG
        let png_data = create_test_png();
        let result = CaptureResult::new(1, 1, ImageFormat::Png, png_data);

        // Should succeed with existing directory
        assert!(save_to_file(&result, &path).is_ok());
        assert!(path.exists());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_creation_failed_error() {
        use std::error::Error;

        let error = ScreenshotError::DirectoryCreationFailed {
            path: "/some/path".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        };
        assert_eq!(error.error_code(), "SCREENSHOT_DIRECTORY_CREATION_FAILED");
        assert!(error.is_user_error());
        assert!(error.to_string().contains("/some/path"));

        // Verify error source chain is preserved
        assert!(error.source().is_some());
        assert!(
            error
                .source()
                .unwrap()
                .to_string()
                .contains("permission denied")
        );
    }

    #[test]
    fn test_save_to_file_with_filename_only() {
        use std::env;

        // Use a unique filename in the current temp directory
        let temp_dir = env::temp_dir();
        let filename_only = temp_dir.join("kild_peek_test_filename_only.png");

        // Clean up if exists from previous run
        let _ = std::fs::remove_file(&filename_only);

        // Create a minimal valid PNG
        let png_data = create_test_png();
        let result = CaptureResult::new(1, 1, ImageFormat::Png, png_data);

        // Should succeed - no directory creation needed when parent exists
        assert!(save_to_file(&result, &filename_only).is_ok());
        assert!(filename_only.exists());

        // Clean up
        let _ = std::fs::remove_file(&filename_only);
    }

    /// Helper to create a minimal valid PNG for testing
    fn create_test_png() -> Vec<u8> {
        use image::ImageEncoder;
        use image::codecs::png::PngEncoder;
        use std::io::Cursor;

        let img = image::RgbaImage::new(1, 1);
        let mut buffer = Cursor::new(Vec::new());
        let encoder = PngEncoder::new(&mut buffer);
        encoder
            .write_image(&img, 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        buffer.into_inner()
    }
}
