# Investigation: PID file retry logic never runs - serde defaults to 0

**Issue**: #67 (https://github.com/Wirasm/shards/issues/67)
**Type**: BUG
**Investigated**: 2026-01-23T14:10:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | HIGH | Process tracking completely broken when config fields omitted; sessions appear crashed when actually running; no error logged to alert users |
| Complexity | LOW | Fix requires changes to 2 files (types.rs, defaults.rs) with minimal risk; clear pattern already exists in codebase |
| Confidence | HIGH | Root cause is definitively identified in code; behavior is reproducible via TOML deserialization with missing fields |

---

## Problem Statement

When users configure `[terminal]` with only `preferred` set (e.g., `preferred = "ghostty"`), the `spawn_delay_ms` and `max_retry_attempts` fields default to 0 instead of the documented defaults (1000ms and 5 attempts). This causes the retry loop in `read_pid_file_with_retry()` to never execute (`1..=0` is an empty range), resulting in PID files never being read even though they exist after ~500ms.

---

## Analysis

### Root Cause / Change Rationale

**WHY 1**: Why do sessions show "No PID tracked" and "Crashed" when processes are running?
- Because `read_pid_file_with_retry()` returns `Ok(None)` immediately without attempting to read the PID file.
- Evidence: `handler.rs:235` - `match read_pid_file_with_retry(pid_file, max_attempts, initial_delay)`

**WHY 2**: Why does the retry function return without attempting?
- Because the loop `for attempt in 1..=max_attempts` never executes when `max_attempts` is 0.
- Evidence: `pid_file.rs:53` - `for attempt in 1..=max_attempts {`

**WHY 3**: Why is `max_attempts` set to 0?
- Because the config value comes from `config.terminal.max_retry_attempts` which is deserialized to 0.
- Evidence: `handler.rs:232-233`:
```rust
let max_attempts = config.terminal.max_retry_attempts;
let initial_delay = config.terminal.spawn_delay_ms;
```

**WHY 4**: Why are these fields deserialized to 0?
- Because they use `#[serde(default)]` which calls `u32::default()` and `u64::default()`, both returning 0.
- Evidence: `types.rs:134,139`:
```rust
#[serde(default)]  // calls u64::default() = 0
pub spawn_delay_ms: u64,

#[serde(default)]  // calls u32::default() = 0
pub max_retry_attempts: u32,
```

**ROOT CAUSE**: The `#[serde(default)]` attribute on `spawn_delay_ms` and `max_retry_attempts` in `TerminalConfig` uses Rust's `Default` trait for the primitive types, which returns 0, not the intended application defaults of 1000ms and 5 attempts.

### Evidence Chain

