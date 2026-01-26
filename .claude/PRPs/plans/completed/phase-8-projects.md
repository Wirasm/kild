# Implementation Plan: Phase 8 - Projects

**Source PRD**: `.claude/PRPs/prds/gpui-native-terminal-ui.prd.md`
**Phase**: 8 - Projects
**Date**: 2026-01-26

---

## Summary

Add project management to shards-ui. Users can add/remove projects (git repository paths), switch between them, and the shard list filters to show only shards for the active project. Projects are auto-tracked when creating shards in new repositories.

---

## Key Insight: Sessions Already Have `project_id`

The `Session` struct in shards-core already has a `project_id` field that uniquely identifies which repository a shard belongs to. This makes filtering straightforward - we just need to match `session.project_id` against the active project's ID.

---

## Patterns to Mirror

| Pattern | Reference File |
|---------|---------------|
| Dialog component | `crates/shards-ui/src/views/create_dialog.rs` |
| State management | `crates/shards-ui/src/state.rs` |
| Action handlers | `crates/shards-ui/src/actions.rs` |
| Main view integration | `crates/shards-ui/src/views/main_view.rs` |
| JSON file persistence | Follow standard serde patterns |

---

## Files to Change

### CREATE

| File | Purpose |
|------|---------|
| `crates/shards-ui/src/projects.rs` | Project data types, load/save, git validation |
| `crates/shards-ui/src/views/project_selector.rs` | Dropdown component for switching projects |
| `crates/shards-ui/src/views/add_project_dialog.rs` | Dialog for adding new projects |

### UPDATE

| File | Change |
|------|--------|
| `crates/shards-ui/src/state.rs` | Add project list, active project, project form state |
| `crates/shards-ui/src/actions.rs` | Add project CRUD actions, filter sessions by project |
| `crates/shards-ui/src/views/main_view.rs` | Add project selector to header, integrate dialogs |
| `crates/shards-ui/src/views/mod.rs` | Export new view modules |
| `crates/shards-ui/src/main.rs` (or lib.rs) | Export projects module |

---

## Step-by-Step Tasks

### Task 1: CREATE `crates/shards-ui/src/projects.rs`

Define project data types and persistence.

```rust
//! Project management for shards-ui.
//!
//! Handles storing, loading, and validating projects (git repositories).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A project is a git repository where shards can be created.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    /// File system path to the repository root
    pub path: PathBuf,
    /// Display name (defaults to directory name if not set)
    pub name: String,
}

/// Stored projects data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsData {
    pub projects: Vec<Project>,
    /// Path of the currently active project (None if no project selected)
    pub active: Option<PathBuf>,
}

/// Check if a path is a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    // Check for .git directory
    if path.join(".git").exists() {
        return true;
    }
    // Also check via git command (handles worktrees and bare repos)
    std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Derive project ID from path (matches shards-core's project ID generation).
pub fn derive_project_id(path: &Path) -> String {
    // shards-core uses the directory name as project_id
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Load projects from ~/.shards/projects.json
pub fn load_projects() -> ProjectsData {
    let path = projects_file_path();
    if !path.exists() {
        return ProjectsData::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(e) => {
            tracing::warn!(
                event = "ui.projects.load_failed",
                path = %path.display(),
                error = %e
            );
            ProjectsData::default()
        }
    }
}

/// Save projects to ~/.shards/projects.json
pub fn save_projects(data: &ProjectsData) -> Result<(), String> {
    let path = projects_file_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write projects file: {}", e))?;

    tracing::info!(
        event = "ui.projects.saved",
        path = %path.display(),
        count = data.projects.len()
    );

    Ok(())
}

fn projects_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".shards")
        .join("projects.json")
}

/// Validation result for adding a project.
#[derive(Debug)]
pub enum ProjectValidation {
    Valid,
    NotADirectory,
    NotAGitRepo,
    AlreadyExists,
}

/// Validate a path before adding as a project.
pub fn validate_project_path(path: &Path, existing: &[Project]) -> ProjectValidation {
    if !path.is_dir() {
        return ProjectValidation::NotADirectory;
    }
    if !is_git_repo(path) {
        return ProjectValidation::NotAGitRepo;
    }
    if existing.iter().any(|p| p.path == path) {
        return ProjectValidation::AlreadyExists;
    }
    ProjectValidation::Valid
}
```

**MIRROR**: Follow serde patterns from `crates/shards-core/src/sessions/types.rs`

**Validation**: `cargo check -p shards-ui`

---

### Task 2: UPDATE `crates/shards-ui/src/state.rs`

