# Investigation: Rename *Manager types to domain-role names

**Issue**: #520 (https://github.com/Wirasm/kild/issues/520)
**Type**: REFACTOR
**Investigated**: 2026-03-03

### Assessment

| Metric     | Value  | Reasoning                                                                                     |
| ---------- | ------ | --------------------------------------------------------------------------------------------- |
| Priority   | LOW    | Naming convention violation only — no behavior change, no user-facing impact, not blocking     |
| Complexity | MEDIUM | 4 types across 3 crates, ~15 files total, plus CLAUDE.md and AGENTS.md documentation updates  |
| Confidence | HIGH   | Pure rename — all usages enumerated, no serde/string-literal/trait-bound complications          |

---

## Problem Statement

Four types use the vague `Manager` suffix, violating the Code Naming Contract which requires types to reflect domain role, not implementation detail. The types are `SessionManager`, `PtyManager`, `ProjectManager`, and `TeamManager`.

---

## Analysis

### Change Rationale

The CLAUDE.md Code Naming Contract states: "Name types and modules by domain role, not implementation detail — e.g. `GhForgeBackend`, `GhosttyTerminalBackend` over vague names like `Manager` or `Helper`." These four types predate this convention and need renaming for consistency.

### Rename Map

| Current            | New                  | Crate        | File                            |
| ------------------ | -------------------- | ------------ | ------------------------------- |
| `SessionManager`   | `DaemonSessionStore` | kild-daemon  | `session/manager.rs:21`         |
| `PtyManager`       | `PtyStore`           | kild-daemon  | `pty/manager.rs:110`            |
| `ProjectManager`   | `ProjectRegistry`    | kild-core    | `projects/manager.rs:12`        |
| `TeamManager`      | `TeamStore`          | kild-ui      | `teams/state.rs:12`             |

### Key Properties (Safe Rename)

All four types share properties that make this a safe mechanical rename:

- **No serde**: None are serialized/deserialized — names don't appear in wire formats
- **No string literals**: Names don't appear in log messages or error strings
- **No trait bounds**: None are used as generic type parameters or trait constraints
- **No type aliases**: No aliases or shims exist for any of these types
- **Crate-internal** (3 of 4): `SessionManager`, `PtyManager`, `TeamManager` are all crate-internal
- **Single public API**: Only `ProjectManager` crosses crate boundaries (`kild_core::ProjectManager` → `kild-ui`)

### Affected Files

#### SessionManager → DaemonSessionStore

| File                                           | Lines            | Action | Description                                     |
| ---------------------------------------------- | ---------------- | ------ | ----------------------------------------------- |
| `crates/kild-daemon/src/session/manager.rs`    | 13-28, 30, 473+  | UPDATE | Struct def, impl block, doc comments, tests      |
| `crates/kild-daemon/src/session/mod.rs`        | 4                | UPDATE | Re-export                                        |
| `crates/kild-daemon/src/server/mod.rs`         | 17, 79, 198      | UPDATE | Import, construction, function param             |
| `crates/kild-daemon/src/server/connection.rs`  | 13, 25, 109, 389 | UPDATE | Import, function params, comment                 |

#### PtyManager → PtyStore

| File                                           | Lines            | Action | Description                                     |
| ---------------------------------------------- | ---------------- | ------ | ----------------------------------------------- |
| `crates/kild-daemon/src/pty/manager.rs`        | 13, 110, 114, 281, tests | UPDATE | Doc comment, struct def, impl, Default impl, tests |
| `crates/kild-daemon/src/pty/mod.rs`            | 4                | UPDATE | Re-export                                        |
| `crates/kild-daemon/src/session/manager.rs`    | 8, 15, 23, 37   | UPDATE | Import, doc comment, field type, construction    |

#### ProjectManager → ProjectRegistry

| File                                           | Lines            | Action | Description                                     |
| ---------------------------------------------- | ---------------- | ------ | ----------------------------------------------- |
| `crates/kild-core/src/projects/manager.rs`     | 12, 19, tests    | UPDATE | Struct def, impl block, test constructors        |
| `crates/kild-core/src/projects/types.rs`       | 105              | UPDATE | Doc comment intra-doc link                       |
| `crates/kild-core/src/projects/mod.rs`         | 8                | UPDATE | Re-export                                        |
| `crates/kild-core/src/lib.rs`                  | 54               | UPDATE | Crate-level re-export                            |
| `crates/kild-ui/src/state/app_state/state.rs`  | 2, 29, 58, 410, 467 | UPDATE | Import, field type, constructions              |

#### TeamManager → TeamStore

| File                                           | Lines            | Action | Description                                     |
| ---------------------------------------------- | ---------------- | ------ | ----------------------------------------------- |
| `crates/kild-ui/src/teams/state.rs`            | 12, 21           | UPDATE | Struct def, impl block                           |
| `crates/kild-ui/src/teams/mod.rs`              | 9                | UPDATE | Re-export                                        |
| `crates/kild-ui/src/views/main_view/main_view_def.rs` | 59, 193, 257 | UPDATE | Field type, comment, construction             |
| `crates/kild-ui/src/views/dashboard_view.rs`   | 24               | UPDATE | Function param type                              |
| `crates/kild-ui/src/views/sidebar.rs`          | 30               | UPDATE | Function param type                              |

