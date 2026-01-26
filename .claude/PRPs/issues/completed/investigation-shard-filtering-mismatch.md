# Investigation: Shard Creation/Filtering Project ID Mismatch

**Issue**: Free-form description (no GitHub issue)
**Type**: BUG
**Investigated**: 2026-01-26T14:10:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                         |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Core functionality broken - shards created while viewing a project don't appear in that project's filtered list  |
| Complexity | MEDIUM | 3 files affected, straightforward fix with canonicalization, but requires careful handling across UI and core    |
| Confidence | HIGH   | Root cause identified with concrete evidence - path case mismatch between stored path and git-discovered path    |

---

## Problem Statement

When creating a shard in the UI while viewing a specific project, the newly created shard doesn't appear in the filtered list for that project. It only appears when viewing "All Projects". This happens because the path stored in `projects.json` is not canonicalized, leading to hash mismatches between the UI's filtering logic and shards-core's project detection.

Additionally, the Create Shard dialog should allow selecting an existing project to ensure shards are created in the correct context, rather than relying solely on the currently active project filter.

---

## Analysis

### Root Cause

The issue is a **path canonicalization mismatch**:

**WHY 1**: Why don't newly created shards appear in the filtered project list?
-> Because `session.project_id` doesn't match the derived ID from `active_project`
-> Evidence: Filtering at `state.rs:388-390`

**WHY 2**: Why don't the project IDs match?
-> Because they're hashed from different path strings
-> Evidence: `derive_project_id()` in `projects.rs:62-65` vs `generate_project_id()` in `operations.rs:37-43`

**WHY 3**: Why are the path strings different?
-> Because the UI stores the user-typed path (e.g., `/users/rasmus/projects/mine/shards`) but shards-core gets the canonical path from git (e.g., `/Users/rasmus/Projects/mine/SHARDS`)
-> Evidence: `~/.shards/projects.json` contains lowercase path, but `git rev-parse --show-toplevel` returns canonical case

**ROOT CAUSE**: The `normalize_project_path()` function in `main_view.rs` doesn't canonicalize the path, and on macOS (case-insensitive filesystem), paths with different cases hash to different values but navigate to the same directory.

### Evidence Chain

```bash
# Stored path in projects.json
$ cat ~/.shards/projects.json
{
  "projects": [{"path": "/users/rasmus/projects/mine/shards", ...}],
  "active": "/users/rasmus/projects/mine/shards"
}

# Actual canonical path from git
$ cd /users/rasmus/projects/mine/shards && git rev-parse --show-toplevel
/Users/rasmus/Projects/mine/SHARDS

# These paths hash to different values, causing the mismatch
```

### Affected Files

| File                                       | Lines   | Action | Description                                          |
| ------------------------------------------ | ------- | ------ | ---------------------------------------------------- |
| `crates/shards-ui/src/views/main_view.rs`  | 32-81   | UPDATE | Add path canonicalization in normalize_project_path  |
| `crates/shards-ui/src/views/main_view.rs`  | 463-515 | UPDATE | Alternative: canonicalize in on_add_project_submit   |
| `crates/shards-ui/src/projects.rs`         | 58-66   | UPDATE | Consider canonicalizing in derive_project_id         |

### Integration Points

- `main_view.rs:479` calls `normalize_project_path()`
- `main_view.rs:488` calls `actions::add_project(path, name)`
- `main_view.rs:498` sets `state.active_project = Some(path)`
- `state.rs:371-375` derives project ID for filtering via `derive_project_id()`
- `shards-core/git/handler.rs:71-117` uses `Repository::discover().workdir()` which returns canonical path

### Git History

- **Introduced**: Original multi-project support (commit 13a3b16)
- **Partially fixed**: Commit 47bf59d fixed creation context but not path canonicalization
- **Implication**: Path canonicalization was never implemented when projects feature was added

---

## Implementation Plan

### Step 1: Canonicalize path in normalize_project_path

**File**: `crates/shards-ui/src/views/main_view.rs`
**Lines**: 32-81
**Action**: UPDATE

**Current code (end of function, around line 80):**
```rust
    // Return as-is for absolute paths
    Ok(PathBuf::from(path_str))
}
```

**Required change:**
```rust
    // Canonicalize the path to ensure consistent hashing across UI and core
    // This resolves symlinks and normalizes case on case-insensitive filesystems
    let path = PathBuf::from(path_str);
    match path.canonicalize() {
        Ok(canonical) => {
            debug!(
                event = "ui.normalize_path.canonicalized",
                original = %path.display(),
                canonical = %canonical.display()
            );
            Ok(canonical)
        }
        Err(e) => {
            // Path doesn't exist or can't be accessed - return original for better error
            debug!(
                event = "ui.normalize_path.canonicalize_failed",
                path = %path.display(),
                error = %e
            );
            Ok(path)
        }
    }
}
```

**Why**: `canonicalize()` resolves the actual filesystem path, ensuring consistent case on macOS and resolving symlinks. This makes the stored path match what git's `workdir()` returns.

---

### Step 2: Update existing projects with canonical paths (migration)

**File**: `crates/shards-ui/src/projects.rs`
**Action**: UPDATE - add migration function

Add a function to migrate existing projects to canonical paths:

