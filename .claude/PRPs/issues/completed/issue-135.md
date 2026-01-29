# Investigation: peek: Screenshot save fails if output directory doesn't exist

**Issue**: #135 (https://github.com/Wirasm/kild/issues/135)
**Type**: BUG
**Investigated**: 2026-01-29T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                              |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Users can work around by pre-creating directories, but the error message is confusing and hides the root cause         |
| Complexity | LOW    | Single function change in one file, isolated to `save_to_file`, no integration points affected                         |
| Confidence | HIGH   | Root cause is clear: `std::fs::write()` doesn't create parent directories, and the codebase has an established pattern |

---

## Problem Statement

When saving a screenshot to a path where the parent directory doesn't exist, `kild-peek` fails with a generic OS error (`"No such file or directory"`) instead of either creating the directories automatically or providing a clear error message indicating which directory is missing.

---

## Analysis

### Root Cause

The `save_to_file` function uses `std::fs::write()` directly without ensuring the parent directory exists first.

### Evidence Chain

WHY: User gets `IoError { source: Os { code: 2, kind: NotFound, message: "No such file or directory" } }`
↓ BECAUSE: `std::fs::write()` fails when parent directory doesn't exist
Evidence: `crates/kild-peek-core/src/screenshot/handler.rs:28` - `std::fs::write(path, result.data())?;`

↓ BECAUSE: There's no directory creation before writing
Evidence: `crates/kild-peek-core/src/screenshot/handler.rs:25-32` - function has no `create_dir_all` call

↓ ROOT CAUSE: Missing parent directory check/creation before file write
Evidence: The codebase already has this pattern in `kild-core` but it wasn't applied to `kild-peek-core`

### Established Pattern

`kild-core` handles this correctly in `sessions/persistence.rs:9-12`:

```rust
pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}
```

### Affected Files

| File                                               | Lines | Action | Description                                       |
| -------------------------------------------------- | ----- | ------ | ------------------------------------------------- |
| `crates/kild-peek-core/src/screenshot/handler.rs`  | 25-32 | UPDATE | Add parent directory creation before write        |
| `crates/kild-peek-core/src/screenshot/errors.rs`   | 31-35 | UPDATE | Add specific error variant for directory creation |
| `crates/kild-peek-core/src/screenshot/handler.rs`  | tests | UPDATE | Add test for missing parent directory case        |

### Integration Points

- `crates/kild-peek/src/commands.rs:151` - calls `save_to_file` for screenshot command
- `crates/kild-peek/src/commands.rs:260` - calls `save_to_file` for assert similar (uses `/tmp`, always exists)

### Git History

- **Introduced**: `0514fc4` - 2026-01-28 - "Add kild-peek CLI for native application inspection (#122)"
- **Implication**: Original implementation, not a regression

---

## Implementation Plan

### Step 1: Add specific error variant for directory creation failure

**File**: `crates/kild-peek-core/src/screenshot/errors.rs`
**Lines**: 31-35
**Action**: UPDATE

**Current code:**

```rust
    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
```

**Required change:**

Add a new error variant before `IoError` for clearer messaging when directory creation fails:

```rust
    #[error("Failed to create output directory '{path}': {source}")]
    DirectoryCreationFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
```

**Why**: Provides actionable error message showing exactly which directory couldn't be created.

---

### Step 2: Update error_code() match for new variant

**File**: `crates/kild-peek-core/src/screenshot/errors.rs`
**Lines**: 39-51 (error_code function)
**Action**: UPDATE

**Current code:**

```rust
    fn error_code(&self) -> &'static str {
        match self {
            ScreenshotError::WindowNotFound { .. } => "SCREENSHOT_WINDOW_NOT_FOUND",
            // ... other variants ...
            ScreenshotError::IoError { .. } => "SCREENSHOT_IO_ERROR",
        }
    }
```

**Required change:**

Add match arm for the new variant:

```rust
            ScreenshotError::DirectoryCreationFailed { .. } => "SCREENSHOT_DIRECTORY_CREATION_FAILED",
```

**Why**: Maintains consistent error code pattern.

---

### Step 3: Update is_user_error() match for new variant

**File**: `crates/kild-peek-core/src/screenshot/errors.rs`
**Lines**: 53-62 (is_user_error function)
**Action**: UPDATE

**Current code:**

```rust
    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ScreenshotError::WindowNotFound { .. }
                | ScreenshotError::WindowNotFoundById { .. }
                | ScreenshotError::WindowMinimized { .. }
                | ScreenshotError::PermissionDenied
                | ScreenshotError::MonitorNotFound { .. }
        )
    }
```

**Required change:**

Add `DirectoryCreationFailed` to user errors (users can fix by providing a valid path):

```rust
    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ScreenshotError::WindowNotFound { .. }
                | ScreenshotError::WindowNotFoundById { .. }
                | ScreenshotError::WindowMinimized { .. }
                | ScreenshotError::PermissionDenied
                | ScreenshotError::MonitorNotFound { .. }
                | ScreenshotError::DirectoryCreationFailed { .. }
        )
    }
```