```
WHY: Sessions show "Crashed" when process is actually running
↓ BECAUSE: PID file is never read (Ok(None) returned)
  Evidence: `handler.rs:235` - retry function called with 0 attempts

↓ BECAUSE: Retry loop condition 1..=0 is empty range
  Evidence: `pid_file.rs:53` - `for attempt in 1..=max_attempts {`

↓ BECAUSE: max_retry_attempts is 0 from config
  Evidence: `handler.rs:232` - `let max_attempts = config.terminal.max_retry_attempts;`

↓ ROOT CAUSE: serde(default) uses u32::default() = 0, not intended default of 5
  Evidence: `types.rs:139-140`:
  ```rust
  #[serde(default)]
  pub max_retry_attempts: u32,
  ```
```

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `crates/shards-core/src/config/types.rs` | 134, 139 | UPDATE | Change `#[serde(default)]` to use custom default functions |
| `crates/shards-core/src/config/defaults.rs` | 14-15 | UPDATE | Add two new default functions for the fields |
| `crates/shards-core/src/config/defaults.rs` | tests | UPDATE | Add test for TOML deserialization with missing fields |

### Integration Points

- `handler.rs:20-21` - `find_agent_process_with_retry()` uses these config values
- `handler.rs:232-233` - `read_pid_from_file_with_validation()` uses these config values
- `loading.rs:107-108` - `merge_configs()` copies these values (secondary issue: no fallback logic)

### Git History

- **Introduced**: c8a52f60 - 2026-01-22 - "refactor: Split config module into focused submodules (#58)"
- **Implication**: Regression introduced during config module refactoring; the incorrect serde annotation was present in the new types.rs

---

## Implementation Plan

### Step 1: Add custom default functions

**File**: `crates/shards-core/src/config/defaults.rs`
**Lines**: 14-15 (after `default_agent` function)
**Action**: UPDATE

**Current code:**
```rust
pub fn default_agent() -> String {
    agents::default_agent_name().to_string()
}
```

**Required change:**
```rust
pub fn default_agent() -> String {
    agents::default_agent_name().to_string()
}

/// Returns the default spawn delay in milliseconds (1000ms).
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_spawn_delay_ms() -> u64 {
    1000
}

/// Returns the default max retry attempts (5).
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_max_retry_attempts() -> u32 {
    5
}
```

**Why**: These functions provide named defaults that serde can call during deserialization when fields are missing from TOML.

---

### Step 2: Update serde attributes to use custom defaults

**File**: `crates/shards-core/src/config/types.rs`
**Lines**: 134, 139
**Action**: UPDATE

**Current code:**
```rust
    /// Delay in milliseconds after spawning a terminal.
    /// Default: 1000ms.
    #[serde(default)]
    pub spawn_delay_ms: u64,

    /// Maximum retry attempts for terminal spawn.
    /// Default: 5.
    #[serde(default)]
    pub max_retry_attempts: u32,
```

**Required change:**
```rust
    /// Delay in milliseconds after spawning a terminal.
    /// Default: 1000ms.
    #[serde(default = "super::defaults::default_spawn_delay_ms")]
    pub spawn_delay_ms: u64,

    /// Maximum retry attempts for terminal spawn.
    /// Default: 5.
    #[serde(default = "super::defaults::default_max_retry_attempts")]
    pub max_retry_attempts: u32,
```

**Why**: This tells serde to call the custom default functions instead of using `u64::default()` and `u32::default()`.

---

### Step 3: Add test for TOML deserialization defaults

**File**: `crates/shards-core/src/config/defaults.rs`
**Lines**: After `test_terminal_config_default` test (~line 174)
**Action**: UPDATE

**Test case to add:**
```rust
#[test]
fn test_terminal_config_serde_defaults() {
    // Test that TOML deserialization with missing fields uses correct defaults
    let toml_str = r#"
[terminal]
preferred = "ghostty"
"#;
    let config: ShardsConfig = toml::from_str(toml_str).unwrap();

    // These should be the documented defaults, NOT 0
    assert_eq!(config.terminal.spawn_delay_ms, 1000,
        "spawn_delay_ms should default to 1000, not 0");
    assert_eq!(config.terminal.max_retry_attempts, 5,
        "max_retry_attempts should default to 5, not 0");
    assert_eq!(config.terminal.preferred, Some("ghostty".to_string()));
}

#[test]
fn test_terminal_config_empty_section_serde_defaults() {
    // Test with completely empty terminal section
    let toml_str = r#"
[agent]
default = "claude"
"#;
    let config: ShardsConfig = toml::from_str(toml_str).unwrap();

    assert_eq!(config.terminal.spawn_delay_ms, 1000);
    assert_eq!(config.terminal.max_retry_attempts, 5);
}
```

**Why**: The existing test `test_terminal_config_default` only tests the `Default` trait implementation, not TOML deserialization behavior. These new tests specifically verify that serde deserialization uses the correct defaults.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/shards-core/src/config/types.rs:110
// Pattern for custom serde default function reference
#[serde(default = "super::defaults::default_agent")]
pub default: String,
```

```rust
// SOURCE: crates/shards-core/src/config/defaults.rs:13-15
// Pattern for default function signature and documentation
/// Returns the default agent name.
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_agent() -> String {
    agents::default_agent_name().to_string()
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Users explicitly set 0 in config | Zero is still a valid value when explicitly set; the fix only affects missing fields |
| Config merge behavior | `merge_configs()` currently always takes override value; this is a separate issue but doesn't block this fix |
| Existing tests break | The fix aligns behavior with documentation; if tests break, they were testing incorrect behavior |

---

## Validation

### Automated Checks

```bash
cargo check -p shards-core
cargo test -p shards-core config
cargo clippy -p shards-core
```

### Manual Verification

1. Create config with only `[terminal]` and `preferred = "ghostty"`
2. Run `shards start` and verify PID file is read after spawn
3. Run `shards status` and verify health shows "Working" not "Crashed"

---

## Scope Boundaries

**IN SCOPE:**
- Fix `spawn_delay_ms` and `max_retry_attempts` serde defaults in types.rs
- Add custom default functions in defaults.rs
- Add tests verifying TOML deserialization uses correct defaults

**OUT OF SCOPE (do not touch):**
- Config merge logic in loading.rs (separate issue: always takes override even if 0)
- Any changes to pid_file.rs or handler.rs (they correctly use config values)
- Changes to HealthConfig defaults (uses `Option<T>` pattern with accessor methods)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-23T14:10:00Z
- **Artifact**: `.archon/artifacts/issues/issue-67.md`
