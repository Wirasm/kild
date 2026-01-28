# Investigation: UI shows 'Welcome to KILD' when kilds exist but no projects are added

**Issue**: #142 (https://github.com/Wirasm/kild/issues/142)
**Type**: BUG
**Investigated**: 2026-01-28T18:30:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                         |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Feature partially broken - CLI-created kilds invisible in UI, but workaround exists (add project via UI)          |
| Complexity | LOW    | Single condition change in one file, no integration points affected                                               |
| Confidence | HIGH   | Root cause clearly identified with evidence, well-understood code path, exact commit that introduced bug located |

---

## Problem Statement

The kild-ui main content area shows "Welcome to KILD! Add a project to start creating kilds." even when kilds exist (visible in sidebar "All" count and header buttons). Kilds created via CLI are invisible in the UI until a project is manually added.

---

## Analysis

### Root Cause

The condition `state.projects.is_empty()` at `kild_list.rs:70` gates the entire kild list display, preventing the `filtered_displays()` method from ever being called when no UI projects exist - even though that method correctly handles the "show all kilds" case.

### Evidence Chain

WHY: "Welcome to KILD!" shown instead of kild list
↓ BECAUSE: `state.projects.is_empty()` returns `true` at `kild_list.rs:70`
Evidence: `crates/kild-ui/src/views/kild_list.rs:70` - `else if state.projects.is_empty() {`

↓ BECAUSE: `projects` vector is empty (loaded from `~/.kild/projects.json`)
Evidence: No projects added via UI, even though kilds exist from CLI creation

↓ BECAUSE: Projects and displays are loaded from different sources
- `projects` loaded from `~/.kild/projects.json` (UI-managed)
- `displays` loaded from `~/.kild/sessions/*.json` (CLI/UI-managed)

↓ ROOT CAUSE: The condition `projects.is_empty()` was intended to show an onboarding screen, but it incorrectly blocks displaying CLI-created kilds
Evidence: `crates/kild-ui/src/views/kild_list.rs:70-96` - Welcome message branch prevents `filtered_displays()` from being reached

### Affected Files

| File                                      | Lines | Action | Description                               |
| ----------------------------------------- | ----- | ------ | ----------------------------------------- |
| `crates/kild-ui/src/views/kild_list.rs`   | 70    | UPDATE | Fix condition to include displays check   |

### Integration Points

- `state.rs:464-473` - `filtered_displays()` correctly handles "no active project" case by returning all kilds
- `sidebar.rs:65` - Sidebar uses `state.total_kild_count()` which correctly shows all kilds
- `main_view.rs:887,901` - Header buttons use `stopped_count()`/`running_count()` which correctly count all kilds

### Git History

- **Introduced**: `13a3b16` (2026-01-26) - "Add multi-project support to shards-ui (Phase 8)"
- **Original condition**: `state.displays.is_empty()` - showed empty state only when no kilds existed
- **Changed to**: `state.projects.is_empty()` - shows welcome when no projects, regardless of kilds
- **Implication**: Regression from Phase 8 multi-project feature

---

## Implementation Plan

### Step 1: Fix the welcome screen condition

**File**: `crates/kild-ui/src/views/kild_list.rs`
**Lines**: 70
**Action**: UPDATE

**Current code:**

```rust
// Line 70
    } else if state.projects.is_empty() {
```

**Required change:**

```rust
// Line 70
    } else if state.projects.is_empty() && state.displays.is_empty() {
```

**Why**: Welcome screen should only show when there are NO projects AND NO kilds. This ensures:
- CLI-created kilds display even without UI projects
- "All" filter correctly shows all kilds when selected
- Welcome screen still appears for true first-run experience

---

## Patterns to Follow

**From codebase - the `filtered_displays()` method already handles this correctly:**

```rust
// SOURCE: crates/kild-ui/src/state.rs:464-473
// Pattern for handling "no active project" case
pub fn filtered_displays(&self) -> Vec<&KildDisplay> {
    if let Some(active_id) = self.active_project_id() {
        self.displays
            .iter()
            .filter(|d| d.session.project_id == active_id)
            .collect()
    } else {
        // No active project - show ALL kilds
        self.displays.iter().collect()
    }
}
```

The existing code at lines 101-107 already handles the empty filtered list case correctly:

```rust
// SOURCE: crates/kild-ui/src/views/kild_list.rs:101-107
if filtered.is_empty() {
    let message = if state.active_project.is_some() {
        "No active kilds for this project"
    } else {
        "No active kilds"
    };
    // ... render empty state
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                         | Mitigation                                                                     |
| -------------------------------------- | ------------------------------------------------------------------------------ |
| First-run experience unchanged         | When both `projects` AND `displays` are empty, welcome screen still appears    |
| Project added but no kilds for project | Handled by existing `filtered.is_empty()` check at line 101                    |
| All filter with kilds from CLI         | Now correctly shows kilds since `filtered_displays()` returns all              |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy -p kild-ui -- -D warnings
cargo test -p kild-ui
cargo build -p kild-ui
```

### Manual Verification

1. Delete `~/.kild/projects.json` or set `"projects": []`
2. Create a kild via CLI: `kild create test-branch`
3. Start kild-ui: `cargo run -p kild-ui`
4. Verify: Sidebar shows "All (1)", main content shows kild list (not welcome screen)
5. Verify: With no projects AND no kilds, welcome screen appears

---

## Scope Boundaries

**IN SCOPE:**

- Fix the condition at `kild_list.rs:70` to check both `projects.is_empty()` AND `displays.is_empty()`

**OUT OF SCOPE (do not touch):**

- Sidebar rendering (already correct)
- Header buttons (already correct)
- `filtered_displays()` logic (already correct)
- Project management features
- Any refactoring beyond the single condition fix

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T18:30:00Z
- **Artifact**: `.claude/PRPs/issues/issue-142.md`
