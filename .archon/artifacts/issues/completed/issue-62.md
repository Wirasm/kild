# Investigation: UX: Silent config fallback hides user errors

**Issue**: #62 (https://github.com/Wirasm/shards/issues/62)
**Type**: BUG
**Investigated**: 2026-01-23T12:00:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | MEDIUM | User's config is silently ignored but functionality still works with defaults; workaround is to manually verify config syntax with `toml` parser |
| Complexity | LOW | 3 call sites need to change pattern from `.unwrap_or_default()` to proper error handling with stderr warning; no architectural changes |
| Confidence | HIGH | Root cause is clearly identified: three `.unwrap_or_default()` calls discard `Err` values; CLAUDE.md explicitly prohibits silent failures |

---

## Problem Statement

When a config file has errors (invalid TOML, validation failure), the CLI silently falls back to default configuration using `.unwrap_or_default()`. Users have no indication their config was ignored, leading to confusion when settings don't apply (e.g., expecting `kiro` but getting `claude`).

---

## Analysis

### Root Cause / Change Rationale

The `load_hierarchy()` function in `shards-core` correctly returns `Err` for parse/validation failures. However, all three call sites in the CLI discard these errors using `.unwrap_or_default()`.

### Evidence Chain

WHY: User's config settings are not applied
↓ BECAUSE: CLI uses `ShardsConfig::default()` instead of user's config
  Evidence: `crates/shards/src/commands.rs:52` - `.unwrap_or_default()` returns default on any error

↓ BECAUSE: `load_hierarchy()` returns `Err` for config parse/validation errors
  Evidence: `crates/shards-core/src/config/loading.rs:46` - `return Err(e)` on parse error
  Evidence: `crates/shards-core/src/config/loading.rs:58` - `validate_config(&config)?` propagates validation errors

↓ ROOT CAUSE: `.unwrap_or_default()` discards the `Err` without logging or notifying user
  Evidence: Three locations use this pattern:
  - `crates/shards/src/commands.rs:52` - `handle_create_command()`
  - `crates/shards/src/commands.rs:382` - `run_health_watch_loop()`
  - `crates/shards-core/src/sessions/handler.rs:371` - `restart_session()`

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `crates/shards/src/commands.rs` | 52, 382 | UPDATE | Replace `.unwrap_or_default()` with proper error handling |
| `crates/shards-core/src/sessions/handler.rs` | 371 | UPDATE | Replace `.unwrap_or_default()` with proper error handling |
| `crates/shards-core/src/config/mod.rs` | 35 | UPDATE | Fix documentation example showing `.unwrap_or_default()` |

### Integration Points

- `crates/shards/src/commands.rs:52` - Called from `handle_create_command()` during `shards create`
- `crates/shards/src/commands.rs:382` - Called in health watch loop during `shards health --watch`
- `crates/shards-core/src/sessions/handler.rs:371` - Called during `shards restart`

### Git History

- **Introduced**: `3f23e66` - "refactor: Restructure project as Cargo workspace (#55)"
- **Pattern continued**: `355c896` - "refactor: Extract agent-specific logic into centralized agents module (#56)"
- **Implication**: Silent fallback was inherited from initial implementation, not an intentional design choice

---

## Implementation Plan

### Step 1: Add helper function for config loading with warning

**File**: `crates/shards/src/commands.rs`
**Lines**: After imports (~line 10)
**Action**: UPDATE - Add helper function

**Current code:**
```rust
use shards_core::config::ShardsConfig;
```

**Required change:**
```rust
use shards_core::config::ShardsConfig;

/// Load configuration with warning on errors.
/// Falls back to defaults if config loading fails, but warns the user.
fn load_config_with_warning() -> ShardsConfig {
    match ShardsConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Warning: Could not load config: {}. Using defaults.", e);
            tracing::warn!(
                event = "cli.config_load_failed",
                error = %e,
                "Config load failed, using defaults"
            );
            ShardsConfig::default()
        }
    }
}
```

**Why**: Centralizes the config loading pattern for CLI commands; provides consistent warning behavior.

