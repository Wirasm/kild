# Feature: Configurable File Include Patterns for Worktree Creation

## Summary

Add default file include patterns and implement array merging for the `[include_patterns]` config section. Currently, when `include_patterns` is not configured, no files are copied. This change will:
1. Provide sensible default patterns (`.env*`, `*.local.json`, `.claude/**`, `.cursor/**`)
2. Merge user and project config patterns (instead of replacing) so users can set global patterns and projects can add project-specific ones

## User Story

As a developer using KILD for parallel AI workflows
I want commonly needed files (like `.env`, AI context files, and implementation plans) to automatically copy to new worktrees
So that I don't have to manually configure patterns or copy files for every project

## Problem Statement

When creating a kild/worktree:
1. Only committed files are available - uncommitted work-in-progress files (like implementation plans) are missing
2. Gitignored files (like `.env`) are not copied automatically
3. Users must explicitly configure `[include_patterns]` in every project to get file copying
4. If a user sets global patterns and a project sets its own patterns, the project patterns completely replace the user patterns (no merging)

## Solution Statement

1. Add default patterns to `IncludeConfig::default()` so file copying works out-of-box
2. Change `merge_configs()` to merge `include_patterns.patterns` arrays (deduplicated) rather than object-level replacement
3. Ensure the existing file copy infrastructure uses the merged/defaulted config

## Metadata

| Field            | Value                                              |
| ---------------- | -------------------------------------------------- |
| Type             | ENHANCEMENT                                        |
| Complexity       | LOW                                                |
| Systems Affected | config, files                                      |
| Dependencies     | None (all infrastructure exists)                   |
| Estimated Tasks  | 5                                                  |

---

## UX Design

### Before State

```
╔════════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                       ║
╠════════════════════════════════════════════════════════════════════════════════╣
║                                                                                ║
║   ┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐ ║
║   │  kild create    │ ──────► │  Worktree       │ ──────► │   Worktree      │ ║
║   │  my-feature     │         │  Created        │         │   MISSING:      │ ║
║   └─────────────────┘         └─────────────────┘         │   - .env        │ ║
║                                                           │   - plans/*.md  │ ║
║                                                           │   - .claude/**  │ ║
║                                                           └─────────────────┘ ║
║                                                                                ║
║   USER_FLOW:                                                                   ║
║   1. User runs `kild create my-feature`                                        ║
║   2. Worktree created with only committed files                                ║
║   3. User manually copies .env, plans, AI context files                        ║
║   4. User must add [include_patterns] to config to automate                    ║
║                                                                                ║
║   PAIN_POINT:                                                                  ║
║   - No defaults = no automatic file copying                                    ║
║   - Must configure every project                                               ║
║   - Project config replaces user config (no merging)                           ║
║                                                                                ║
╚════════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔════════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                       ║
╠════════════════════════════════════════════════════════════════════════════════╣
║                                                                                ║
║   ┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐ ║
║   │  kild create    │ ──────► │  Worktree       │ ──────► │   Worktree      │ ║
║   │  my-feature     │         │  Created        │         │   INCLUDES:     │ ║
║   └─────────────────┘         │                 │         │   ✓ .env        │ ║
║                               │  + Default      │         │   ✓ .env.local  │ ║
║                               │    Patterns     │         │   ✓ .claude/**  │ ║
║                               │  + User Config  │         │   ✓ .cursor/**  │ ║
║                               │  + Project Cfg  │         │   ✓ *.local.json│ ║
║                               │    (MERGED)     │         └─────────────────┘ ║
║                               └─────────────────┘                              ║
║                                                                                ║
║   USER_FLOW:                                                                   ║
║   1. User runs `kild create my-feature`                                        ║
║   2. Default patterns auto-copy .env, AI context files                         ║
║   3. User patterns (global) add project-type-specific patterns                 ║
║   4. Project patterns add repo-specific patterns                               ║
║   5. All three merged = comprehensive file copying                             ║
║                                                                                ║
║   VALUE_ADD:                                                                   ║
║   - Works out of the box with sensible defaults                                ║
║   - Global user patterns persist across all projects                           ║
║   - Projects extend (not replace) the pattern list                             ║
║                                                                                ║
╚════════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `kild create` | No patterns = no file copy | Default patterns applied | .env, AI context files copied automatically |
| User config `~/.kild/config.toml` | Patterns override by project | Patterns merged with project | Global patterns always apply |
| Project config `.kild/config.toml` | Replaces user patterns entirely | Adds to user + default patterns | Project-specific patterns extend, don't replace |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-core/src/files/types.rs` | 1-71 | `IncludeConfig` struct definition - modify for defaults |
| P0 | `crates/kild-core/src/config/loading.rs` | 89-134 | `merge_configs()` function - modify for array merging |
| P1 | `crates/kild-core/src/config/defaults.rs` | 1-50 | Default implementation pattern to FOLLOW |
| P1 | `crates/kild-core/src/git/handler.rs` | 238-278 | Integration point - verify config flows through |
| P2 | `crates/kild-core/src/files/handler.rs` | 168-271 | Tests to extend for default patterns |
| P2 | `crates/kild-core/src/config/loading.rs` | 360-427 | Test patterns to FOLLOW for merge tests |

