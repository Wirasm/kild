use crate::files::{
    errors::FileError,
    types::{CopyOptions, IncludeConfig, PatternRule},
};
use glob::Pattern;
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tracing::{debug, warn};

/// Validate glob patterns in the include config
pub fn validate_patterns(config: &IncludeConfig) -> Result<Vec<PatternRule>, FileError> {
    let mut rules = Vec::new();

    for pattern_str in &config.patterns {
        match Pattern::new(pattern_str) {
            Ok(compiled) => {
                rules.push(PatternRule {
                    pattern: pattern_str.clone(),
                    compiled,
                });
            }
            Err(e) => {
                return Err(FileError::InvalidPattern {
                    pattern: pattern_str.clone(),
                    message: e.to_string(),
                });
            }
        }
    }

    debug!(
        event = "core.files.patterns.validated",
        pattern_count = rules.len(),
        patterns = ?config.patterns
    );

    Ok(rules)
}

/// Find files matching include patterns, overriding gitignore.
///
/// Patterns are matched against relative paths from source_root.
///
/// # Pattern Examples
/// - `.env*` matches .env, .env.local, .env.production
/// - `*.local.json` matches any .local.json file in any directory
/// - `build/artifacts/**` matches all files under build/artifacts/
///
/// # How It Works
/// Patterns use glob syntax and are checked AFTER gitignore rules,
/// effectively overriding gitignore for matching files. The `ignore`
/// crate's override mechanism ensures gitignored files matching these
/// patterns are still included.
pub fn find_matching_files(
    source_root: &Path,
    rules: &[PatternRule],
) -> Result<Vec<PathBuf>, FileError> {
    let mut matching_files = Vec::new();

    // Create override builder to ignore gitignore for our patterns
    let mut override_builder = ignore::overrides::OverrideBuilder::new(source_root);

    // Add each pattern as an override (this makes gitignore ignore them)
    for rule in rules {
        if let Err(e) = override_builder.add(&rule.pattern) {
            return Err(FileError::ValidationError {
                message: format!(
                    "Failed to add override for pattern '{}': {}",
                    rule.pattern, e
                ),
            });
        }
    }

    let overrides = override_builder
        .build()
        .map_err(|e| FileError::ValidationError {
            message: format!("Failed to build overrides: {}", e),
        })?;

    // Walk the directory with overrides
    let walker = WalkBuilder::new(source_root)
        .overrides(overrides)
        .hidden(false) // Include hidden files
        .git_ignore(true) // Still respect gitignore for non-overridden files
        .build();

    for entry in walker {
        match entry {
            Ok(entry) => {
                let path = entry.path();

                // Skip directories
                if path.is_dir() {
                    continue;
                }

                // Get relative path for pattern matching
                let relative_path = match path.strip_prefix(source_root) {
                    Ok(rel) => rel,
                    Err(_) => continue,
                };

                // Check if any pattern matches
                let path_str = relative_path.to_string_lossy();
                for rule in rules {
                    if rule.compiled.matches(&path_str) {
                        matching_files.push(path.to_path_buf());
                        debug!(
                            event = "core.files.pattern.matched",
                            pattern = rule.pattern,
                            file = %path.display()
                        );
                        break; // Don't need to check other patterns for this file
                    }
                }
            }
            Err(e) => {
                warn!(
                    event = "core.files.walk.error",
                    error = %e,
                    message = "Error walking directory, skipping entry"
                );
            }
        }
    }

    debug!(
        event = "core.files.matching.completed",
        source_root = %source_root.display(),
        matched_count = matching_files.len()
    );

    Ok(matching_files)
}

/// Copy a single file safely with atomic operations.
///
/// Uses a temporary file in the same directory as the destination,
/// then atomically renames it to prevent partial writes or race conditions.
///
/// # Errors
/// Returns an error if:
/// - Source file doesn't exist
/// - File exceeds max_file_size limit
/// - I/O operations fail
pub fn copy_file_safely(
    source: &Path,
    destination: &Path,
    options: &CopyOptions,
) -> Result<(), FileError> {
    // Check if source file exists
    if !source.exists() {
        return Err(FileError::FileNotFound {
            path: source.display().to_string(),
        });
    }

    // Check file size if limit is set
    if let Some(max_size) = options.max_file_size {
        let metadata = fs::metadata(source).map_err(|e| FileError::IoError { source: e })?;
        if metadata.len() > max_size {
            return Err(FileError::FileTooLarge {
                path: source.display().to_string(),
                size: metadata.len(),
            });
        }
    }

    // Create destination directory if it doesn't exist
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| FileError::IoError { source: e })?;
    }

    // Create temp file in same directory as destination for atomic rename
    let temp_file = NamedTempFile::new_in(destination.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| FileError::IoError { source: e })?;

    // Copy contents to temp file
    fs::copy(source, temp_file.path()).map_err(|e| FileError::IoError { source: e })?;

    // Atomically persist temp file to destination
    temp_file
        .persist(destination)
        .map_err(|e| FileError::IoError { source: e.error })?;

    debug!(
        event = "core.files.copy.completed",
        source = %source.display(),
        destination = %destination.display()
    );

    Ok(())
}

/// Parse max file size string (e.g., "10MB", "1GB") to bytes
pub fn parse_file_size(size_str: &str) -> Result<u64, FileError> {
    let size_str = size_str.trim().to_uppercase();

    let (number_part, unit_part) = if size_str.ends_with("KB") {
        (&size_str[..size_str.len() - 2], 1024u64)
    } else if size_str.ends_with("MB") {
        (&size_str[..size_str.len() - 2], 1024u64 * 1024)
    } else if size_str.ends_with("GB") {
        (&size_str[..size_str.len() - 2], 1024u64 * 1024 * 1024)
    } else if size_str.ends_with('B') {
        (&size_str[..size_str.len() - 1], 1u64)
    } else {
        (size_str.as_str(), 1u64) // Assume bytes if no unit
    };

    let number: u64 = number_part
        .parse()
        .map_err(|_| FileError::ValidationError {
            message: format!("Invalid file size format: '{}'", size_str),
        })?;

    Ok(number * unit_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_patterns_success() {
        let config = IncludeConfig {
            patterns: vec![".env*".to_string(), "*.local.json".to_string()],
            enabled: true,
            max_file_size: None,
        };

        let result = validate_patterns(&config);
        assert!(result.is_ok());

        let rules = result.unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, ".env*");
        assert_eq!(rules[1].pattern, "*.local.json");
    }

    #[test]
    fn test_validate_patterns_invalid() {
        let config = IncludeConfig {
            patterns: vec!["[invalid".to_string()], // Invalid glob pattern
            enabled: true,
            max_file_size: None,
        };

        let result = validate_patterns(&config);
        assert!(result.is_err());

        if let Err(FileError::InvalidPattern { pattern, .. }) = result {
            assert_eq!(pattern, "[invalid");
        } else {
            panic!("Expected InvalidPattern error");
        }
    }

    #[test]
    fn test_parse_file_size() {
        assert_eq!(parse_file_size("1024").unwrap(), 1024);
        assert_eq!(parse_file_size("1KB").unwrap(), 1024);
        assert_eq!(parse_file_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_file_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_file_size("10MB").unwrap(), 10 * 1024 * 1024);

        assert!(parse_file_size("invalid").is_err());
        assert!(parse_file_size("1XB").is_err());
    }
}
