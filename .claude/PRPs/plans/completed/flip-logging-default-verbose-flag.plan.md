# Feature: Flip Logging Default - Quiet by Default, --verbose to Enable

## Summary

Change the logging default so structured JSON logs are OFF by default. Replace the `-q/--quiet` flag with `-v/--verbose` flag to enable logs when needed for debugging. This affects both `kild` and `kild-peek` CLIs.

## User Story

As a power user running kild commands
I want clean output by default without JSON logs
So that I can quickly see results without visual noise and only enable verbose logs when debugging

## Problem Statement

Currently, running any kild command produces verbose JSON logs before the actual output. For power users who want speed and clean output, this is friction. Logs are useful for debugging, not everyday use.

## Solution Statement

Flip the logging default: quiet by default, with a new `--verbose/-v` flag to enable logs when needed. This is a simple boolean inversion at the CLI layer - the core logging logic remains unchanged.

## Metadata

| Field            | Value                                          |
| ---------------- | ---------------------------------------------- |
| Type             | ENHANCEMENT                                    |
| Complexity       | LOW                                            |
| Systems Affected | kild CLI, kild-peek CLI, documentation         |
| Dependencies     | None                                           |
| Estimated Tasks  | 8                                              |
| GitHub Issue     | #90                                            |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ kild list                                                                 ║
║   ↓                                                                           ║
║   {"timestamp":"...","level":"INFO","fields":{"event":"core.app.startup...    ║
║   {"timestamp":"...","level":"INFO","fields":{"event":"cli.list_started"...   ║
║   {"timestamp":"...","level":"INFO","fields":{"event":"core.session.list...   ║
║   ... 10+ more JSON log lines ...                                             ║
║   Active shards:                                                              ║
║   ┌────────────┬─────────┬─────────┬...                                       ║
║                                                                               ║
║   USER_FLOW: Run command → see JSON logs → see actual output                  ║
║   PAIN_POINT: Visual noise before useful output                               ║
║   WORKAROUND: Must use `-q` flag every time for clean output                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ kild list                                                                 ║
║   ↓                                                                           ║
║   Active shards:                                                              ║
║   ┌────────────┬─────────┬─────────┬...                                       ║
║                                                                               ║
║   $ kild -v list  (or --verbose)                                              ║
║   ↓                                                                           ║
║   {"timestamp":"...","level":"INFO","fields":{"event":"core.app.startup...    ║
║   {"timestamp":"...","level":"INFO","fields":{"event":"cli.list_started"...   ║
║   Active shards:                                                              ║
║   ┌────────────┬─────────┬─────────┬...                                       ║
║                                                                               ║
║   USER_FLOW: Run command → see clean output immediately                       ║
║   VALUE_ADD: Clean output by default, logs available on demand                ║
║   OPT-IN: Use `-v` or `--verbose` when debugging                              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `kild <cmd>` | JSON logs + output | Clean output only | No visual noise |
| `kild -q <cmd>` | Clean output | ❌ Invalid flag | Must update to new flag |
| `kild -v <cmd>` | ❌ Invalid flag | JSON logs + output | New debugging method |
| `kild-peek <cmd>` | JSON logs + output | Clean output only | Same as kild |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild/src/app.rs` | 8-15 | Current quiet flag definition to REPLACE |
| P0 | `crates/kild/src/main.rs` | 11-13 | Logic to INVERT |
| P0 | `crates/kild/src/app.rs` | 504-600 | Test patterns to MIRROR and RENAME |
| P1 | `crates/kild-peek/src/app.rs` | 12-18 | Parallel implementation to UPDATE |
| P1 | `crates/kild-peek/src/main.rs` | 11-13 | Parallel logic to INVERT |
| P2 | `crates/kild-core/src/logging/mod.rs` | 3-8 | Comment to UPDATE (logic stays same) |

---

## Patterns to Mirror

**FLAG_DEFINITION:**
```rust
// SOURCE: crates/kild/src/app.rs:8-15
// CURRENT (to be replaced):
.arg(
    Arg::new("quiet")
        .short('q')
        .long("quiet")
        .help("Suppress log output, show only essential information")
        .action(ArgAction::SetTrue)
        .global(true),
)
```

**FLAG_EXTRACTION:**
```rust
// SOURCE: crates/kild/src/main.rs:11-13
// CURRENT (to be inverted):
let quiet = matches.get_flag("quiet");
init_logging(quiet);
```

**TEST_STRUCTURE:**
```rust
// SOURCE: crates/kild/src/app.rs:504-511
// CURRENT (to be renamed):
#[test]
fn test_cli_quiet_flag_short() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild", "-q", "list"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(matches.get_flag("quiet"));
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild/src/app.rs` | UPDATE | Replace quiet flag with verbose flag |
| `crates/kild/src/main.rs` | UPDATE | Invert flag logic |
| `crates/kild-peek/src/app.rs` | UPDATE | Replace quiet flag with verbose flag |
| `crates/kild-peek/src/main.rs` | UPDATE | Invert flag logic |
| `crates/kild-core/src/logging/mod.rs` | UPDATE | Update doc comment only |
| `README.md` | UPDATE | Update global flags documentation |
| `CLAUDE.md` | UPDATE | Update all -q references to -v |
| `.claude/skills/kild/SKILL.md` | UPDATE | Update quiet mode references |
| `.claude/skills/kild-peek/SKILL.md` | UPDATE | Update all -q references to -v |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No changes to logging logic** - The `init_logging(quiet: bool)` function signature and behavior stays exactly the same
- **No new log levels** - Not adding debug/trace toggle, just flipping default
- **No config file option** - Verbose setting via CLI flag only, not config.toml
- **No backwards compatibility** - Breaking change, old `-q` flag will error

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `crates/kild/src/app.rs` - Replace quiet flag with verbose flag

- **ACTION**: Replace the quiet flag definition with verbose flag
- **IMPLEMENT**: Change arg name, short, long, and help text
- **FILE**: `crates/kild/src/app.rs` lines 8-15

**Change FROM:**
```rust
.arg(
    Arg::new("quiet")
        .short('q')
        .long("quiet")
        .help("Suppress log output, show only essential information")
        .action(ArgAction::SetTrue)
        .global(true),
)
```

**Change TO:**
```rust
.arg(
    Arg::new("verbose")
        .short('v')
        .long("verbose")
        .help("Enable verbose logging output")
        .action(ArgAction::SetTrue)
        .global(true),
)
```

- **VALIDATE**: `cargo build -p kild`

### Task 2: UPDATE `crates/kild/src/main.rs` - Invert flag logic

- **ACTION**: Change flag extraction and invert the boolean passed to init_logging
- **IMPLEMENT**: Extract verbose flag and pass `!verbose` to init_logging
- **FILE**: `crates/kild/src/main.rs` lines 11-13

**Change FROM:**
```rust
// Extract quiet flag before initializing logging
let quiet = matches.get_flag("quiet");
init_logging(quiet);
```

**Change TO:**
```rust
// Extract verbose flag before initializing logging
// Default (no flag) = quiet mode, -v/--verbose = verbose mode
let verbose = matches.get_flag("verbose");
init_logging(!verbose);
```

- **VALIDATE**: `cargo build -p kild && cargo run -p kild -- list 2>&1 | head -5` (should show clean output)

### Task 3: UPDATE `crates/kild/src/app.rs` - Update all tests

- **ACTION**: Rename and update all quiet flag tests to verbose flag tests
- **IMPLEMENT**: Change test names, flag names in test vectors, assertions
- **FILE**: `crates/kild/src/app.rs` lines 504-600

**Tests to rename and update:**
| Current Test Name | New Test Name |
|-------------------|---------------|
| `test_cli_quiet_flag_short` | `test_cli_verbose_flag_short` |
| `test_cli_quiet_flag_long` | `test_cli_verbose_flag_long` |
| `test_cli_quiet_flag_with_subcommand_args` | `test_cli_verbose_flag_with_subcommand_args` |
| `test_cli_quiet_flag_default_false` | `test_cli_verbose_flag_default_false` |
| `test_cli_quiet_flag_after_subcommand` | `test_cli_verbose_flag_after_subcommand` |
| `test_cli_quiet_flag_after_subcommand_long` | `test_cli_verbose_flag_after_subcommand_long` |
| `test_cli_quiet_flag_after_subcommand_args` | `test_cli_verbose_flag_after_subcommand_args` |
| `test_cli_quiet_flag_with_destroy_force` | `test_cli_verbose_flag_with_destroy_force` |

**Changes per test:**
- Rename function
- Change `-q` to `-v` in test vectors
- Change `--quiet` to `--verbose` in test vectors
- Change `get_flag("quiet")` to `get_flag("verbose")`

- **VALIDATE**: `cargo test -p kild -- test_cli_verbose`

### Task 4: UPDATE `crates/kild-peek/src/app.rs` - Replace quiet flag with verbose flag

- **ACTION**: Replace the quiet flag definition with verbose flag (mirror kild changes)
- **IMPLEMENT**: Change arg name, short, long, and help text
- **FILE**: `crates/kild-peek/src/app.rs` lines 12-18

**Change FROM:**
```rust
.arg(
    Arg::new("quiet")
        .short('q')
        .long("quiet")
        .help("Suppress log output, show only essential information")
        .action(ArgAction::SetTrue)
        .global(true),
)
```

**Change TO:**
```rust
.arg(
    Arg::new("verbose")
        .short('v')
        .long("verbose")
        .help("Enable verbose logging output")
        .action(ArgAction::SetTrue)
        .global(true),
)
```

- **VALIDATE**: `cargo build -p kild-peek`

### Task 5: UPDATE `crates/kild-peek/src/main.rs` - Invert flag logic

- **ACTION**: Change flag extraction and invert the boolean passed to init_logging
- **IMPLEMENT**: Extract verbose flag and pass `!verbose` to init_logging
- **FILE**: `crates/kild-peek/src/main.rs` lines 11-13

**Change FROM:**
```rust
// Extract quiet flag before initializing logging
let quiet = matches.get_flag("quiet");
init_logging(quiet);
```

**Change TO:**
```rust
// Extract verbose flag before initializing logging
// Default (no flag) = quiet mode, -v/--verbose = verbose mode
let verbose = matches.get_flag("verbose");
init_logging(!verbose);
```

- **VALIDATE**: `cargo build -p kild-peek`

### Task 6: UPDATE `crates/kild-peek/src/app.rs` - Update test

- **ACTION**: Rename and update the quiet flag test
- **IMPLEMENT**: Change test name, flag in test vector, assertion
- **FILE**: `crates/kild-peek/src/app.rs` lines 389-396

**Change FROM:**
```rust
#[test]
fn test_cli_quiet_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild-peek", "-q", "list", "windows"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(matches.get_flag("quiet"));
}
```

**Change TO:**
```rust
#[test]
fn test_cli_verbose_flag() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["kild-peek", "-v", "list", "windows"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    assert!(matches.get_flag("verbose"));
}
```

- **VALIDATE**: `cargo test -p kild-peek -- test_cli_verbose`

### Task 7: UPDATE `crates/kild-core/src/logging/mod.rs` - Update doc comment

- **ACTION**: Update the doc comment to reflect new default behavior
- **IMPLEMENT**: Clarify that quiet=true is now the default
- **FILE**: `crates/kild-core/src/logging/mod.rs` lines 3-6

**Change FROM:**
```rust
/// Initialize logging with optional quiet mode.
///
/// When `quiet` is true, only error-level events are emitted.
/// When `quiet` is false, info-level and above events are emitted (default).
```

**Change TO:**
```rust
/// Initialize logging with quiet mode control.
///
/// When `quiet` is true, only error-level events are emitted (default via CLI).
/// When `quiet` is false, info-level and above events are emitted (via -v/--verbose).
```

- **VALIDATE**: `cargo doc -p kild-core --no-deps`

### Task 8: UPDATE documentation files

- **ACTION**: Update all documentation to reflect the new flag
- **IMPLEMENT**: Replace all `-q/--quiet` references with `-v/--verbose`

**Files and changes:**

**README.md** (lines 51-57):
```markdown
### Global flags

```bash
# Enable verbose logging output (shows JSON logs)
kild -v <command>
kild --verbose <command>
```
```

**CLAUDE.md** - Update these lines:
- Line 68: Change `cargo run -p kild -- -q list` to show default behavior example
- Line 95: Change `cargo run -p kild-peek -- -q list windows` to show default behavior
- Lines 147-153: Update Structured Logging section

**New CLAUDE.md lines 147-153:**
```markdown
Logging is initialized via `kild_core::init_logging(quiet)` in the CLI main.rs. Output is JSON format via tracing-subscriber.

By default, only error-level events are emitted (clean output). When `-v/--verbose` flag is used, info-level and above events are emitted.

Control log level with `RUST_LOG` env var: `RUST_LOG=debug cargo run -- list`

Enable verbose logs with the verbose flag: `cargo run -- -v list`
```

**.claude/skills/kild/SKILL.md** - Update:
- Line 12: Change "quiet mode" reference
- Lines 279-298: Update the Quiet Mode section to Verbose Mode

**.claude/skills/kild-peek/SKILL.md** - Update all `-q` to `-v`:
- Replace all occurrences of `-q` with `-v` (approximately 25+ occurrences)
- Update line 248: Change guidance about quiet mode to verbose mode
- Update line 257: Change flag description

- **VALIDATE**: `grep -r '\-q' README.md CLAUDE.md .claude/skills/` should return no matches

---

## Testing Strategy

### Unit Tests to Update

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `crates/kild/src/app.rs` | 8 verbose flag tests | Flag parsing, position, combinations |
| `crates/kild-peek/src/app.rs` | 1 verbose flag test | Flag parsing |

### Manual Verification

- [ ] `kild list` produces clean output (no JSON logs)
- [ ] `kild -v list` shows JSON logs
- [ ] `kild --verbose list` shows JSON logs
- [ ] `-v` works before subcommand: `kild -v create test`
- [ ] `-v` works after subcommand: `kild list -v`
- [ ] `kild-peek list windows` produces clean output
- [ ] `kild-peek -v list windows` shows JSON logs

### Edge Cases Checklist

- [ ] `-v` combined with other flags: `kild -v destroy test --force`
- [ ] `-v` with JSON output flag: `kild -v list --json`
- [ ] Old `-q` flag now errors with helpful message

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test --all
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: BEHAVIOR_VALIDATION

```bash
# Verify default is quiet (no JSON logs on stderr)
cargo run -p kild -- list 2>&1 | grep -c "timestamp" || echo "PASS: No logs by default"

# Verify -v enables logs
cargo run -p kild -- -v list 2>&1 | grep -c "timestamp" && echo "PASS: Logs with -v"

# Same for kild-peek
cargo run -p kild-peek -- list windows 2>&1 | grep -c "timestamp" || echo "PASS: No logs by default"
cargo run -p kild-peek -- -v list windows 2>&1 | grep -c "timestamp" && echo "PASS: Logs with -v"
```

---

## Acceptance Criteria

From GitHub Issue #90:

- [ ] `kild list` produces clean output (no JSON logs)
- [ ] `kild -v list` shows JSON logs
- [ ] `kild --verbose list` shows JSON logs
- [ ] `-v` works in any position (before or after subcommand)
- [ ] All existing tests updated and passing
- [ ] Documentation updated

---

## Completion Checklist

- [ ] Task 1: kild app.rs flag definition updated
- [ ] Task 2: kild main.rs logic inverted
- [ ] Task 3: kild app.rs tests updated (8 tests)
- [ ] Task 4: kild-peek app.rs flag definition updated
- [ ] Task 5: kild-peek main.rs logic inverted
- [ ] Task 6: kild-peek app.rs test updated
- [ ] Task 7: kild-core logging doc comment updated
- [ ] Task 8: Documentation updated (README, CLAUDE.md, SKILL.md files)
- [ ] Level 1: Static analysis passes
- [ ] Level 2: All unit tests pass
- [ ] Level 3: Full build succeeds
- [ ] Level 4: Behavior validation passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking change for users using `-q` | HIGH | LOW | Pre-1.0 software, document in release notes |
| Missed documentation reference | MED | LOW | Grep search for all `-q` and `quiet` references |
| Test failures from renamed tests | LOW | LOW | Consistent rename pattern, run all tests |

---

## Notes

**Breaking Change Notice**: This is a breaking change. Users who have scripts or muscle memory using `-q` will need to update. Since KILD is pre-1.0, this is acceptable and the change better serves the target persona (power users who want clean output by default).

**Implementation Pattern**: The change is a simple boolean inversion. The core `init_logging(quiet: bool)` function signature and logic remains unchanged - only the CLI layer flips the default. This keeps the change minimal and low-risk.

**Dual Crate Pattern**: Both `kild` and `kild-peek` need identical changes since they share the same CLI pattern. This ensures consistent user experience across both tools.
