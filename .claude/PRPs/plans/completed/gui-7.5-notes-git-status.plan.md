# Implementation Plan: GUI Phase 7.5 - Notes & Git Status

**Source PRD**: `.claude/PRPs/prds/gpui-native-terminal-ui.prd.md`
**Phase**: 7.5
**Status**: READY FOR IMPLEMENTATION
**Dependencies**: CLI-1.1 (`--note`) - DONE

---

## Summary

Add session notes display and git dirty indicators to the shard list view. Add a note text field to the create dialog. This enhances the UI by helping users:
1. Remember what each shard is for via notes
2. See at a glance which shards have uncommitted work (git dirty indicator)

## User Story

As a power user managing multiple shards, I want to see session notes in the list and add notes when creating shards so that I can quickly remember the purpose of each shard. I also want to see which shards have uncommitted changes so I can avoid destroying work accidentally.

## Problem Statement

Currently, the shard list shows branch, agent, project, and status. Users cannot:
- See what each shard is being used for
- Quickly identify shards with uncommitted changes before destroying
- Add notes when creating shards via the UI

## Solution Statement

1. Display the `note` field from Session in the shard list (truncated to ~25 chars, full on hover)
2. Add a git dirty indicator (orange dot) when worktree has uncommitted changes
3. Add a "Note" text input field to the create dialog
4. Pass note to `create_session` via `CreateSessionRequest`

## Metadata

| Field | Value |
|-------|-------|
| Type | ENHANCEMENT |
| Complexity | MEDIUM |
| Systems Affected | shards-ui |
| Dependencies | CLI-1.1 (--note) - DONE |
| Estimated Tasks | 6 |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-ui/src/views/shard_list.rs` | 1-198 | Current list implementation |
| P0 | `crates/shards-ui/src/views/create_dialog.rs` | 1-219 | Dialog structure, field pattern |
| P0 | `crates/shards-ui/src/state.rs` | 56-100 | CreateFormState, form field pattern |
| P1 | `crates/shards-ui/src/actions.rs` | 10-60 | create_shard function |
| P1 | `crates/shards-core/src/sessions/types.rs` | 82-127 | Session.note field, CreateSessionRequest |

---

## Patterns to Mirror

**Form Field in CreateFormState:**
```rust
// SOURCE: crates/shards-ui/src/state.rs:56-62
pub struct CreateFormState {
    pub branch_name: String,
    pub selected_agent: String,
    pub selected_agent_index: usize,
}
```

**Dialog Text Input Field:**
```rust
// SOURCE: crates/shards-ui/src/views/create_dialog.rs:74-107
// Branch name field pattern
.child(
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().text_color(rgb(0xaaaaaa)).child("Branch Name"))
        .child(
            div()
                .px_3()
                .py_2()
                .bg(rgb(0x1e1e1e))
                .rounded_md()
                .border_1()
                .border_color(rgb(0x555555))
                .min_h(px(36.0))
                .child(div().text_color(...).child(...)),
        ),
)
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-ui/src/state.rs` | UPDATE | Add note to CreateFormState, git_dirty to ShardDisplay |
| `crates/shards-ui/src/views/create_dialog.rs` | UPDATE | Add Note text field |
| `crates/shards-ui/src/views/shard_list.rs` | UPDATE | Add note column, git dirty indicator |
| `crates/shards-ui/src/actions.rs` | UPDATE | Pass note to CreateSessionRequest |
| `crates/shards-ui/src/views/main_view.rs` | UPDATE | Handle note field keyboard input |

---

## NOT Building (Scope Limits)

- **Full git diff view** - Phase 7.7 Quick Actions
- **Async git status refresh** - Keep simple, sync check is acceptable
- **Note editing after creation** - Not in PRD scope
- **Inline note expansion** - Tooltip on hover is sufficient

---

## Step-by-Step Tasks

### Task 1: Add note field to CreateFormState

- **ACTION**: Add note string field to form state
- **FILE**: `crates/shards-ui/src/state.rs`
- **IMPLEMENT**:
```rust
pub struct CreateFormState {
    pub branch_name: String,
    pub selected_agent: String,
    pub selected_agent_index: usize,
    pub note: String,  // NEW
    pub focused_field: CreateDialogField,  // NEW for Tab navigation
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum CreateDialogField {
    #[default]
    BranchName,
    Agent,
    Note,
}
```
- Initialize in Default: `note: String::new(), focused_field: CreateDialogField::default()`
- **VALIDATE**: `cargo check -p shards-ui`

### Task 2: Add git_dirty field to ShardDisplay

- **ACTION**: Check git status when loading sessions
- **FILE**: `crates/shards-ui/src/state.rs`
- **IMPLEMENT**:
```rust
pub struct ShardDisplay {
    pub session: Session,
    pub status: ProcessStatus,
    pub git_dirty: bool,  // NEW
}

fn check_git_dirty(worktree_path: &std::path::Path) -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

impl ShardDisplay {
    pub fn from_session(session: Session) -> Self {
        let status = /* existing code */;
        let git_dirty = if session.worktree_path.exists() {
            check_git_dirty(&session.worktree_path)
        } else {
            false
        };
        Self { session, status, git_dirty }
    }
}
```
- **VALIDATE**: `cargo check -p shards-ui`

### Task 3: Add Note field to create dialog UI

- **ACTION**: Add text input field for notes
- **FILE**: `crates/shards-ui/src/views/create_dialog.rs`
- **LOCATION**: After Agent selector
- **IMPLEMENT**:
```rust
let note = state.create_form.note.clone();

// Note field (after Agent selector)
.child(
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().text_color(rgb(0xaaaaaa)).child("Note (optional)"))
        .child(
            div()
                .px_3()
                .py_2()
                .bg(rgb(0x1e1e1e))
                .rounded_md()
                .border_1()
                .border_color(rgb(0x555555))
                .min_h(px(36.0))
                .child(
                    div()
                        .text_color(if note.is_empty() { rgb(0x666666) } else { rgb(0xffffff) })
                        .child(if note.is_empty() {
                            "What is this shard for?".to_string()
                        } else if state.create_form.focused_field == CreateDialogField::Note {
                            format!("{}|", note)  // Show cursor
                        } else {
                            note.clone()
                        }),
                ),
        ),
)
```
- **MIRROR**: Branch name field structure
- **VALIDATE**: `cargo run -p shards-ui`

### Task 4: Display note and git indicator in shard list

- **ACTION**: Add columns to list view
- **FILE**: `crates/shards-ui/src/views/shard_list.rs`
- **IMPLEMENT**:
```rust
// Git dirty indicator (after status indicator)
.when(display.git_dirty, |row| {
    row.child(div().text_color(rgb(0xffa500)).child("●"))  // Orange
})

