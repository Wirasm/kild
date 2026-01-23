# Feature: GPUI Phase 1 - Project Scaffolding

## Summary

Add GPUI as a dependency to the `shards-ui` crate with proper workspace integration. This phase establishes the build foundation for the UI without implementing any functionality. The goal is to make `cargo check -p shards-ui` pass with GPUI available.

## User Story

As a developer working on shards-ui
I want GPUI dependencies to compile correctly
So that I can build the visual dashboard in subsequent phases

## Problem Statement

The `shards-ui` crate exists as a placeholder with no actual UI framework. We need to add GPUI as a dependency following the existing workspace pattern so future phases can build on it.

## Solution Statement

Add `gpui = "0.2"` to workspace dependencies and wire it up in `shards-ui/Cargo.toml`. Update `main.rs` to import gpui and prove it compiles. No UI functionality yet.

## Metadata

| Field            | Value                    |
| ---------------- | ------------------------ |
| Type             | NEW_CAPABILITY           |
| Complexity       | LOW                      |
| Systems Affected | shards-ui, workspace     |
| Dependencies     | gpui 0.2.2               |
| Estimated Tasks  | 3                        |

---

## UX Design

### Before State

```
┌─────────────────────────────────────────────────────────────┐
│                      CURRENT STATE                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  $ cargo build -p shards-ui                                 │
│  Compiling shards-ui v0.1.0                                 │
│  Finished `dev` profile                                     │
│                                                             │
│  $ ./target/debug/shards-ui                                 │
│  shards-ui is not yet implemented.                          │
│  See Phase 1 of gpui-native-terminal-ui.prd.md...           │
│  [exit 1]                                                   │
│                                                             │
│  NO GPUI DEPENDENCY - Cannot build UI components            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### After State

```
┌─────────────────────────────────────────────────────────────┐
│                       AFTER STATE                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  $ cargo check -p shards-ui                                 │
│  Compiling gpui v0.2.2                                      │
│  Compiling shards-ui v0.1.0                                 │
│  Finished `dev` profile                                     │
│                                                             │
│  $ cargo build -p shards-ui                                 │
│  [builds successfully with gpui linked]                     │
│                                                             │
│  $ ./target/debug/shards-ui                                 │
│  shards-ui is not yet implemented.                          │
│  GPUI scaffolding ready. See Phase 2 to continue.           │
│  [exit 1]                                                   │
│                                                             │
│  GPUI AVAILABLE - Ready for Phase 2 window implementation   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `cargo check -p shards-ui` | No GPUI | GPUI compiles | Can start UI dev |
| `cargo check` (CLI) | Works | Still works | No regression |
| Binary size | Small | Larger (GPUI) | Expected |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `Cargo.toml` (root) | 1-35 | Workspace dependency pattern to MIRROR |
| P0 | `crates/shards-ui/Cargo.toml` | 1-17 | Current state, where to add dependency |
| P0 | `crates/shards-ui/src/main.rs` | 1-10 | Current placeholder to update |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI crates.io](https://crates.io/crates/gpui) | Version | Confirm 0.2.2 latest |
| [GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md) | Requirements | macOS + Metal + Xcode needed |

---

## Patterns to Mirror

**WORKSPACE_DEPENDENCY_PATTERN:**
```toml
# SOURCE: Cargo.toml:11-34
# COPY THIS PATTERN for adding new workspace deps:
[workspace.dependencies]
# Core dependencies (shared across crates)
thiserror = "2"
tracing = "0.1"
# ... add gpui here following same style
```

**CRATE_DEPENDENCY_PATTERN:**
```toml
# SOURCE: crates/shards-ui/Cargo.toml:12-13
# COPY THIS PATTERN for using workspace deps:
[dependencies]
shards-core.workspace = true
# ... add gpui.workspace = true here
```

---

## Files to Change

| File | Action | Justification |
| ---- | ------ | ------------- |
| `Cargo.toml` (root) | UPDATE | Add gpui to workspace.dependencies |
| `crates/shards-ui/Cargo.toml` | UPDATE | Reference gpui from workspace |
| `crates/shards-ui/src/main.rs` | UPDATE | Import gpui to prove it compiles |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No GPUI window creation** - That's Phase 2
- **No UI modules** - Just scaffolding
- **No feature flags** - shards-ui is already a separate crate (isolation achieved)
- **No shards-core changes** - UI crate only
- **No CLI changes** - shards CLI is unaffected

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (workspace root)

- **ACTION**: Add gpui to workspace.dependencies
- **IMPLEMENT**: Add `gpui = "0.2"` following existing dependency style
- **LOCATION**: After line 34 (after `shards-core` entry), add a UI section comment
- **MIRROR**: Follow exact formatting of existing entries (no trailing features unless needed)
- **EXACT CHANGE**:
  ```toml
  # After line 34, add:

  # UI dependencies
  gpui = "0.2"
  ```
- **VALIDATE**: `cargo check` - workspace parses correctly

### Task 2: UPDATE `crates/shards-ui/Cargo.toml`

- **ACTION**: Reference gpui from workspace
- **IMPLEMENT**: Add `gpui.workspace = true` to dependencies
- **LOCATION**: After `shards-core.workspace = true`, remove the comment about Phase 1
- **EXACT CHANGE**:
  ```toml
  [dependencies]
  shards-core.workspace = true
  gpui.workspace = true
  ```
- **VALIDATE**: `cargo check -p shards-ui` - gpui dependency resolves

### Task 3: UPDATE `crates/shards-ui/src/main.rs`

- **ACTION**: Import gpui to prove it compiles, update placeholder message
- **IMPLEMENT**: Add `use gpui;` and update the exit message
- **MIRROR**: Keep doc comment style from existing file
- **EXACT CHANGE**:
  ```rust
  //! shards-ui: GUI for Shards
  //!
  //! GPUI-based visual dashboard for shard management.
  //! See .claude/PRPs/prds/gpui-native-terminal-ui.prd.md for implementation plan.

  // Import gpui to verify dependency compiles
  use gpui as _;

  fn main() {
      eprintln!("shards-ui: GPUI scaffolding ready.");
      eprintln!("See Phase 2 of gpui-native-terminal-ui.prd.md to continue.");
      std::process::exit(1);
  }
  ```
- **VALIDATE**: `cargo check -p shards-ui` && `cargo build -p shards-ui`

---

## Testing Strategy

### Validation Tests

| Test | Command | Expected |
|------|---------|----------|
| Workspace parses | `cargo check` | Exit 0 |
| GPUI compiles | `cargo check -p shards-ui` | Exit 0, includes gpui |
| CLI unaffected | `cargo check -p shards` | Exit 0, no gpui |
| Build succeeds | `cargo build -p shards-ui` | Binary created |
| Binary runs | `./target/debug/shards-ui` | Prints message, exits 1 |

### Edge Cases Checklist

- [ ] `cargo check` (full workspace) still works
- [ ] `cargo build -p shards` (CLI) doesn't pull gpui
- [ ] `cargo clippy --all -- -D warnings` passes
- [ ] `cargo fmt --check` passes

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: TYPE_CHECK

```bash
cargo check && cargo check -p shards-ui
```

**EXPECT**: Exit 0, gpui dependency resolves

### Level 3: BUILD

```bash
cargo build -p shards-ui
```

**EXPECT**: Binary created at `target/debug/shards-ui`

### Level 4: SMOKE_TEST

```bash
./target/debug/shards-ui 2>&1 | grep -q "GPUI scaffolding ready"
```

**EXPECT**: Exit 0, message found

### Level 5: REGRESSION_CHECK

```bash
cargo build -p shards && cargo test --all
```

**EXPECT**: CLI builds without gpui, all tests pass

---

## Acceptance Criteria

- [ ] `cargo check` passes (workspace valid)
- [ ] `cargo check -p shards-ui` passes (gpui compiles)
- [ ] `cargo build -p shards-ui` produces binary
- [ ] `cargo build -p shards` does NOT include gpui (check with `cargo tree -p shards | grep gpui` returns empty)
- [ ] `cargo clippy --all -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Running binary prints "GPUI scaffolding ready" message

---

## Completion Checklist

- [ ] Task 1: Workspace Cargo.toml updated with gpui
- [ ] Task 2: shards-ui Cargo.toml references gpui
- [ ] Task 3: main.rs imports gpui and compiles
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Type check passes
- [ ] Level 3: Build succeeds
- [ ] Level 4: Smoke test passes
- [ ] Level 5: Regression check passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| GPUI doesn't compile on edition 2024 | LOW | HIGH | Test immediately; if fails, check gpui issues |
| GPUI version mismatch | LOW | LOW | Using latest (0.2.2), workspace manages version |
| Metal/Xcode not installed | MED | HIGH | PRD requirement: macOS with Xcode installed |

---

## Notes

- GPUI requires macOS with Metal support and Xcode installed
- This is a pure scaffolding phase - no UI functionality
- The `use gpui as _;` pattern imports the crate without using anything, proving compilation works
- Phase 2 will create an actual window
