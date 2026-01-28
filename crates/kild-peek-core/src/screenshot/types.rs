use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

/// Target for screenshot capture
#[derive(Debug, Clone)]
pub enum CaptureTarget {
    /// Capture a window by title (partial match)
    Window { title: String },
    /// Capture a window by ID
    WindowId { id: u32 },
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
}

/// Result of a screenshot capture
#[derive(Debug)]
pub struct CaptureResult {
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Output format
    pub format: ImageFormat,
    /// Encoded image bytes (PNG or JPEG)
    pub data: Vec<u8>,
}

impl CaptureResult {
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
        let png_result = CaptureResult {
            width: 100,
            height: 100,
            format: ImageFormat::Png,
            data: vec![],
        };
        assert_eq!(png_result.mime_type(), "image/png");

        let jpg_result = CaptureResult {
            width: 100,
            height: 100,
            format: ImageFormat::Jpeg { quality: 85 },
            data: vec![],
        };
        assert_eq!(jpg_result.mime_type(), "image/jpeg");
    }

    #[test]
    fn test_capture_result_to_base64() {
        let result = CaptureResult {
            width: 1,
            height: 1,
            format: ImageFormat::Png,
            data: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes
        };
        let base64 = result.to_base64();
        assert!(!base64.is_empty());
    }

    #[test]
    fn test_capture_result_to_data_uri() {
        let result = CaptureResult {
            width: 1,
            height: 1,
            format: ImageFormat::Png,
            data: vec![0x89, 0x50, 0x4E, 0x47],
        };
        let uri = result.to_data_uri();
        assert!(uri.starts_with("data:image/png;base64,"));
    }
}