// Note column (truncated)
.when_some(display.session.note.clone(), |row, note| {
    let truncated = if note.chars().count() > 25 {
        format!("{}...", note.chars().take(25).collect::<String>())
    } else {
        note.clone()
    };
    row.child(div().text_color(rgb(0x888888)).text_sm().child(truncated))
})
```
- **VALIDATE**: `cargo run -p shards-ui`

### Task 5: Handle note field keyboard input

- **ACTION**: Add Tab navigation and note input handling
- **FILE**: `crates/shards-ui/src/views/main_view.rs`
- **IMPLEMENT** (in `on_key_down`):
```rust
"tab" => {
    self.state.create_form.focused_field = match self.state.create_form.focused_field {
        CreateDialogField::BranchName => CreateDialogField::Agent,
        CreateDialogField::Agent => CreateDialogField::Note,
        CreateDialogField::Note => CreateDialogField::BranchName,
    };
    cx.notify();
}

"space" => {
    match self.state.create_form.focused_field {
        CreateDialogField::BranchName => self.state.create_form.branch_name.push('-'),
        CreateDialogField::Note => self.state.create_form.note.push(' '),
        CreateDialogField::Agent => {}
    }
    cx.notify();
}

"backspace" => {
    match self.state.create_form.focused_field {
        CreateDialogField::BranchName => { self.state.create_form.branch_name.pop(); }
        CreateDialogField::Note => { self.state.create_form.note.pop(); }
        CreateDialogField::Agent => {}
    }
    cx.notify();
}

key if key.len() == 1 => {
    if let Some(c) = key.chars().next() {
        match self.state.create_form.focused_field {
            CreateDialogField::BranchName => {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                    self.state.create_form.branch_name.push(c);
                }
            }
            CreateDialogField::Note => {
                if !c.is_control() {
                    self.state.create_form.note.push(c);
                }
            }
            CreateDialogField::Agent => {}
        }
        cx.notify();
    }
}
```
- **IMPORTS**: `use crate::state::CreateDialogField;`
- **VALIDATE**: `cargo run -p shards-ui`

### Task 6: Pass note to create_shard action

- **ACTION**: Update action to include note
- **FILE**: `crates/shards-ui/src/actions.rs`
- **IMPLEMENT**:
```rust
pub fn create_shard(branch: &str, agent: &str, note: Option<String>) -> Result<Session, String> {
    // ...
    let request = CreateSessionRequest::new(branch.to_string(), Some(agent.to_string()), note);
    // ...
}
```
- **Update caller in main_view.rs**:
```rust
let note = if self.state.create_form.note.trim().is_empty() {
    None
} else {
    Some(self.state.create_form.note.trim().to_string())
};
match actions::create_shard(&branch, &agent, note) { ... }
```
- **Also reset note in reset_create_form()**
- **VALIDATE**: `cargo check -p shards-ui`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

### Level 2: TYPE_CHECK
```bash
cargo check --all
```

### Level 3: BUILD
```bash
cargo build --all
```

### Level 4: TESTS
```bash
cargo test --all
```

### Level 5: MANUAL_VALIDATION
```bash
# Create shard with note via CLI
cargo run -p shards -- create test-cli --note "CLI test"

# Open UI, verify note displays truncated in list
cargo run -p shards-ui

# Create shard via UI with note
# Tab through fields, enter note, verify it appears

# Make uncommitted changes in worktree
echo "test" >> ~/.shards/worktrees/*/test-cli/test.txt

# Refresh UI, verify git dirty indicator (orange dot) appears
```

---

## Acceptance Criteria

- [ ] Notes display in shard list, truncated to 25 chars with "..."
- [ ] Git dirty indicator (orange dot) shows when uncommitted changes
- [ ] Create dialog has Note field with placeholder
- [ ] Tab cycles through fields (branch -> agent -> note)
- [ ] Note field allows spaces and general text
- [ ] Note is passed to create_session and persisted
- [ ] All validation commands pass

---

## Completion Checklist

- [ ] Task 1: CreateFormState.note and CreateDialogField added
- [ ] Task 2: ShardDisplay.git_dirty and check_git_dirty() added
- [ ] Task 3: Note field added to create dialog UI
- [ ] Task 4: Note column and git indicator added to shard list
- [ ] Task 5: Keyboard input handling with focus tracking
- [ ] Task 6: create_shard() passes note to CreateSessionRequest
- [ ] Level 1-4 validation commands pass
- [ ] Manual testing completed
