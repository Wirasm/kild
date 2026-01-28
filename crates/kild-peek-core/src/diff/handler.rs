use image::GenericImageView;
use image_compare::Algorithm;
use tracing::info;

use super::errors::DiffError;
use super::types::{DiffRequest, DiffResult};

/// Compare two images and calculate their similarity using SSIM (Structural Similarity Index)
///
/// # Errors
///
/// Returns [`DiffError::ImageLoadFailed`] if either image cannot be loaded (file not found,
/// invalid format, or permission denied).
///
/// Returns [`DiffError::DimensionMismatch`] if the images have different dimensions.
///
/// Returns [`DiffError::ComparisonFailed`] if the SSIM calculation fails.
pub fn compare_images(request: &DiffRequest) -> Result<DiffResult, DiffError> {
    info!(
        event = "core.diff.compare_started",
        image1 = %request.image1_path.display(),
        image2 = %request.image2_path.display(),
        threshold = request.threshold
    );

    // Load images
    let img1 = image::open(&request.image1_path).map_err(|e| DiffError::ImageLoadFailed {
        path: request.image1_path.display().to_string(),
        message: e.to_string(),
    })?;

    let img2 = image::open(&request.image2_path).map_err(|e| DiffError::ImageLoadFailed {
        path: request.image2_path.display().to_string(),
        message: e.to_string(),
    })?;

    let (width1, height1) = img1.dimensions();
    let (width2, height2) = img2.dimensions();

    // Check dimensions match
    if width1 != width2 || height1 != height2 {
        return Err(DiffError::DimensionMismatch {
            width1,
            height1,
            width2,
            height2,
        });
    }

    // Convert to grayscale for SSIM comparison
    let gray1 = img1.to_luma8();
    let gray2 = img2.to_luma8();

    // Calculate SSIM (Structural Similarity Index)
    let result = image_compare::gray_similarity_structure(&Algorithm::MSSIMSimple, &gray1, &gray2)
        .map_err(|e| DiffError::ComparisonFailed(e.to_string()))?;

    let similarity = result.score;

    let diff_result = DiffResult::new(
        similarity,
        width1,
        height1,
        width2,
        height2,
        request.threshold,
    );

    info!(
        event = "core.diff.compare_completed",
        similarity = similarity,
        is_similar = diff_result.is_similar()
    );

    Ok(diff_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::PeekError;
    use std::path::PathBuf;

    #[test]
    fn test_compare_nonexistent_image() {
        let request = DiffRequest::new(
            PathBuf::from("/nonexistent/image1.png"),
            PathBuf::from("/nonexistent/image2.png"),
        );
        let result = compare_images(&request);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "DIFF_IMAGE_LOAD_FAILED");
        }
    }
}
