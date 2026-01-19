use crate::files::{
    errors::FileError,
    operations,
    types::{CopyOptions, IncludeConfig},
};
use std::path::Path;
use tracing::{error, info, warn};

/// Copy files matching include patterns from source to destination.
///
/// Returns `Ok((copied_count, failed_count))` where:
/// - `copied_count`: Number of files successfully copied
/// - `failed_count`: Number of files that failed to copy
///
/// Individual file failures are logged but don't stop the operation.
/// Returns `Err` only for fatal errors like pattern validation failure.
pub fn copy_include_files(
    source_root: &Path,
    destination_root: &Path,
    config: &IncludeConfig,
) -> Result<(usize, usize), FileError> {
    info!(
        event = "files.copy.started",
        source_root = %source_root.display(),
        destination_root = %destination_root.display(),
        pattern_count = config.patterns.len(),
        enabled = config.enabled
    );

    // Early return if not enabled
    if !config.enabled {
        info!(
            event = "files.copy.skipped",
            reason = "include_patterns disabled in config"
        );
        return Ok((0, 0));
    }

    // Early return if no patterns
    if config.patterns.is_empty() {
        info!(
            event = "files.copy.skipped",
            reason = "no patterns configured"
        );
        return Ok((0, 0));
    }

    // Validate patterns
    let rules = match operations::validate_patterns(config) {
        Ok(rules) => rules,
        Err(e) => {
            error!(
                event = "files.copy.failed",
                error = %e,
                error_type = "pattern_validation",
                patterns = ?config.patterns
            );
            return Err(e);
        }
    };

    // Find matching files
    let matching_files = match operations::find_matching_files(source_root, &rules) {
        Ok(files) => files,
        Err(e) => {
            error!(
                event = "files.copy.failed",
                error = %e,
                error_type = "file_discovery",
                source_root = %source_root.display()
            );
            return Err(e);
        }
    };

    if matching_files.is_empty() {
        info!(
            event = "files.copy.completed",
            files_copied = 0,
            reason = "no matching files found"
        );
        return Ok((0, 0));
    }

    // Parse max file size if configured - fail fast on invalid format
    let max_file_size = if let Some(size_str) = &config.max_file_size {
        Some(operations::parse_file_size(size_str).map_err(|e| {
            error!(
                event = "files.copy.failed",
                error = %e,
                error_type = "invalid_max_file_size",
                max_file_size = size_str
            );
            e
        })?)
    } else {
        None
    };

    // Create copy options
    let copy_options = CopyOptions { max_file_size };

    // Copy each matching file
    let mut copied_count = 0;
    let mut error_count = 0;

    for source_file in &matching_files {
        // Calculate relative path and destination
        let relative_path = match source_file.strip_prefix(source_root) {
            Ok(rel) => rel,
            Err(_) => {
                warn!(
                    event = "files.copy.warning",
                    warning_type = "path_calculation",
                    source_file = %source_file.display(),
                    message = "Could not calculate relative path, skipping"
                );
                error_count += 1;
                continue;
            }
        };

        let destination_file = destination_root.join(relative_path);

        // Copy the file
        match operations::copy_file_safely(source_file, &destination_file, &copy_options) {
            Ok(()) => {
                copied_count += 1;
                info!(
                    event = "files.copy.file_completed",
                    source = %source_file.display(),
                    destination = %destination_file.display(),
                    relative_path = %relative_path.display()
                );
            }
            Err(e) => {
                error_count += 1;
                warn!(
                    event = "files.copy.file_failed",
                    error = %e,
                    source = %source_file.display(),
                    destination = %destination_file.display(),
                    relative_path = %relative_path.display(),
                    message = "Failed to copy file, continuing with others"
                );
            }
        }
    }

    if error_count > 0 {
        warn!(
            event = "files.copy.completed_with_errors",
            files_copied = copied_count,
            files_failed = error_count,
            total_files = matching_files.len()
        );
    } else {
        info!(
            event = "files.copy.completed",
            files_copied = copied_count,
            total_files = matching_files.len()
        );
    }

    Ok((copied_count, error_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::types::IncludeConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_copy_include_files_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&dest).unwrap();

        let config = IncludeConfig {
            patterns: vec![".env*".to_string()],
            enabled: false,
            max_file_size: None,
        };

        let result = copy_include_files(&source, &dest, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (0, 0));
    }

    #[test]
    fn test_copy_include_files_no_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&dest).unwrap();

        let config = IncludeConfig {
            patterns: vec![],
            enabled: true,
            max_file_size: None,
        };

        let result = copy_include_files(&source, &dest, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (0, 0));
    }

    #[test]
    fn test_copy_include_files_invalid_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&dest).unwrap();

        let config = IncludeConfig {
            patterns: vec!["[invalid".to_string()],
            enabled: true,
            max_file_size: None,
        };

        let result = copy_include_files(&source, &dest, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_include_files_overrides_gitignore() {
        use std::process::Command;

        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source");
        let dest = temp_dir.path().join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&dest).unwrap();

        // Initialize git repo (required for ignore crate)
        Command::new("git")
            .args(&["init"])
            .current_dir(&source)
            .output()
            .unwrap();

        // Create .gitignore that ignores .env files
        fs::write(source.join(".gitignore"), ".env*\n").unwrap();

        // Create .env file that should be ignored by git
        fs::write(source.join(".env"), "SECRET=value\n").unwrap();

        // Create .env.local file
        fs::write(source.join(".env.local"), "LOCAL=value\n").unwrap();

        let config = IncludeConfig {
            patterns: vec![".env*".to_string()],
            enabled: true,
            max_file_size: None,
        };

        let result = copy_include_files(&source, &dest, &config);
        assert!(result.is_ok());
        let (copied, failed) = result.unwrap();
        assert_eq!(copied, 2);
        assert_eq!(failed, 0);
        assert!(dest.join(".env").exists());
        assert!(dest.join(".env.local").exists());
    }
}
