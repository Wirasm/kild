use serde::{Deserialize, Serialize};

/// Configuration for including files that override gitignore rules.
///
/// When creating a new kild, files matching these patterns will be copied
/// from the source repository even if they are in .gitignore.
///
/// # Examples
///
/// ```
/// use kild_core::files::types::IncludeConfig;
///
/// let config = IncludeConfig {
///     patterns: vec![".env*".to_string(), "*.local.json".to_string()],
///     enabled: true,
///     max_file_size: Some("10MB".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeConfig {
    /// Glob patterns to match against relative file paths.
    /// Examples: ".env*", "*.local.json", "build/artifacts/**"
    ///
    /// When deserialized from TOML, defaults to `default_include_patterns()`
    /// if not specified.
    #[serde(default = "default_include_patterns")]
    pub patterns: Vec<String>,

    /// Whether include pattern copying is enabled. Defaults to true.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Optional maximum file size limit (e.g., "10MB", "1GB").
    /// Files exceeding this limit will be skipped with a warning.
    #[serde(default)]
    pub max_file_size: Option<String>,
}

impl IncludeConfig {
    /// Validate that all patterns are valid glob patterns.
    ///
    /// Returns an error if any pattern is invalid.
    pub fn validate(&self) -> Result<(), String> {
        for pattern in &self.patterns {
            glob::Pattern::new(pattern)
                .map_err(|e| format!("Invalid pattern '{}': {}", pattern, e))?;
        }
        Ok(())
    }
}

/// A compiled glob pattern rule for matching files.
///
/// This is an internal type used by the file operations module.
/// Users should work with `IncludeConfig` instead.
#[derive(Debug, Clone)]
pub struct PatternRule {
    /// Original pattern string for logging and error messages
    pub pattern: String,
    /// Compiled glob pattern for efficient matching
    pub compiled: glob::Pattern,
}

/// Options for copying files safely with validation.
#[derive(Debug, Clone)]
pub struct CopyOptions {
    /// Optional maximum file size in bytes
    pub max_file_size: Option<u64>,
}

fn default_enabled() -> bool {
    true
}

/// Returns the default include patterns.
///
/// These patterns provide sensible defaults for common use cases:
/// - `.env*` - Environment files
/// - `*.local.json` - Local config files
/// - `.claude/**` - Claude AI context files
/// - `.cursor/**` - Cursor AI context files
pub fn default_include_patterns() -> Vec<String> {
    vec![
        ".env*".to_string(),
        "*.local.json".to_string(),
        ".claude/**".to_string(),
        ".cursor/**".to_string(),
    ]
}

impl Default for IncludeConfig {
    fn default() -> Self {
        Self {
            patterns: default_include_patterns(),
            enabled: true,
            max_file_size: None,
        }
    }
}