**Why**: Directory creation failure is user-fixable (provide valid path or permissions).

---

### Step 4: Update save_to_file to create parent directories

**File**: `crates/kild-peek-core/src/screenshot/handler.rs`
**Lines**: 25-32
**Action**: UPDATE

**Current code:**

```rust
/// Save a capture result to a file
pub fn save_to_file(result: &CaptureResult, path: &Path) -> Result<(), ScreenshotError> {
    info!(event = "core.screenshot.save_started", path = %path.display());

    std::fs::write(path, result.data())?;

    info!(event = "core.screenshot.save_completed", path = %path.display());
    Ok(())
}
```

**Required change:**

```rust
/// Save a capture result to a file
///
/// Creates parent directories if they don't exist.
pub fn save_to_file(result: &CaptureResult, path: &Path) -> Result<(), ScreenshotError> {
    info!(event = "core.screenshot.save_started", path = %path.display());

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            debug!(event = "core.screenshot.creating_parent_directory", path = %parent.display());
            std::fs::create_dir_all(parent).map_err(|source| {
                ScreenshotError::DirectoryCreationFailed {
                    path: parent.display().to_string(),
                    source,
                }
            })?;
        }
    }

    std::fs::write(path, result.data())?;

    info!(event = "core.screenshot.save_completed", path = %path.display());
    Ok(())
}
```

**Why**: Follows established pattern from `kild-core`, creates directories like `mkdir -p`, and provides clear error on failure.

---

### Step 5: Add import for debug macro

**File**: `crates/kild-peek-core/src/screenshot/handler.rs`
**Lines**: 7
**Action**: UPDATE

**Current code:**

```rust
use tracing::{debug, info, warn};
```

**Required change:**

No change needed - `debug` is already imported.

---

### Step 6: Add tests for directory creation

**File**: `crates/kild-peek-core/src/screenshot/handler.rs`
**Lines**: After line 286 (end of tests module)
**Action**: UPDATE

**Test cases to add:**

```rust
    #[test]
    fn test_save_to_file_creates_parent_directories() {
        use std::env;
        use super::*;

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
        use super::*;

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
        let error = ScreenshotError::DirectoryCreationFailed {
            path: "/some/path".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        };
        assert_eq!(error.error_code(), "SCREENSHOT_DIRECTORY_CREATION_FAILED");
        assert!(error.is_user_error());
        assert!(error.to_string().contains("/some/path"));
    }

    /// Helper to create a minimal valid PNG for testing
    fn create_test_png() -> Vec<u8> {
        use image::codecs::png::PngEncoder;
        use image::ImageEncoder;
        use std::io::Cursor;

        let img = image::RgbaImage::new(1, 1);
        let mut buffer = Cursor::new(Vec::new());
        let encoder = PngEncoder::new(&mut buffer);
        encoder
            .write_image(&img, 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        buffer.into_inner()
    }
```

---

## Patterns to Follow

**From codebase - directory creation pattern:**

```rust
// SOURCE: crates/kild-core/src/sessions/persistence.rs:9-12
// Pattern for ensuring directory exists before writing
pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}
```

**From codebase - error variant pattern:**

```rust
// SOURCE: crates/kild-peek-core/src/screenshot/errors.rs:14-17
// Pattern for errors with context
#[error(
    "Screen recording permission denied. Enable in System Settings > Privacy & Security > Screen Recording"
)]
PermissionDenied,
```

---

## Edge Cases & Risks

| Risk/Edge Case                          | Mitigation                                                      |
| --------------------------------------- | --------------------------------------------------------------- |
| Path is just a filename (no parent)     | Check `parent.as_os_str().is_empty()` before creating           |
| Parent directory exists                 | Check `!parent.exists()` to avoid unnecessary syscall           |
| Permission denied on directory creation | Wrap in `DirectoryCreationFailed` with clear message            |
| Disk full                               | Falls through to `IoError` on write (acceptable)                |
| Path contains invalid characters        | OS will return appropriate error, wrapped in our error type     |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test -p kild-peek-core
cargo build --all
```

### Manual Verification

1. Run `kild-peek screenshot --window "Finder" -o /tmp/nonexistent/path/test.png`
   - Should succeed and create `/tmp/nonexistent/path/` directory
2. Verify screenshot file exists at the path
3. Run with permission-denied path (e.g., `/root/test.png` on macOS)
   - Should get clear error: `"Failed to create output directory '/root': permission denied"`

---

## Scope Boundaries

**IN SCOPE:**

- Adding parent directory creation to `save_to_file`
- Adding `DirectoryCreationFailed` error variant
- Tests for the new behavior

**OUT OF SCOPE (do not touch):**

- CLI argument parsing (already handles paths correctly)
- Other screenshot functionality (capture, encode, etc.)
- The assert command's temp file handling (uses `/tmp`, always exists)
- Any changes to kild-core or kild-ui

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-29T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-135.md`