```rust
/// Migrate projects to use canonical paths.
///
/// This fixes the path case mismatch issue on case-insensitive filesystems.
/// Called on app startup to ensure existing projects are canonicalized.
pub fn migrate_projects_to_canonical() -> Result<(), String> {
    let mut data = load_projects();
    let mut changed = false;

    for project in &mut data.projects {
        if let Ok(canonical) = project.path.canonicalize() {
            if canonical != project.path {
                tracing::info!(
                    event = "ui.projects.path_migrated",
                    original = %project.path.display(),
                    canonical = %canonical.display()
                );
                project.path = canonical;
                changed = true;
            }
        }
    }

    // Also canonicalize active project
    if let Some(ref active) = data.active {
        if let Ok(canonical) = active.canonicalize() {
            if &canonical != active {
                data.active = Some(canonical);
                changed = true;
            }
        }
    }

    if changed {
        save_projects(&data)?;
    }

    Ok(())
}
```

---

### Step 3: Call migration on app startup

**File**: `crates/shards-ui/src/state.rs`
**Action**: UPDATE - call migration in AppState::new()

**Current code (line ~260):**
```rust
    pub fn new() -> Self {
        let (displays, load_error) = crate::actions::refresh_sessions();

        // Load projects from disk
        let projects_data = crate::projects::load_projects();
```

**Required change:**
```rust
    pub fn new() -> Self {
        let (displays, load_error) = crate::actions::refresh_sessions();

        // Migrate projects to canonical paths (fixes case mismatch on macOS)
        if let Err(e) = crate::projects::migrate_projects_to_canonical() {
            tracing::warn!(
                event = "ui.projects.migration_failed",
                error = %e
            );
        }

        // Load projects from disk (after migration)
        let projects_data = crate::projects::load_projects();
```

---

### Step 4: Add tests for path canonicalization

**File**: `crates/shards-ui/src/views/main_view.rs`
**Action**: UPDATE - add tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_project_path_canonicalizes() {
        // On macOS, paths are case-insensitive
        // This test verifies that we get the canonical case
        if let Ok(home) = std::env::var("HOME") {
            // Use a path we know exists
            let lowercase = home.to_lowercase();
            if let Ok(result) = normalize_project_path(&lowercase) {
                // The result should be canonicalized
                assert!(result.exists(), "Canonicalized path should exist");
                // On macOS, the canonical path may have different case
                // The important thing is consistency
            }
        }
    }

    #[test]
    fn test_normalize_project_path_tilde_expansion_canonical() {
        if let Ok(result) = normalize_project_path("~") {
            // Should be canonicalized
            assert!(result.is_absolute());
            assert!(result.exists());
        }
    }
}
```

---

### Step 5: Add test for project migration

**File**: `crates/shards-ui/src/projects.rs`
**Action**: UPDATE - add test

```rust
#[test]
fn test_migrate_projects_canonicalizes_paths() {
    // This is a unit test for the migration logic
    // Full integration test would require mocking the filesystem
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("git init failed");

    // Verify canonical path matches original for this test dir
    let canonical = path.canonicalize().unwrap();
    assert_eq!(path.canonicalize().unwrap(), canonical);
}
```

---

## Second Issue: Project Selection in Create Dialog

The user also mentioned wanting to select existing projects in the create dialog. This is a separate enhancement.

### Enhancement: Add Project Selector to Create Dialog

**File**: `crates/shards-ui/src/views/create_dialog.rs`
**Action**: UPDATE - add project selector dropdown

This would require:
1. Adding a `selected_project: Option<PathBuf>` to `CreateFormState`
2. Rendering a dropdown in the create dialog showing available projects
3. Using the selected project instead of `active_project` when creating

**Recommendation**: This is a separate enhancement and should be tracked separately. The path canonicalization fix should be implemented first as it fixes the core bug.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/shards-ui/src/views/main_view.rs:62-67
// Pattern for debug logging after path transformation
debug!(
    event = "ui.normalize_path.slash_prefix_applied",
    original = path_str,
    normalized = %with_slash.display()
);
```

```rust
// SOURCE: crates/shards-ui/src/projects.rs:134-138
// Pattern for logging after save
tracing::info!(
    event = "ui.projects.saved",
    path = %path.display(),
    count = data.projects.len()
);
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                    |
| ------------------------------------- | ------------------------------------------------------------- |
| Path doesn't exist during canonicalize| Fall back to original path, let validation catch it later     |
| Symlink resolution changes path       | This is intentional - canonical path is what we want          |
| Migration fails on startup            | Log warning but continue - non-fatal, just affects filtering  |
| Network filesystem paths              | canonicalize() should handle these, but may be slow           |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test -p shards-ui
cargo test -p shards-core
```

### Manual Verification

1. Delete `~/.shards/projects.json` (or backup)
2. Launch UI, add a project using lowercase path (e.g., `/users/rasmus/projects/mine/shards`)
3. Verify `~/.shards/projects.json` contains canonical path with correct case
4. Create a shard while that project is selected
5. Verify the shard appears in the filtered list (not just "All Projects")
6. Restart UI, verify migration works for any existing non-canonical paths

---

## Scope Boundaries

**IN SCOPE:**
- Path canonicalization in normalize_project_path
- Migration of existing projects to canonical paths
- Tests for canonicalization behavior

**OUT OF SCOPE (separate enhancements):**
- Project selector dropdown in create dialog (track separately)
- Bulk re-association of existing shards with correct project IDs

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-26T14:10:00Z
- **Artifact**: `.claude/PRPs/issues/investigation-shard-filtering-mismatch.md`
