# Investigation: Fix CLI flag parsing to accept flags without equals sign

**Issue**: #12 (https://github.com/Wirasm/shards/issues/12)
**Type**: BUG
**Investigated**: 2026-01-15T12:43:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | LOW | Minor UX issue with simple workaround (use equals sign), doesn't break functionality |
| Complexity | LOW | Single-line fix in one file, no integration points affected |
| Confidence | HIGH | Root cause clearly identified in clap documentation, tested and confirmed behavior |

---

## Problem Statement

The `--flags` argument requires equals sign syntax (`--flags='value'`) instead of accepting space-separated values (`--flags 'value'`). This creates an unintuitive user experience where `shards create my-branch --agent kiro --flags '--trust-all-tools'` fails with "unexpected argument" error.

---

## Analysis

### Root Cause / Change Rationale

WHY: `shards create my-branch --agent kiro --flags '--trust-all-tools'` fails with "unexpected argument '--trust-all-tools' found"
↓ BECAUSE: Clap interprets `'--trust-all-tools'` as a separate flag argument, not as a value for `--flags`
  Evidence: Test output shows `error: unexpected argument '--trust-all-tools' found`

↓ BECAUSE: The `--flags` argument definition doesn't specify that it takes a value
  Evidence: `src/cli/app.rs:38-41` - Missing `.num_args(1)` or `.action(ArgAction::Set)`

↓ ROOT CAUSE: Clap requires explicit configuration to know an argument takes a value
  Evidence: Clap documentation states "implicitly sets `Arg::action(ArgAction::Set)`" when using `num_args(1)`

### Evidence Chain

WHY: User gets "unexpected argument" error
↓ BECAUSE: Clap treats the value as a separate argument
  Evidence: Error message: `error: unexpected argument '--trust-all-tools' found`

↓ BECAUSE: `--flags` argument lacks value configuration
  Evidence: `src/cli/app.rs:38-41`:
```rust
.arg(
    Arg::new("flags")
        .long("flags")
        .help("Additional flags for agent (overrides config)")
)
```

↓ ROOT CAUSE: Missing `.num_args(1)` to tell clap this argument takes a value
  Evidence: Clap docs: "Specifies the number of arguments parsed per occurrence... Users may specify values for arguments in any of the following methods: Using a space such as `-o value` or `--option value`"

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/cli/app.rs` | 38-41 | UPDATE | Add `.num_args(1)` to flags argument |
| `src/cli/app.rs` | 90-140 | UPDATE | Add test for space-separated flags syntax |

### Integration Points

- `src/cli/commands.rs:43` reads the flags value with `get_one::<String>("flags")`
- No changes needed in commands.rs - it already handles the value correctly
- Other arguments (`agent`, `terminal`, `startup-command`) work correctly without explicit `num_args` because they use `.value_parser()` which implies taking a value

### Git History

- **Introduced**: 02c6cc45 - 2026-01-12 - "feat: implement hierarchical TOML configuration system"
- **Last modified**: 02c6cc45 - 2026-01-12
- **Implication**: Recently added feature, not a regression

---

## Implementation Plan

### Step 1: Add num_args to flags argument

**File**: `src/cli/app.rs`
**Lines**: 38-41
**Action**: UPDATE

**Current code:**
```rust
.arg(
    Arg::new("flags")
        .long("flags")
        .help("Additional flags for agent (overrides config)")
)
```

**Required change:**
```rust
.arg(
    Arg::new("flags")
        .long("flags")
        .num_args(1)
        .help("Additional flags for agent (overrides config)")
)
```

**Why**: Adding `.num_args(1)` tells clap that this argument expects exactly one value, allowing both space-separated and equals-sign syntax.

---

### Step 2: Add test for space-separated flags syntax

**File**: `src/cli/app.rs`
**Action**: UPDATE (add new test)

**Test case to add:**
```rust
#[test]
fn test_cli_create_with_flags_space_separated() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "shards",
        "create",
        "test-branch",
        "--agent",
        "kiro",
        "--flags",
        "--trust-all-tools",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert_eq!(
        create_matches.get_one::<String>("flags").unwrap(),
        "--trust-all-tools"
    );
}

#[test]
fn test_cli_create_with_flags_equals_syntax() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec![
        "shards",
        "create",
        "test-branch",
        "--agent",
        "kiro",
        "--flags=--trust-all-tools",
    ]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let create_matches = matches.subcommand_matches("create").unwrap();
    assert_eq!(
        create_matches.get_one::<String>("flags").unwrap(),
        "--trust-all-tools"
    );
}
```

**Why**: Ensures both syntaxes work correctly and prevents regression.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/cli/app.rs:24-28
// Pattern for arguments that take values with num_args
.arg(
    Arg::new("agent")
        .long("agent")
        .short('a')
        .help("AI agent to launch (overrides config)")
        .value_parser(["claude", "kiro", "gemini", "codex", "aether"])
)
```

Note: The `agent` argument works without explicit `num_args(1)` because `.value_parser()` implies it takes a value. Since `flags` doesn't use `value_parser`, it needs explicit `num_args(1)`.

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Flags value contains spaces | User must quote the value: `--flags '--trust-all-tools --verbose'` |
| Multiple flag values | Current design supports single string only, which is correct |
| Backwards compatibility | Equals syntax still works, so no breaking change |

---

## Validation

### Automated Checks

```bash
cargo test test_cli_create_with_flags_space_separated
cargo test test_cli_create_with_flags_equals_syntax
cargo test  # Run all tests
cargo clippy
```

### Manual Verification

1. Test space-separated syntax:
   ```bash
   cargo build
   ./target/debug/shards create test-branch --agent kiro --flags '--trust-all-tools'
   ```
   Expected: No error, session created successfully

2. Test equals syntax (should still work):
   ```bash
   ./target/debug/shards create test-branch2 --agent kiro --flags='--trust-all-tools'
   ```
   Expected: No error, session created successfully

3. Test with complex flags:
   ```bash
   ./target/debug/shards create test-branch3 --agent kiro --flags '--trust-all-tools --verbose'
   ```
   Expected: No error, both flags passed to agent

---

## Scope Boundaries

**IN SCOPE:**
- Fix `--flags` argument to accept space-separated values
- Add tests for both syntaxes
- Ensure backwards compatibility with equals syntax

**OUT OF SCOPE (do not touch):**
- Other CLI arguments (they work correctly)
- Flag parsing logic in commands.rs (already correct)
- Configuration system (unrelated)
- Multiple flag values support (not requested)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-15T12:43:00Z
- **Artifact**: `.archon/artifacts/issues/issue-12.md`
