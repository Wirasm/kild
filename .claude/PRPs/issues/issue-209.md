# Investigation: Build Issue - found a virtual manifest

**Issue**: #209 (https://github.com/Wirasm/kild/issues/209)
**Type**: DOCUMENTATION
**Investigated**: 2026-02-05T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                           |
| ---------- | ------ | --------------------------------------------------------------------------------------------------- |
| Priority   | MEDIUM | Blocks new users from installing successfully; first impression issue, but workaround is documented  |
| Complexity | LOW    | Single file change (README.md line 46), isolated documentation fix                                  |
| Confidence | HIGH   | Root cause is definitively identified: README.md has wrong path, correct path exists in cookbook     |

---

## Problem Statement

The README.md installation section tells users to run `cargo install --path .`, but the root `Cargo.toml` is a virtual workspace manifest with no `[package]` section. This causes cargo to error with "found a virtual manifest instead of a package manifest". The correct command is `cargo install --path crates/kild`.

---

## Analysis

### Root Cause

The README.md installation instruction at line 46 points to the workspace root (`.`) instead of the specific binary crate (`crates/kild`). The root `Cargo.toml` defines a virtual workspace (`[workspace]` with no `[package]`), which is not installable.

### Evidence Chain

WHY: `cargo install --path .` fails with "found a virtual manifest"
↓ BECAUSE: The root `Cargo.toml` is a virtual workspace manifest
Evidence: `Cargo.toml:1-3` - `[workspace] resolver = "3" members = ["crates/*"]`

↓ BECAUSE: The installable binary is in a sub-crate, not at root
Evidence: `crates/kild/Cargo.toml:8-10` - `[[bin]] name = "kild" path = "src/main.rs"`

↓ ROOT CAUSE: README.md contains incorrect install path
Evidence: `README.md:46` - `cargo install --path .`

### Affected Files

| File        | Lines | Action | Description                                           |
| ----------- | ----- | ------ | ----------------------------------------------------- |
| `README.md` | 45-47 | UPDATE | Fix install command from `--path .` to `--path crates/kild` |

### Integration Points

- `.claude/skills/kild/cookbook/installation.md:14` already has the correct command
- No other files reference `cargo install --path .`

### Git History

- **Introduced**: `f8ee571` - 2026-01-08 - Original README content
- **Implication**: Long-standing documentation bug since README was created

---

## Implementation Plan

### Step 1: Fix installation command in README.md

**File**: `README.md`
**Lines**: 45-47
**Action**: UPDATE

**Current code:**

```markdown
```bash
cargo install --path .
```​
```

**Required change:**

```markdown
```bash
cargo install --path crates/kild
```​
```

**Why**: The root `Cargo.toml` is a virtual workspace manifest. The installable binary crate is at `crates/kild`.

---

## Patterns to Follow

**From codebase - the correct install command already exists in the cookbook:**

```markdown
# SOURCE: .claude/skills/kild/cookbook/installation.md:11-14
# Pattern for correct installation instruction
From the KILD repo root, run:

cargo install --path crates/kild
```

---

## Edge Cases & Risks

| Risk/Edge Case              | Mitigation                                                    |
| --------------------------- | ------------------------------------------------------------- |
| kild-peek install not documented | Out of scope - separate issue if needed                  |
| kild-ui install not documented   | Out of scope - kild-ui is the GUI, separate concern      |

---

## Validation

### Manual Verification

1. Run `cargo install --path crates/kild` from repo root - should succeed
2. Read updated README.md and confirm the command is correct
3. Compare with `.claude/skills/kild/cookbook/installation.md` for consistency

---

## Scope Boundaries

**IN SCOPE:**

- Fix the install command in README.md line 46

**OUT OF SCOPE (do not touch):**

- Adding installation instructions for kild-peek or kild-ui
- Expanding the README installation section (keep it minimal)
- Modifying the cookbook installation.md (already correct)
- Any code changes

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-05T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-209.md`
