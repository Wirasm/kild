use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("Failed to load image '{path}': {message}")]
    ImageLoadFailed { path: String, message: String },

    #[error("Image dimensions do not match: {width1}x{height1} vs {width2}x{height2}")]
    DimensionMismatch {
        width1: u32,
        height1: u32,
        width2: u32,
        height2: u32,
    },

    #[error("Failed to compare images: {0}")]
    ComparisonFailed(String),

    #[error("Failed to generate diff image: {0}")]
    DiffGenerationFailed(String),

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl PeekError for DiffError {
    fn error_code(&self) -> &'static str {
        match self {
            DiffError::ImageLoadFailed { .. } => "DIFF_IMAGE_LOAD_FAILED",
            DiffError::DimensionMismatch { .. } => "DIFF_DIMENSION_MISMATCH",
            DiffError::ComparisonFailed(_) => "DIFF_COMPARISON_FAILED",
            DiffError::DiffGenerationFailed(_) => "DIFF_GENERATION_FAILED",
            DiffError::IoError { .. } => "DIFF_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            DiffError::ImageLoadFailed { .. } | DiffError::DimensionMismatch { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_diff_error_display() {
        let error = DiffError::ImageLoadFailed {
            path: "/path/to/image.png".to_string(),
            message: "file not found".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to load image '/path/to/image.png': file not found"
        );
        assert_eq!(error.error_code(), "DIFF_IMAGE_LOAD_FAILED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let error = DiffError::DimensionMismatch {
            width1: 100,
            height1: 200,
            width2: 150,
            height2: 200,
        };
        assert_eq!(
            error.to_string(),
            "Image dimensions do not match: 100x200 vs 150x200"
        );
        assert_eq!(error.error_code(), "DIFF_DIMENSION_MISMATCH");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_comparison_failed_error() {
        let error = DiffError::ComparisonFailed("SSIM calculation failed".to_string());
        assert_eq!(
            error.to_string(),
            "Failed to compare images: SSIM calculation failed"
        );
        assert_eq!(error.error_code(), "DIFF_COMPARISON_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_diff_generation_failed_error() {
        let error = DiffError::DiffGenerationFailed("write failed".to_string());
        assert_eq!(
            error.to_string(),
            "Failed to generate diff image: write failed"
        );
        assert_eq!(error.error_code(), "DIFF_GENERATION_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: DiffError = io_error.into();
        assert_eq!(error.error_code(), "DIFF_IO_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DiffError>();
    }

    #[test]
    fn test_error_source() {
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let error: DiffError = io_error.into();
        assert!(error.source().is_some());
    }
}