#### Documentation

| File                                           | Action | Description                                     |
| ---------------------------------------------- | ------ | ----------------------------------------------- |
| `CLAUDE.md`                                    | UPDATE | Lines ~170, ~180, ~181: daemon module descriptions |
| `AGENTS.md`                                    | UPDATE | Lines ~249, ~250: daemon module descriptions     |

---

## Implementation Plan

### Step 1: Rename PtyManager → PtyStore

**File**: `crates/kild-daemon/src/pty/manager.rs`

Replace all occurrences of `PtyManager` with `PtyStore` in:
- Struct definition (line 110)
- Impl block (line 114)
- Default impl (line 281)
- Doc comment at line 13 referencing `SessionManager` (leave that reference — it gets renamed in Step 2)
- All test functions using `PtyManager::new()`

**File**: `crates/kild-daemon/src/pty/mod.rs`
- Update re-export at line 4

**Why**: PtyManager is a leaf dependency — renaming it first avoids intermediate broken states since SessionManager depends on it.

### Step 2: Rename SessionManager → DaemonSessionStore

**File**: `crates/kild-daemon/src/session/manager.rs`
- Struct definition (line 21)
- Impl block (line 30)
- Doc comments (lines 13-19)
- Import of PtyStore (already renamed in Step 1, update the doc comment at line 15)
- All test functions

**File**: `crates/kild-daemon/src/session/mod.rs`
- Update re-export at line 4

**File**: `crates/kild-daemon/src/server/mod.rs`
- Update import (line 17)
- Update construction (line 79)
- Update function param (line 198)

**File**: `crates/kild-daemon/src/server/connection.rs`
- Update import (line 13)
- Update function params (lines 25, 109)
- Update comment (line 389)

**Why**: SessionManager depends on PtyManager (now PtyStore) — rename after PtyStore to keep intermediate states compilable.

### Step 3: Rename ProjectManager → ProjectRegistry

**File**: `crates/kild-core/src/projects/manager.rs`
- Struct definition (line 12)
- Impl block (line 19)
- All test functions

**File**: `crates/kild-core/src/projects/types.rs`
- Update doc comment intra-doc link (line 105)

**File**: `crates/kild-core/src/projects/mod.rs`
- Update re-export (line 8)

**File**: `crates/kild-core/src/lib.rs`
- Update crate-level re-export (line 54)

**File**: `crates/kild-ui/src/state/app_state/state.rs`
- Update import (line 2)
- Update field type (line 29)
- Update all constructions (lines 58, 410, 467)

**Why**: This is the only cross-crate rename (kild-core public API consumed by kild-ui). Update both crates together.

### Step 4: Rename TeamManager → TeamStore

**File**: `crates/kild-ui/src/teams/state.rs`
- Struct definition (line 12)
- Impl block (line 21)

**File**: `crates/kild-ui/src/teams/mod.rs`
- Update re-export (line 9)

**File**: `crates/kild-ui/src/views/main_view/main_view_def.rs`
- Update field type (line 59)
- Update comment (line 193)
- Update construction (line 257)

**File**: `crates/kild-ui/src/views/dashboard_view.rs`
- Update function param type (line 24)

**File**: `crates/kild-ui/src/views/sidebar.rs`
- Update function param type (line 30)

**Why**: Entirely kild-ui internal — no cross-crate impact.

### Step 5: Update Documentation

**File**: `CLAUDE.md`
- Update daemon module description lines (~170, ~180, ~181) to use new names

**File**: `AGENTS.md`
- Update daemon module description lines (~249, ~250) to use new names

**Why**: Documentation must match code to avoid confusion.

---

## Patterns to Follow

Each rename is a mechanical find-and-replace within the affected files. Use `replace_all` for each type name within each file. No behavior changes, no new code, no removed code beyond the name itself.

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                    |
| ------------------------------------------- | ------------------------------------------------------------- |
| Missing a usage causes compile error        | `cargo build --all` catches all misses at compile time        |
| Variable names like `pty_manager` field      | Rename struct field `pty_manager` → `pty_store` in SessionManager for consistency |
| Local variables named `mgr` in tests        | Leave as-is — `mgr` is a generic abbreviation, not tied to the type name |
| Doc comments with old names                 | Grep for all four old names after rename to catch stragglers  |
| AGENTS.md and other .md files               | Grep `*Manager` across all .md files                          |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Post-Rename Verification

```bash
# Verify no straggler references remain (should return 0 matches)
rg "SessionManager|PtyManager|ProjectManager|TeamManager" --type rust
rg "SessionManager|PtyManager|ProjectManager|TeamManager" --glob "*.md"
```

---

## Scope Boundaries

**IN SCOPE:**
- Rename the 4 struct types and all usages
- Rename the `pty_manager` field in SessionManager → `pty_store`
- Update doc comments containing old names
- Update CLAUDE.md and AGENTS.md references

**OUT OF SCOPE (do not touch):**
- File names (`manager.rs`, `state.rs`) — renaming files is a separate concern and the current names are fine
- Module names — `session/`, `pty/`, `projects/`, `teams/` are domain-scoped and correct
- Any behavior, logic, or API changes
- Other `manager.rs` files or Manager-like patterns elsewhere

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-03-03
- **Artifact**: `.claude/PRPs/issues/issue-520.md`