Add project state to AppState.

**Changes**:
1. Add imports for projects module
2. Add `AddProjectDialogField` enum
3. Add `AddProjectFormState` struct
4. Add project fields to `AppState`:
   - `projects: Vec<Project>`
   - `active_project: Option<PathBuf>`
   - `show_add_project_dialog: bool`
   - `add_project_form: AddProjectFormState`
   - `add_project_error: Option<String>`
5. Add helper methods:
   - `active_project_id(&self) -> Option<String>` - derives project_id from active path
   - `filtered_displays(&self) -> Vec<&ShardDisplay>` - filters by active project
   - `reset_add_project_form(&mut self)`
6. Update `AppState::new()` to load projects on startup

**MIRROR**: Pattern from `CreateFormState` for the form state

**Validation**: `cargo check -p shards-ui`

---

### Task 3: UPDATE `crates/shards-ui/src/actions.rs`

Add project actions.

**Add functions**:
```rust
/// Add a new project after validation.
pub fn add_project(path: PathBuf, name: Option<String>) -> Result<Project, String>

/// Remove a project from the list (doesn't affect shards).
pub fn remove_project(path: &Path) -> Result<(), String>

/// Set the active project.
pub fn set_active_project(path: Option<PathBuf>) -> Result<(), String>

/// Auto-track a project when a shard is created in it.
/// Called after successful shard creation.
pub fn auto_track_project(project_id: &str, worktree_path: &Path) -> Result<(), String>
```

**Update `refresh_sessions`**: Add optional project_id filter parameter.

**Validation**: `cargo check -p shards-ui`

---

### Task 4: CREATE `crates/shards-ui/src/views/add_project_dialog.rs`

Dialog for adding new projects.

**Fields**:
- Path input (text field, keyboard capture)
- Name input (optional, defaults to directory name)
- Error display

**Layout** (mirror `create_dialog.rs`):
```
┌─────────────────────────────────┐
│ Add Project                     │
├─────────────────────────────────┤
│ Path                            │
│ [/path/to/repo____________]     │
│                                 │
│ Name (optional)                 │
│ [project-name_____________]     │
│                                 │
│ [Error message if any]          │
├─────────────────────────────────┤
│              [Cancel] [Add]     │
└─────────────────────────────────┘
```

**Keyboard handling**:
- Tab cycles between Path → Name → Path
- Enter submits
- Escape cancels
- Path field: accepts any printable character (file paths can have spaces, etc.)

**MIRROR**: `crates/shards-ui/src/views/create_dialog.rs`

**Validation**: `cargo check -p shards-ui`

---

### Task 5: CREATE `crates/shards-ui/src/views/project_selector.rs`

Dropdown component for switching projects.

**States**:
- No projects: Show "Add Project" button
- Projects exist: Show dropdown with active project name + "Add Project" option

**Layout**:
```
┌──────────────────┐
│ my-project  ▼    │  <- Click to expand dropdown
└──────────────────┘

Expanded:
┌──────────────────┐
│ my-project  ▲    │
├──────────────────┤
│ ● my-project     │  <- Active (checkmark or filled dot)
│ ○ other-project  │
│ ○ third-project  │
├──────────────────┤
│ + Add Project    │
│ ─────────────────│
│ Remove current   │  <- Only when dropdown expanded
└──────────────────┘
```

**Click handlers**:
- Click project → set as active, close dropdown
- Click "Add Project" → open add project dialog
- Click "Remove current" → remove active project from list

**MIRROR**: Agent selector pattern from `create_dialog.rs`, but as a dropdown

**Validation**: `cargo check -p shards-ui`

---

### Task 6: UPDATE `crates/shards-ui/src/views/mod.rs`

Export new view modules.

```rust
pub mod add_project_dialog;
pub mod project_selector;
```

**Validation**: `cargo check -p shards-ui`

---

### Task 7: UPDATE `crates/shards-ui/src/views/main_view.rs`

Integrate project selector and dialog.

**Changes**:
1. Import new modules
2. Add project selector to header (left of "Open All" button)
3. Add state for dropdown open/closed: `show_project_dropdown: bool`
4. Add handlers:
   - `on_add_project_click` → opens dialog
   - `on_add_project_submit` → validates and adds
   - `on_add_project_cancel` → closes dialog
   - `on_project_select` → sets active project
   - `on_remove_project` → removes from list
   - `on_toggle_project_dropdown` → toggles dropdown
5. Add keyboard handling for add project dialog
6. Filter shard list by active project in render
7. Conditionally render add project dialog