---

### Step 2: Update create command to use helper

**File**: `crates/shards/src/commands.rs`
**Lines**: 52
**Action**: UPDATE

**Current code:**
```rust
    // Load config hierarchy
    let mut config = ShardsConfig::load_hierarchy().unwrap_or_default();
```

**Required change:**
```rust
    // Load config hierarchy (warns user on errors)
    let mut config = load_config_with_warning();
```

**Why**: Uses the new helper that warns users when config has errors.

---

### Step 3: Update health watch loop to use helper

**File**: `crates/shards/src/commands.rs`
**Lines**: 382
**Action**: UPDATE

**Current code:**
```rust
    let config = ShardsConfig::load_hierarchy().unwrap_or_default();
```

**Required change:**
```rust
    let config = load_config_with_warning();
```

**Why**: Same pattern for consistency across all CLI commands.

---

### Step 4: Update session restart handler

**File**: `crates/shards-core/src/sessions/handler.rs`
**Lines**: 371
**Action**: UPDATE

**Current code:**
```rust
    let shards_config = ShardsConfig::load_hierarchy().unwrap_or_default();
```

**Required change:**
```rust
    let shards_config = match ShardsConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            warn!(
                event = "core.session.config_load_failed",
                error = %e,
                session_id = %session.id,
                "Config load failed during restart, using defaults"
            );
            ShardsConfig::default()
        }
    };
```

**Why**: Handler is in shards-core, cannot use CLI helper; uses structured logging consistent with other handlers.

---

### Step 5: Update documentation example

**File**: `crates/shards-core/src/config/mod.rs`
**Lines**: 35
**Action**: UPDATE

**Current code:**
```rust
/// let config = ShardsConfig::load_hierarchy().unwrap_or_default();
```

**Required change:**
```rust
/// let config = ShardsConfig::load_hierarchy()?;
```

**Why**: Documentation should show proper error handling, not silent fallback pattern.

---

### Step 6: Add/Update Tests

**File**: `crates/shards/src/commands.rs`
**Action**: UPDATE - Add unit test

**Test cases to add:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_with_warning_valid_config() {
        // When config loads successfully, should return the config
        let config = load_config_with_warning();
        // Should not panic and return a valid config
        assert!(!config.agent.default.is_empty());
    }
}
```

**Why**: Ensures the helper function works correctly for the happy path.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/shards/src/commands.rs:98
// Pattern for error display to user via stderr
eprintln!("Failed to create shard: {}", e);
```

```rust
// SOURCE: crates/shards-core/src/sessions/handler.rs:382-388
// Pattern for warning with structured logging
warn!(
    event = "core.session.agent_not_available",
    agent = %agent,
    session_id = %session.id,
    "Agent CLI '{}' not found in PATH - session may fail to start",
    agent
);
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Multiple config errors (user + project) | First error wins; warn once, use defaults |
| Config error message too long for terminal | Error messages from TOML parser are concise; no truncation needed |
| User doesn't see stderr in background | Structured logging captures event for debug; stderr for immediate visibility |
| Breaking change for scripts | Not breaking; fallback behavior unchanged, just adds warning |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Manual Verification

1. Create invalid config: `echo "invalid toml [[[" > ~/.shards/config.toml`
2. Run `shards create test-branch`
3. Verify warning is shown: `Warning: Could not load config: Failed to parse config file...`
4. Verify shard is created with defaults (not blocked)
5. Clean up: `rm ~/.shards/config.toml`

---

## Scope Boundaries

**IN SCOPE:**
- Add stderr warning when config load fails
- Update 3 call sites using `.unwrap_or_default()`
- Update documentation example
- Add unit test for helper function

**OUT OF SCOPE (do not touch):**
- Adding `shards config check` command (future enhancement)
- Adding `shards config show` command (future enhancement)
- Strict mode with `--ignore-config` flag (future enhancement)
- Changes to config loading logic in shards-core

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-23T12:00:00Z
- **Artifact**: `.archon/artifacts/issues/issue-62.md`
