use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

/// Target for screenshot capture
#[derive(Debug, Clone)]
pub enum CaptureTarget {
    /// Capture a window by title (partial match)
    Window { title: String },
    /// Capture a window by ID
    WindowId { id: u32 },
    /// Capture a window by app name
    WindowApp { app: String },
    /// Capture a window by app name and title (for precision)
    WindowAppAndTitle { app: String, title: String },
    /// Capture a specific monitor by index
    Monitor { index: usize },
    /// Capture the primary monitor
    PrimaryMonitor,
}

/// Image format for screenshot output
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg {
        quality: u8,
    },
}

/// Request to capture a screenshot
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// What to capture
    pub target: CaptureTarget,
    /// Output format
    pub format: ImageFormat,
}

impl CaptureRequest {
    /// Create a new capture request for a window by title
    pub fn window(title: impl Into<String>) -> Self {
        Self {
            target: CaptureTarget::Window {
                title: title.into(),
            },
            format: ImageFormat::default(),
        }
    }

    /// Create a new capture request for a window by ID
    pub fn window_id(id: u32) -> Self {
        Self {
            target: CaptureTarget::WindowId { id },
            format: ImageFormat::default(),
        }
    }

    /// Create a new capture request for a window by app name
    pub fn window_app(app: impl Into<String>) -> Self {
        Self {
            target: CaptureTarget::WindowApp { app: app.into() },
            format: ImageFormat::default(),
        }
    }

    /// Create a new capture request for a window by app name and title
    pub fn window_app_and_title(app: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            target: CaptureTarget::WindowAppAndTitle {
                app: app.into(),
                title: title.into(),
            },
            format: ImageFormat::default(),
        }
    }

    /// Create a new capture request for a monitor by index
    pub fn monitor(index: usize) -> Self {
        Self {
            target: CaptureTarget::Monitor { index },
            format: ImageFormat::default(),
        }
    }

    /// Create a new capture request for the primary monitor
    pub fn primary_monitor() -> Self {
        Self {
            target: CaptureTarget::PrimaryMonitor,
            format: ImageFormat::default(),
        }
    }

    /// Set the output format
    pub fn with_format(mut self, format: ImageFormat) -> Self {
        self.format = format;
        self
    }

    /// Set JPEG format with quality clamped to valid range (0-100)
    pub fn with_jpeg_quality(mut self, quality: u8) -> Self {
        self.format = ImageFormat::Jpeg {
            quality: quality.min(100),
        };
        self
    }
}

/// Result of a screenshot capture
#[derive(Debug)]
pub struct CaptureResult {
    width: u32,
    height: u32,
    format: ImageFormat,
    data: Vec<u8>,
}

impl CaptureResult {
    /// Create a new capture result. Internal use only.
    pub(crate) fn new(width: u32, height: u32, format: ImageFormat, data: Vec<u8>) -> Self {
        debug_assert!(width > 0, "Width must be positive");
        debug_assert!(height > 0, "Height must be positive");
        debug_assert!(!data.is_empty(), "Data must not be empty");

        Self {
            width,
            height,
            format,
            data,
        }
    }

    /// Image width in pixels
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Output format
    pub fn format(&self) -> &ImageFormat {
        &self.format
    }

    /// Encoded image bytes (PNG or JPEG)
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Convert the captured image to a base64 string
    pub fn to_base64(&self) -> String {
        STANDARD.encode(&self.data)
    }

    /// Get the MIME type for the image format
    pub fn mime_type(&self) -> &'static str {
        match self.format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg { .. } => "image/jpeg",
        }
    }

    /// Get a data URI for the image (for embedding in HTML/Markdown)
    pub fn to_data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime_type(), self.to_base64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_request_window() {
        let req = CaptureRequest::window("Terminal");
        match req.target {
            CaptureTarget::Window { title } => assert_eq!(title, "Terminal"),
            _ => panic!("Expected Window target"),
        }
    }

    #[test]
    fn test_capture_request_with_format() {
        let req = CaptureRequest::window("Test").with_format(ImageFormat::Jpeg { quality: 90 });
        match req.format {
            ImageFormat::Jpeg { quality } => assert_eq!(quality, 90),
            _ => panic!("Expected JPEG format"),
        }
    }

    #[test]
    fn test_capture_result_mime_type() {
        let png_result = CaptureResult::new(100, 100, ImageFormat::Png, vec![0x89]);
        assert_eq!(png_result.mime_type(), "image/png");

        let jpg_result =
            CaptureResult::new(100, 100, ImageFormat::Jpeg { quality: 85 }, vec![0xFF]);
        assert_eq!(jpg_result.mime_type(), "image/jpeg");
    }

    #[test]
    fn test_capture_result_to_base64() {
        let result = CaptureResult::new(
            1,
            1,
            ImageFormat::Png,
            vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes
        );
        let base64 = result.to_base64();
        assert!(!base64.is_empty());
    }

    #[test]
    fn test_capture_result_to_data_uri() {
        let result = CaptureResult::new(1, 1, ImageFormat::Png, vec![0x89, 0x50, 0x4E, 0x47]);
        let uri = result.to_data_uri();
        assert!(uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_capture_result_getters() {
        let result = CaptureResult::new(800, 600, ImageFormat::Png, vec![0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(result.width(), 800);
        assert_eq!(result.height(), 600);
        assert_eq!(result.data().len(), 4);
        assert!(matches!(result.format(), ImageFormat::Png));
    }

    #[test]
    fn test_capture_request_jpeg_quality_clamped() {
        // Normal quality
        let req = CaptureRequest::window("Test").with_jpeg_quality(85);
        match req.format {
            ImageFormat::Jpeg { quality } => assert_eq!(quality, 85),
            _ => panic!("Expected JPEG format"),
        }

        // Quality > 100 should be clamped to 100
        let req_high = CaptureRequest::window("Test").with_jpeg_quality(150);
        match req_high.format {
            ImageFormat::Jpeg { quality } => assert_eq!(quality, 100),
            _ => panic!("Expected JPEG format"),
        }

        // Quality 0 is valid
        let req_zero = CaptureRequest::window("Test").with_jpeg_quality(0);
        match req_zero.format {
            ImageFormat::Jpeg { quality } => assert_eq!(quality, 0),
            _ => panic!("Expected JPEG format"),
        }
    }
}