**Header layout**:
```
┌─────────────────────────────────────────────────────────────────┐
│ Shards    [my-project ▼]    [▶ Open All] [⏹ Stop All] [↻] [+]  │
└─────────────────────────────────────────────────────────────────┘
```

**MIRROR**: Create dialog integration pattern

**Validation**: `cargo check -p shards-ui`

---

### Task 8: UPDATE `crates/shards-ui/src/main.rs`

Export projects module.

Add: `mod projects;`

**Validation**: `cargo check -p shards-ui`

---

### Task 9: UPDATE shard list filtering

In `main_view.rs` render, filter `state.displays` by active project before passing to `shard_list::render_shard_list`.

**Logic**:
```rust
let filtered_displays: Vec<_> = if let Some(active_path) = &self.state.active_project {
    let active_id = crate::projects::derive_project_id(active_path);
    self.state.displays.iter()
        .filter(|d| d.session.project_id == active_id)
        .collect()
} else {
    // No active project - show all shards
    self.state.displays.iter().collect()
};
```

**Update `shard_list::render_shard_list`** to accept `&[&ShardDisplay]` instead of getting from state directly, or filter in state.

**Validation**: `cargo check -p shards-ui`

---

### Task 10: Auto-track projects on shard creation

After successful shard creation in `on_dialog_submit`, auto-add the project if not already tracked.

**Logic**:
```rust
// After create_shard succeeds:
if let Some(ref active_path) = self.state.active_project {
    // Already have an active project, this shard was created there
} else {
    // Creating shard without active project context
    // Try to derive project from the session's worktree path
    let project_path = session.worktree_path.parent()
        .and_then(|p| p.parent()); // worktree is ~/.shards/worktrees/{project}/{branch}
    if let Some(path) = project_path {
        // Auto-track if not already in list
        actions::auto_track_project(&session.project_id, path);
    }
}
```

**Note**: Need to think about this more - in the UI, we should probably require an active project before creating shards. Otherwise the user experience is confusing. For MVP, we can require active project selection.

**Validation**: `cargo check -p shards-ui`

---

### Task 11: Empty state UX

When no projects exist, show a friendly empty state instead of an empty shard list.

**Layout**:
```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                    Welcome to Shards!                           │
│                                                                 │
│         Add a project to start creating shards.                 │
│                                                                 │
│                    [+ Add Project]                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Validation**: `cargo check -p shards-ui`

---

### Task 12: Persist active project selection

Save active project when changed, load on startup.

**Already handled in Task 1** via `ProjectsData.active` field.

**Validation**: Full test - `cargo run -p shards-ui`

---

### Task 13: Write tests

**Test cases**:
1. `test_is_git_repo_valid` - returns true for git repo
2. `test_is_git_repo_invalid` - returns false for non-git directory
3. `test_validate_project_path_not_directory` - returns NotADirectory
4. `test_validate_project_path_not_git` - returns NotAGitRepo
5. `test_validate_project_path_already_exists` - returns AlreadyExists
6. `test_validate_project_path_valid` - returns Valid
7. `test_load_projects_missing_file` - returns empty default
8. `test_save_and_load_projects` - round-trip
9. `test_derive_project_id` - extracts directory name

**Validation**: `cargo test -p shards-ui`

---

## Validation Commands

```bash
# Format
cargo fmt --check

# Lint
cargo clippy -p shards-ui -- -D warnings

# Type check
cargo check -p shards-ui

# Build
cargo build -p shards-ui

# Test
cargo test -p shards-ui

# Full validation
cargo fmt --check && cargo clippy -p shards-ui -- -D warnings && cargo test -p shards-ui && cargo build -p shards-ui
```

---

## Acceptance Criteria

- [ ] Projects stored in `~/.shards/projects.json`
- [ ] Add project via dialog with path + optional name
- [ ] Path validated as git repository (error if not)
- [ ] Switch between projects via dropdown
- [ ] Shard list filters to active project only
- [ ] Remove project from list (doesn't affect shards)
- [ ] Active project persists across app restarts
- [ ] Empty state shown when no projects
- [ ] All validation commands pass

---

## Open Questions (Resolved)

1. **Should we require active project before creating shards?**
   - YES for MVP. Simplifies UX. User must select/add a project first.

2. **What if user has existing shards but no projects?**
   - Show all shards in "All Projects" view, or prompt to add projects
   - For MVP: show empty state, guide to add project

3. **How to handle project_id matching?**
   - `derive_project_id(path)` matches shards-core's approach (uses directory name)
