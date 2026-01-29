use std::path::Path;

use image::GenericImageView;
use image_compare::Algorithm;
use tracing::{debug, info};

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

    // Generate visual diff image if output path requested
    let diff_output_path = if let Some(ref output_path) = request.diff_output_path {
        save_diff_image(&img1, &img2, output_path)?;
        Some(output_path.display().to_string())
    } else {
        None
    };

    let diff_result = DiffResult::new(
        similarity,
        width1,
        height1,
        width2,
        height2,
        request.threshold,
        diff_output_path,
    );

    info!(
        event = "core.diff.compare_completed",
        similarity = similarity,
        is_similar = diff_result.is_similar()
    );

    Ok(diff_result)
}

/// Compute per-pixel absolute differences between two images and save as PNG
fn save_diff_image(
    img1: &image::DynamicImage,
    img2: &image::DynamicImage,
    output_path: &Path,
) -> Result<(), DiffError> {
    info!(
        event = "core.diff.save_started",
        path = %output_path.display()
    );

    let img1_rgba = img1.to_rgba8();
    let img2_rgba = img2.to_rgba8();
    let (width, height) = img1.dimensions();

    let mut diff_img = image::RgbImage::new(width, height);
    for (x, y, p1) in img1_rgba.enumerate_pixels() {
        let p2 = img2_rgba.get_pixel(x, y);
        diff_img.put_pixel(
            x,
            y,
            image::Rgb([
                p1[0].abs_diff(p2[0]),
                p1[1].abs_diff(p2[1]),
                p1[2].abs_diff(p2[2]),
            ]),
        );
    }

    // Create parent directory if needed
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        debug!(
            event = "core.diff.creating_parent_directory",
            path = %parent.display()
        );
        std::fs::create_dir_all(parent).map_err(|e| {
            DiffError::DiffGenerationFailed(format!(
                "failed to create output directory '{}': {}",
                parent.display(),
                e
            ))
        })?;
    }

    image::DynamicImage::ImageRgb8(diff_img)
        .save(output_path)
        .map_err(|e| DiffError::DiffGenerationFailed(e.to_string()))?;

    info!(
        event = "core.diff.save_completed",
        path = %output_path.display()
    );

    Ok(())
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

    /// Create a 2x2 test PNG with known pixel colors
    fn create_test_image(name: &str, pixels: [[u8; 3]; 4]) -> PathBuf {
        use image::RgbImage;
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb(pixels[0]));
        img.put_pixel(1, 0, image::Rgb(pixels[1]));
        img.put_pixel(0, 1, image::Rgb(pixels[2]));
        img.put_pixel(1, 1, image::Rgb(pixels[3]));

        let dir = std::env::temp_dir().join("kild_peek_test_diff");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{}_{}.png", name, std::process::id()));
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn test_compare_identical_images_without_diff_output() {
        let pixels = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128]];
        let img1 = create_test_image("identical_a", pixels);
        let img2 = create_test_image("identical_b", pixels);

        let request = DiffRequest::new(&img1, &img2);
        let result = compare_images(&request).unwrap();

        assert!((result.similarity() - 1.0).abs() < f64::EPSILON);
        assert!(result.is_similar());
        assert_eq!(result.diff_output_path(), None);

        let _ = std::fs::remove_file(&img1);
        let _ = std::fs::remove_file(&img2);
    }

    #[test]
    fn test_compare_images_saves_diff_output() {
        let img1 = create_test_image(
            "diff_a",
            [[100, 0, 0], [0, 100, 0], [0, 0, 100], [50, 50, 50]],
        );
        let img2 = create_test_image(
            "diff_b",
            [[200, 0, 0], [0, 200, 0], [0, 0, 200], [150, 150, 150]],
        );

        let diff_dir = std::env::temp_dir().join("kild_peek_test_diff_output");
        let _ = std::fs::remove_dir_all(&diff_dir);
        let diff_path = diff_dir.join("diff.png");

        let request = DiffRequest::new(&img1, &img2).with_diff_output(&diff_path);
        let result = compare_images(&request).unwrap();

        // Verify diff_output_path is propagated to result
        assert_eq!(
            result.diff_output_path(),
            Some(diff_path.display().to_string().as_str())
        );

        // Verify file was created
        assert!(diff_path.exists());

        // Verify pixel values are correct absolute differences
        let diff_img = image::open(&diff_path).unwrap().to_rgb8();
        let p00 = diff_img.get_pixel(0, 0);
        assert_eq!(p00[0], 100); // |100 - 200| = 100
        assert_eq!(p00[1], 0); // |0 - 0| = 0
        assert_eq!(p00[2], 0); // |0 - 0| = 0

        let p11 = diff_img.get_pixel(1, 1);
        assert_eq!(p11[0], 100); // |50 - 150| = 100
        assert_eq!(p11[1], 100);
        assert_eq!(p11[2], 100);

        let _ = std::fs::remove_file(&img1);
        let _ = std::fs::remove_file(&img2);
        let _ = std::fs::remove_dir_all(&diff_dir);
    }

    #[test]
    fn test_diff_output_creates_nested_parent_directories() {
        let black_pixels = [[0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]];
        let img1 = create_test_image("nested_a", black_pixels);
        let img2 = create_test_image("nested_b", black_pixels);

        let nested_dir = std::env::temp_dir().join("kild_peek_test_diff_nested");
        let _ = std::fs::remove_dir_all(&nested_dir);
        let diff_path = nested_dir.join("deeply").join("nested").join("diff.png");

        let request = DiffRequest::new(&img1, &img2).with_diff_output(&diff_path);
        let result = compare_images(&request);

        assert!(result.is_ok());
        assert!(diff_path.exists());

        let _ = std::fs::remove_file(&img1);
        let _ = std::fs::remove_file(&img2);
        let _ = std::fs::remove_dir_all(&nested_dir);
    }
}