---

## Patterns to Mirror

**DEFAULT_IMPLEMENTATION:**
```rust
// SOURCE: crates/kild-core/src/config/defaults.rs:39-46
// COPY THIS PATTERN for Default impl:
impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default: default_agent(),
            startup_command: None,
            flags: None,
        }
    }
}
```

**MERGE_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/config/loading.rs:110-116
// COPY THIS PATTERN for HashMap/array merging:
agents: {
    let mut merged = base.agents;
    for (key, value) in override_config.agents {
        merged.insert(key, value);
    }
    merged
},
```

**SERDE_DEFAULT_FUNCTION:**
```rust
// SOURCE: crates/kild-core/src/files/types.rs:68-70
// COPY THIS PATTERN for serde default functions:
fn default_enabled() -> bool {
    true
}
```

**TEST_MERGE_PATTERN:**
```rust
// SOURCE: crates/kild-core/src/config/loading.rs:361-388
// COPY THIS PATTERN for merge tests:
#[test]
fn test_health_config_merge() {
    let user_config: KildConfig = toml::from_str(
        r#"
[health]
idle_threshold_minutes = 15
"#,
    )
    .unwrap();
    let project_config: KildConfig = toml::from_str(/* ... */).unwrap();
    let merged = merge_configs(user_config, project_config);
    assert_eq!(merged.health.idle_threshold_minutes(), 15);
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-core/src/files/types.rs` | UPDATE | Add explicit `impl Default for IncludeConfig` with default patterns |
| `crates/kild-core/src/config/loading.rs` | UPDATE | Modify `merge_configs()` to merge `include_patterns.patterns` arrays |
| `crates/kild-core/src/config/loading.rs` | UPDATE | Add tests for include_patterns merge behavior |
| `.kild/config.example.toml` | UPDATE | Document the default patterns in example config |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **`exclude` patterns** - Future consideration; only `include` patterns for now
- **`--no-copy-files` CLI flag** - Future consideration; can be added separately
- **Dry-run mode** - Not needed for MVP
- **Per-file override** - Patterns only, no individual file specifications

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `crates/kild-core/src/files/types.rs` - Add default patterns

- **ACTION**: Replace derived `Default` with explicit `impl Default for IncludeConfig`
- **IMPLEMENT**:
  ```rust
  /// Returns the default include patterns.
  /// These patterns provide sensible defaults for common use cases.
  pub fn default_include_patterns() -> Vec<String> {
      vec![
          ".env*".to_string(),
          "*.local.json".to_string(),
          ".claude/**".to_string(),
          ".cursor/**".to_string(),
      ]
  }

  impl Default for IncludeConfig {
      fn default() -> Self {
          Self {
              patterns: default_include_patterns(),
              enabled: true,
              max_file_size: None,
          }
      }
  }
  ```
- **MIRROR**: `crates/kild-core/src/config/defaults.rs:39-46`
- **REMOVE**: The `#[derive(Default)]` from `IncludeConfig` struct
- **GOTCHA**: Keep `#[serde(default)]` attributes on fields so TOML parsing still works for partial configs
- **VALIDATE**: `cargo test -p kild-core -- include` && `cargo clippy --all -- -D warnings`

### Task 2: UPDATE `crates/kild-core/src/config/loading.rs` - Implement array merging

- **ACTION**: Modify `merge_configs()` to merge `include_patterns.patterns` arrays
- **IMPLEMENT**:
  ```rust
  // Replace line 117:
  // include_patterns: override_config.include_patterns.or(base.include_patterns),
  // With:
  include_patterns: {
      match (base.include_patterns, override_config.include_patterns) {
          (None, None) => None,
          (Some(base_cfg), None) => Some(base_cfg),
          (None, Some(override_cfg)) => Some(override_cfg),
          (Some(base_cfg), Some(override_cfg)) => {
              // Merge patterns: combine and deduplicate
              let mut merged_patterns = base_cfg.patterns;
              for pattern in override_cfg.patterns {
                  if !merged_patterns.contains(&pattern) {
                      merged_patterns.push(pattern);
                  }
              }
              Some(IncludeConfig {
                  patterns: merged_patterns,
                  // Override config wins for enabled/max_file_size
                  enabled: override_cfg.enabled,
                  max_file_size: override_cfg.max_file_size.or(base_cfg.max_file_size),
              })
          }
      }
  },
  ```
- **MIRROR**: `crates/kild-core/src/config/loading.rs:110-116` (agents HashMap merge pattern)
- **IMPORTS**: Add `use crate::files::types::IncludeConfig;` if not present
- **GOTCHA**: Preserve order - base patterns first, then override patterns appended
- **VALIDATE**: `cargo build -p kild-core` && `cargo clippy --all -- -D warnings`

### Task 3: UPDATE `crates/kild-core/src/config/loading.rs` - Add merge tests

- **ACTION**: Add tests for `include_patterns` merge behavior
- **IMPLEMENT**: Add tests in the `#[cfg(test)]` module:
  ```rust
  #[test]
  fn test_include_patterns_merge_combines_arrays() {
      let user_config: KildConfig = toml::from_str(
          r#"
  [include_patterns]
  patterns = [".env*", "user-specific/**"]
  "#,
      )
      .unwrap();

      let project_config: KildConfig = toml::from_str(
          r#"
  [include_patterns]
  patterns = [".env*", "project-specific/**"]
  "#,
      )
      .unwrap();

      let merged = merge_configs(user_config, project_config);
      let patterns = &merged.include_patterns.unwrap().patterns;

      // Base patterns come first, then override patterns (deduplicated)
      assert_eq!(patterns.len(), 3);
      assert!(patterns.contains(&".env*".to_string()));
      assert!(patterns.contains(&"user-specific/**".to_string()));
      assert!(patterns.contains(&"project-specific/**".to_string()));
  }

  #[test]
  fn test_include_patterns_merge_override_wins_for_enabled() {
      let user_config: KildConfig = toml::from_str(
          r#"
  [include_patterns]
  enabled = true
  patterns = [".env*"]
  "#,
      )
      .unwrap();

      let project_config: KildConfig = toml::from_str(
          r#"
  [include_patterns]
  enabled = false
  patterns = []
  "#,
      )
      .unwrap();

      let merged = merge_configs(user_config, project_config);
      let include = merged.include_patterns.unwrap();

      assert!(!include.enabled); // Project disabled wins
      assert!(include.patterns.contains(&".env*".to_string())); // But patterns still merged
  }

  #[test]
  fn test_include_patterns_default_has_patterns() {
      let config = KildConfig::default();
      let include = config.include_patterns.unwrap_or_default();

      assert!(include.enabled);
      assert!(!include.patterns.is_empty());
      assert!(include.patterns.contains(&".env*".to_string()));
      assert!(include.patterns.contains(&".claude/**".to_string()));
  }
  ```
- **MIRROR**: `crates/kild-core/src/config/loading.rs:361-388`
- **VALIDATE**: `cargo test -p kild-core -- include_patterns`

### Task 4: UPDATE `crates/kild-core/src/config/types.rs` - Set default for include_patterns

- **ACTION**: Change `include_patterns` field to use `IncludeConfig::default()` instead of `None`
- **IMPLEMENT**: In `KildConfig` default impl or via serde default:
  ```rust
  // Option 1: If KildConfig has explicit Default impl, set:
  include_patterns: Some(IncludeConfig::default()),

  // Option 2: If using derive(Default), add serde attribute:
  #[serde(default = "default_include_config")]
  pub include_patterns: Option<IncludeConfig>,

  // With helper function:
  fn default_include_config() -> Option<IncludeConfig> {
      Some(IncludeConfig::default())
  }
  ```
- **GOTCHA**: `KildConfig` uses `#[derive(Default)]` so need serde default function approach
- **LOCATION**: `crates/kild-core/src/config/types.rs:68-70` - add function near struct
- **VALIDATE**: `cargo test -p kild-core` && `cargo build --all`

### Task 5: UPDATE `.kild/config.example.toml` - Document defaults

- **ACTION**: Update the `[include_patterns]` section to document default patterns
- **IMPLEMENT**: Update lines 95-109:
  ```toml
  [include_patterns]
  # Enable/disable pattern-based file copying
  # Default: true
  enabled = true

  # Glob patterns for files to copy (relative to repo root)
  # DEFAULT PATTERNS (applied automatically):
  #   - ".env*"           - Environment files
  #   - "*.local.json"    - Local config files
  #   - ".claude/**"      - Claude AI context files
  #   - ".cursor/**"      - Cursor AI context files
  #
  # Your patterns EXTEND the defaults (not replace).
  # User config (~/.kild/config.toml) patterns are also merged.
  patterns = [
      "build/artifacts/**",  # Project-specific: build artifacts
  ]

  # Maximum file size to copy (skip larger files with warning)
  # Supports: "10MB", "1GB", etc.
  # max_file_size = "10MB"
  ```
- **VALIDATE**: `cargo build --all` (config example is just documentation)

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `config/loading.rs` | `test_include_patterns_merge_combines_arrays` | Array merging works |
| `config/loading.rs` | `test_include_patterns_merge_override_wins_for_enabled` | enabled/max_file_size override behavior |
| `config/loading.rs` | `test_include_patterns_default_has_patterns` | Defaults are applied |

### Edge Cases Checklist

- [x] Empty patterns array in project config (should still merge user patterns)
- [x] Both configs have same pattern (should deduplicate)
- [x] User config has patterns, project config is None (user patterns preserved)
- [x] Project config disables enabled=false (should still merge patterns but disable)
- [x] No config at all (should use defaults)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p kild-core -- include
cargo test -p kild-core -- merge
```

**EXPECT**: All tests pass including new merge tests

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: MANUAL_VALIDATION

1. Create a test project with `.env` file (gitignored)
2. Run `kild create test-defaults` WITHOUT any config
3. Verify `.env` is copied to worktree
4. Add user config with custom patterns
5. Add project config with different patterns
6. Run `kild create test-merge`
7. Verify ALL patterns (defaults + user + project) apply

---

## Acceptance Criteria

- [x] `IncludeConfig::default()` returns sensible default patterns
- [x] `merge_configs()` merges `include_patterns.patterns` arrays (deduplicated)
- [x] Project config `enabled` and `max_file_size` override user config
- [x] Default patterns work without any user configuration
- [x] All tests pass
- [x] Documentation updated in example config

---

## Completion Checklist

- [ ] Task 1: Default patterns added to IncludeConfig
- [ ] Task 2: merge_configs updated for array merging
- [ ] Task 3: Merge tests added and passing
- [ ] Task 4: KildConfig default includes IncludeConfig
- [ ] Task 5: Example config documentation updated
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full suite passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Default patterns copy unwanted files | LOW | LOW | Defaults are conservative (common files only); users can disable with enabled=false |
| Breaking change for users who rely on replacement behavior | LOW | MED | Document in CHANGELOG; merge is more intuitive than replace |

---

## Notes

**Design Decisions:**

1. **Merge vs Replace**: Chose merge because it's more intuitive - users expect project config to extend, not replace. This matches how most config systems work (CSS, webpack, etc.).

2. **Default patterns chosen**:
   - `.env*` - Environment files (most common need)
   - `*.local.json` - Local config files (common pattern)
   - `.claude/**` - Claude Code context (KILD's target users)
   - `.cursor/**` - Cursor AI context (similar user base)

3. **Not including** build artifacts in defaults - too project-specific; users can add via config.

4. **Deduplication**: Simple contains check; order preserved (base first, then override).

**Future Considerations:**
- Add `exclude` patterns for fine-grained control
- Add `--no-copy-files` CLI flag for one-off skipping
- Consider dry-run mode to preview what would be copied
