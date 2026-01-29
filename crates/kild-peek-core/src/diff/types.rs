use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request to compare two images
#[derive(Debug, Clone)]
pub struct DiffRequest {
    /// Path to the first image
    pub image1_path: PathBuf,
    /// Path to the second image
    pub image2_path: PathBuf,
    /// Similarity threshold (0.0 - 1.0, default 0.95)
    pub threshold: f64,
    /// Optional path to save a visual diff image highlighting differences
    pub diff_output_path: Option<PathBuf>,
}

impl DiffRequest {
    /// Create a new diff request with default threshold
    pub fn new(image1: impl Into<PathBuf>, image2: impl Into<PathBuf>) -> Self {
        Self {
            image1_path: image1.into(),
            image2_path: image2.into(),
            threshold: 0.95,
            diff_output_path: None,
        }
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set the path to save a visual diff image
    pub fn with_diff_output(mut self, path: impl Into<PathBuf>) -> Self {
        self.diff_output_path = Some(path.into());
        self
    }
}

/// Result of comparing two images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    similarity: f64,
    is_similar: bool,
    width1: u32,
    height1: u32,
    width2: u32,
    height2: u32,
    threshold: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_output_path: Option<String>,
}

impl DiffResult {
    /// Create a new DiffResult. The is_similar field is computed from similarity >= threshold.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        similarity: f64,
        width1: u32,
        height1: u32,
        width2: u32,
        height2: u32,
        threshold: f64,
        diff_output_path: Option<String>,
    ) -> Self {
        Self {
            similarity,
            is_similar: similarity >= threshold,
            width1,
            height1,
            width2,
            height2,
            threshold,
            diff_output_path,
        }
    }

    /// Structural similarity score (0.0 - 1.0, where 1.0 is identical)
    pub fn similarity(&self) -> f64 {
        self.similarity
    }

    /// Whether the images meet the similarity threshold (computed as similarity >= threshold)
    pub fn is_similar(&self) -> bool {
        self.is_similar
    }

    /// First image width
    pub fn width1(&self) -> u32 {
        self.width1
    }

    /// First image height
    pub fn height1(&self) -> u32 {
        self.height1
    }

    /// Second image width
    pub fn width2(&self) -> u32 {
        self.width2
    }

    /// Second image height
    pub fn height2(&self) -> u32 {
        self.height2
    }

    /// Threshold used for comparison
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Get similarity as a percentage string
    pub fn similarity_percent(&self) -> String {
        format!("{:.1}%", self.similarity * 100.0)
    }

    /// Path where the visual diff image was saved, if requested
    pub fn diff_output_path(&self) -> Option<&str> {
        self.diff_output_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_request_new() {
        let request = DiffRequest::new("/path/to/image1.png", "/path/to/image2.png");
        assert_eq!(request.image1_path, PathBuf::from("/path/to/image1.png"));
        assert_eq!(request.image2_path, PathBuf::from("/path/to/image2.png"));
        assert!((request.threshold - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_diff_request_with_threshold() {
        let request =
            DiffRequest::new("/path/to/image1.png", "/path/to/image2.png").with_threshold(0.80);
        assert!((request.threshold - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn test_diff_request_threshold_clamped() {
        let request_high =
            DiffRequest::new("/path/to/image1.png", "/path/to/image2.png").with_threshold(1.5);
        assert!((request_high.threshold - 1.0).abs() < f64::EPSILON);

        let request_low =
            DiffRequest::new("/path/to/image1.png", "/path/to/image2.png").with_threshold(-0.5);
        assert!(request_low.threshold.abs() < f64::EPSILON);
    }

    #[test]
    fn test_diff_result_similarity_percent() {
        let result = DiffResult::new(0.956, 100, 100, 100, 100, 0.95, None);
        assert_eq!(result.similarity_percent(), "95.6%");
    }

    #[test]
    fn test_diff_result_is_similar_computed() {
        // Similarity >= threshold => is_similar = true
        let result_pass = DiffResult::new(0.96, 100, 100, 100, 100, 0.95, None);
        assert!(result_pass.is_similar());
        assert!((result_pass.similarity() - 0.96).abs() < f64::EPSILON);

        // Similarity exactly at threshold => is_similar = true
        let result_exact = DiffResult::new(0.95, 100, 100, 100, 100, 0.95, None);
        assert!(result_exact.is_similar());

        // Similarity < threshold => is_similar = false
        let result_fail = DiffResult::new(0.94, 100, 100, 100, 100, 0.95, None);
        assert!(!result_fail.is_similar());
    }

    #[test]
    fn test_diff_result_getters() {
        let result = DiffResult::new(0.85, 800, 600, 800, 600, 0.80, None);
        assert_eq!(result.width1(), 800);
        assert_eq!(result.height1(), 600);
        assert_eq!(result.width2(), 800);
        assert_eq!(result.height2(), 600);
        assert!((result.threshold() - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn test_diff_request_with_diff_output() {
        let request = DiffRequest::new("/a.png", "/b.png").with_diff_output("/tmp/diff.png");
        assert_eq!(
            request.diff_output_path,
            Some(PathBuf::from("/tmp/diff.png"))
        );
    }

    #[test]
    fn test_diff_request_default_no_diff_output() {
        let request = DiffRequest::new("/a.png", "/b.png");
        assert!(request.diff_output_path.is_none());
    }

    #[test]
    fn test_diff_result_with_diff_output_path() {
        let result = DiffResult::new(
            0.85,
            100,
            100,
            100,
            100,
            0.80,
            Some("/tmp/diff.png".to_string()),
        );
        assert_eq!(result.diff_output_path(), Some("/tmp/diff.png"));
    }

    #[test]
    fn test_diff_result_without_diff_output_path() {
        let result = DiffResult::new(0.85, 100, 100, 100, 100, 0.80, None);
        assert_eq!(result.diff_output_path(), None);
    }

    #[test]
    fn test_diff_result_json_includes_diff_output_path_when_set() {
        let result = DiffResult::new(
            0.85,
            100,
            100,
            100,
            100,
            0.80,
            Some("/tmp/diff.png".to_string()),
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"diff_output_path\""));
        assert!(json.contains("/tmp/diff.png"));
    }

    #[test]
    fn test_diff_result_json_omits_diff_output_path_when_none() {
        let result = DiffResult::new(0.85, 100, 100, 100, 100, 0.80, None);
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("diff_output_path"));
    }
}
