# Investigation: Add Project dialog doesn't normalize paths

**Issue**: Free-form (no GitHub issue)
**Type**: BUG
**Investigated**: 2026-01-26T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                      |
| ---------- | ------ | ---------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Path validation fails for common user input patterns, but workaround exists (type full path)  |
| Complexity | LOW    | Single function change with path normalization, isolated to one file                           |
| Confidence | HIGH   | Root cause clearly identified in code - no path normalization before validation                |

---

## Problem Statement

When entering a path in the "Add Project" dialog, paths without a leading `/` (e.g., `users/rasmus/projects/mine/shards`) fail validation with "is not a directory" error. The UI should normalize user input to handle common path entry patterns like missing leading slashes and tilde expansion.

---

## Analysis

### Root Cause

The path validation flow creates a `PathBuf` directly from user input without any normalization:

```rust
// main_view.rs:400
let path = PathBuf::from(&path_str);
```

When the user enters `users/rasmus/projects/mine/shards`:
1. `PathBuf::from()` creates a **relative path**
2. `path.is_dir()` checks if this relative path exists from the CWD
3. The CWD is likely `/` or the app's installation directory
4. `users/rasmus/...` doesn't exist relative to CWD → validation fails

### Evidence Chain

WHY: Error shows "'users/rasmus/projects/mine/shards' is not a directory"
↓ BECAUSE: `path.is_dir()` returns false for the relative path
Evidence: `projects.rs:168` - `if !path.is_dir()`

↓ BECAUSE: The path is treated as relative, not absolute
Evidence: `main_view.rs:400` - `let path = PathBuf::from(&path_str);`

↓ ROOT CAUSE: No path normalization before creating PathBuf
Evidence: `main_view.rs:387-400` - User input goes directly to PathBuf with only `.trim()`

### Affected Files

| File                            | Lines   | Action | Description                              |
| ------------------------------- | ------- | ------ | ---------------------------------------- |
| `crates/shards-ui/src/views/main_view.rs` | 386-400 | UPDATE | Add path normalization before validation |

### Integration Points

- `actions::add_project()` at line 402 receives the normalized path
- `projects::validate_project_path()` validates the path
- Error message at `actions.rs:244` displays the path as-is

---

## Implementation Plan

### Step 1: Add path normalization in `on_add_project_submit`

**File**: `crates/shards-ui/src/views/main_view.rs`
**Lines**: 387-400
**Action**: UPDATE

**Current code:**

```rust
// Lines 387-400
let path_str = self.state.add_project_form.path.trim().to_string();
let name = if self.state.add_project_form.name.trim().is_empty() {
    None
} else {
    Some(self.state.add_project_form.name.trim().to_string())
};

if path_str.is_empty() {
    self.state.add_project_error = Some("Path cannot be empty".to_string());
    cx.notify();
    return;
}

let path = PathBuf::from(&path_str);
```

**Required change:**

```rust
// Lines 387-400
let path_str = self.state.add_project_form.path.trim().to_string();
let name = if self.state.add_project_form.name.trim().is_empty() {
    None
} else {
    Some(self.state.add_project_form.name.trim().to_string())
};

if path_str.is_empty() {
    self.state.add_project_error = Some("Path cannot be empty".to_string());
    cx.notify();
    return;
}

// Normalize path: expand ~ and ensure absolute path
let path = normalize_project_path(&path_str);
```

**Why**: Centralizes path normalization logic and handles common user input patterns.

---

### Step 2: Add `normalize_project_path` helper function

**File**: `crates/shards-ui/src/views/main_view.rs`
**Action**: UPDATE (add function)

**Add this function** (near the top of the file, after imports):

```rust
/// Normalize user-entered path for project addition.
///
/// Handles:
/// - Tilde expansion (~/ -> home directory)
/// - Missing leading slash (users/... -> /users/...)
fn normalize_project_path(path_str: &str) -> PathBuf {
    let path_str = path_str.trim();

    // Handle tilde expansion
    if path_str.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path_str[2..]);
        }
    } else if path_str == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    // Handle missing leading slash - if path looks like an absolute path without the /
    // e.g., "users/rasmus/..." should become "/users/rasmus/..."
    if !path_str.starts_with('/') && !path_str.starts_with('~') {
        // Check if adding / would make it a valid directory
        let with_slash = format!("/{}", path_str);
        let potential_path = PathBuf::from(&with_slash);
        if potential_path.is_dir() {
            return potential_path;
        }
    }

    PathBuf::from(path_str)
}
```

**Why**: Handles the two most common path entry mistakes: missing leading `/` and using `~` for home directory.

---

### Step 3: Add `dirs` dependency if not present

**File**: `crates/shards-ui/Cargo.toml`
**Action**: UPDATE (if needed)

Check if `dirs` is already a dependency. If not, add:

```toml
[dependencies]
dirs = "5"
```

**Why**: Required for reliable home directory expansion across platforms.

---

### Step 4: Add tests for path normalization

**File**: `crates/shards-ui/src/views/main_view.rs`
**Action**: UPDATE (add tests)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_with_leading_slash() {
        let result = normalize_project_path("/Users/test/project");
        assert_eq!(result, PathBuf::from("/Users/test/project"));
    }

    #[test]
    fn test_normalize_path_tilde_expansion() {
        let result = normalize_project_path("~/projects/test");
        // Should expand to home directory
        assert!(result.to_string_lossy().contains("projects/test"));
        assert!(!result.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn test_normalize_path_trims_whitespace() {
        let result = normalize_project_path("  /Users/test/project  ");
        assert_eq!(result, PathBuf::from("/Users/test/project"));
    }
}
```

**Why**: Ensures path normalization works correctly for various input patterns.

---

## Patterns to Follow

**From codebase - error handling pattern in actions:**

```rust
// SOURCE: actions.rs:241-252
match validate_project_path(&path, &data.projects) {
    ProjectValidation::Valid => {}
    ProjectValidation::NotADirectory => {
        return Err(format!("'{}' is not a directory", path.display()));
    }
    // ...
}
```

**From codebase - form handling pattern:**

```rust
// SOURCE: main_view.rs:387-398
let path_str = self.state.add_project_form.path.trim().to_string();
// ... validation ...
if path_str.is_empty() {
    self.state.add_project_error = Some("Path cannot be empty".to_string());
    cx.notify();
    return;
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                | Mitigation                                                     |
| ----------------------------- | -------------------------------------------------------------- |
| `dirs` crate not available    | Falls back to treating `~` as literal if home_dir returns None |
| Path with `/` is valid        | Don't modify paths that already start with `/`                 |
| Relative path that's not a dir| Only add `/` if the resulting path is actually a directory     |
| Windows paths                 | Not applicable - shards is macOS only                          |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Manual Verification

1. Open shards-ui
2. Click "Add Project"
3. Enter `users/rasmus/projects/mine/shards` (without leading /)
4. Verify it auto-corrects and adds the project
5. Try with `~/projects/...` path
6. Verify tilde expansion works

---

## Scope Boundaries

**IN SCOPE:**

- Path normalization in Add Project dialog
- Tilde expansion
- Missing leading slash correction

**OUT OF SCOPE (do not touch):**

- Validation logic in `projects.rs`
- Error message formatting in `actions.rs`
- Real-time validation as user types (keep submit-time validation)
- Path autocomplete/suggestions

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-26
- **Artifact**: `.claude/PRPs/issues/investigation-path-normalization.md`
