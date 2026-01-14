# Feature: Include Patterns to Override Git Ignore

## Summary

Add configurable include patterns to Shards CLI that override Git ignore rules when creating new shards. This allows copying specific ignored files (like .env, build artifacts, config files) to new worktrees based on glob patterns defined in the Shards configuration file.

## User Story

As a developer using Shards for parallel development
I want to specify include patterns in my config that override Git ignore
So that important ignored files (like .env, local configs, build artifacts) are copied to new shards automatically

## Problem Statement

Currently, when creating new shards, only Git-tracked files are copied to the new worktree. Important development files that are typically ignored (environment files, local configurations, build artifacts) are not available in the new shard, requiring manual copying or recreation.

## Solution Statement

Implement a configuration-driven file copying system that uses glob patterns to identify ignored files that should be copied to new shards, with integration into the existing worktree creation workflow and comprehensive error handling.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | MEDIUM                                            |
| Systems Affected | core/config, git/handler, sessions/handler, files (new) |
| Dependencies     | ignore = "0.4", glob = "0.3", walkdir = "2"      |
| Estimated Tasks  | 9                                                 |

---

## UX Design

### Before State
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Create    │ ──────► │ Git Worktree│ ──────► │ Only Tracked│            ║
║   │   Shard     │         │ Creation    │         │ Files Copied│            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW: shards create → worktree created → only tracked files present   ║
║   PAIN_POINT: .env, configs, build artifacts missing in new shard            ║
║   DATA_FLOW: Git tracks files → Git worktree → Tracked files only            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Create    │ ──────► │ Git Worktree│ ──────► │ Tracked +   │            ║
║   │   Shard     │         │ Creation    │         │ Include Files│            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                   │                                           ║
║                                   ▼                                           ║
║                          ┌─────────────┐                                      ║
║                          │PATTERN_MATCH│  ◄── Config include patterns        ║
║                          │& FILE_COPY  │                                      ║
║                          └─────────────┘                                      ║
║                                                                               ║
║   USER_FLOW: shards create → worktree + pattern matching → all needed files  ║
║   VALUE_ADD: .env, configs, artifacts automatically available in new shard   ║
║   DATA_FLOW: Config patterns → File matching → Selective copying → Complete env│
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes
| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `shards create` | Only tracked files | Tracked + include patterns | Complete dev environment ready |
| Config file | No file copying options | Include patterns section | Control over copied files |
| New worktree | Missing .env, configs | Has all needed files | No manual file copying needed |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/core/config.rs` | 1-400 | Config loading/merging pattern to EXTEND |
| P0 | `src/git/handler.rs` | 50-120 | Worktree creation flow to INTEGRATE with |
| P0 | `src/sessions/handler.rs` | 20-80 | Session creation workflow to EXTEND |
| P1 | `src/sessions/operations.rs` | 50-120 | File operations patterns to MIRROR |
| P1 | `src/core/errors.rs` | 1-80 | Error handling pattern to FOLLOW |
| P2 | `src/sessions/types.rs` | 1-50 | Type definition patterns to MIRROR |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [ignore crate v0.4](https://docs.rs/ignore/0.4/ignore/) | WalkBuilder, overrides | Pattern matching and gitignore handling |
| [glob crate v0.3](https://docs.rs/glob/0.3/glob/) | Pattern syntax | Glob pattern validation |
| [walkdir crate v2](https://docs.rs/walkdir/2/walkdir/) | Directory traversal | File system walking |

---

## Patterns to Mirror

**CONFIG_EXTENSION:**
```rust
// SOURCE: src/core/config.rs:20-40
// COPY THIS PATTERN:
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShardsConfig {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,
    // ADD: include patterns here
}

// Config loading pattern
pub fn load_hierarchy() -> Result<Self, Box<dyn std::error::Error>> {
    let mut config = ShardsConfig::default();
    
    if let Ok(user_config) = Self::load_user_config() {
        config = Self::merge_configs(config, user_config);
    }
    
    if let Ok(project_config) = Self::load_project_config() {
        config = Self::merge_configs(config, project_config);
    }
    
    config.validate()?;
    Ok(config)
}
```

**ATOMIC_FILE_OPERATIONS:**
```rust
// SOURCE: src/sessions/operations.rs:80-110
// COPY THIS PATTERN:
pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session.id.replace('/', "_")));
    let session_json = serde_json::to_string_pretty(session)?;
    
    // Write atomically by writing to temp file first, then renaming
    let temp_file = session_file.with_extension("json.tmp");
    
    if let Err(e) = fs::write(&temp_file, session_json) {
        let _ = fs::remove_file(&temp_file);
        return Err(SessionError::IoError { source: e });
    }
    
    if let Err(e) = fs::rename(&temp_file, &session_file) {
        let _ = fs::remove_file(&temp_file);
        return Err(SessionError::IoError { source: e });
    }
    
    Ok(())
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/core/errors.rs:10-30
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config file not found at '{path}'")]
    ConfigNotFound { path: String },
    
    #[error("Invalid agent '{agent}'. Supported agents: claude, kiro, gemini, codex, aether")]
    InvalidAgent { agent: String },
    
    #[error("IO error reading config: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl ShardsError for ConfigError {
    fn error_code(&self) -> &'static str {
        match self {
            ConfigError::ConfigNotFound { .. } => "CONFIG_NOT_FOUND",
            ConfigError::InvalidAgent { .. } => "INVALID_AGENT",
            ConfigError::IoError { .. } => "IO_ERROR",
        }
    }
}
```

**STRUCTURED_LOGGING:**
```rust
// SOURCE: src/git/handler.rs:25-35
// COPY THIS PATTERN:
info!(
    event = "git.worktree.create_started",
    project_id = project.id,
    branch = validated_branch,
    repo_path = %project.path.display()
);

warn!(
    event = "session.load_invalid_structure",
    file = %path.display(),
    worktree_path = %session.worktree_path.display(),
    validation_error = validation_error,
    message = "Session file has invalid structure, skipping"
);
```

**INTEGRATION_HOOK:**
```rust
// SOURCE: src/git/handler.rs:75-85
// COPY THIS PATTERN:
pub fn create_worktree(
    base_dir: &Path,
    project: &ProjectInfo,
    branch: &str,
) -> Result<WorktreeInfo, GitError> {
    // ... existing worktree creation logic ...
    
    // CREATE worktree
    repo.worktree(&worktree_name, &worktree_path, None)?;
    
    // ADD: File copying integration point here
    
    Ok(WorktreeInfo::new(worktree_path.clone(), validated_branch.clone(), project.id.clone()))
}
```

---

## Files to Change

| File                             | Action | Justification                            |
| -------------------------------- | ------ | ---------------------------------------- |
| `Cargo.toml`                     | UPDATE | Add ignore, glob, walkdir dependencies   |
| `src/core/config.rs`             | UPDATE | Add include patterns configuration       |
| `src/files/mod.rs`               | CREATE | New files module for pattern matching   |
| `src/files/types.rs`             | CREATE | Include pattern types and config         |
| `src/files/operations.rs`        | CREATE | Pattern matching and file copying logic  |
| `src/files/handler.rs`           | CREATE | High-level file copying orchestration    |
| `src/files/errors.rs`            | CREATE | File operation specific errors           |
| `src/git/handler.rs`             | UPDATE | Integrate file copying into worktree creation |
| `src/sessions/handler.rs`        | UPDATE | Add file copying to session creation     |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Bidirectional sync** - not syncing changes back to main worktree
- **Real-time file watching** - not monitoring file changes after creation
- **Selective file exclusion** - not implementing exclude patterns within includes
- **File transformation** - not modifying file contents during copying
- **Symlink handling** - not creating symlinks, only copying actual files
- **Permission preservation** - using default file permissions, not preserving original

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (add dependencies)

- **ACTION**: ADD file pattern matching dependencies
- **IMPLEMENT**: `ignore = "0.4"`, `glob = "0.3"`, `walkdir = "2"`
- **MIRROR**: Existing dependency format in Cargo.toml
- **GOTCHA**: Use specific versions to avoid breaking changes
- **VALIDATE**: `cargo check` - dependencies must resolve

### Task 2: CREATE `src/files/mod.rs` (module structure)

- **ACTION**: CREATE new files module with submodules
- **IMPLEMENT**: `pub mod types; pub mod operations; pub mod handler; pub mod errors;`
- **MIRROR**: `src/sessions/mod.rs` - follow existing module pattern
- **VALIDATE**: `cargo check` - module structure must compile

### Task 3: CREATE `src/files/types.rs` (include pattern types)

- **ACTION**: CREATE types for include pattern configuration
- **IMPLEMENT**: `IncludeConfig`, `PatternRule`, `CopyOptions` structs
- **MIRROR**: `src/sessions/types.rs:5-20` - follow struct pattern with serde
- **IMPORTS**: `use serde::{Deserialize, Serialize}; use std::path::PathBuf;`
- **VALIDATE**: `cargo check` - types must compile

### Task 4: CREATE `src/files/errors.rs` (file operation errors)

- **ACTION**: CREATE file operation specific error types
- **IMPLEMENT**: `FileError` enum with pattern, copy, and validation variants
- **MIRROR**: `src/core/errors.rs:10-30` - follow error pattern with thiserror
- **PATTERN**: Extend base Error, include code and statusCode
- **VALIDATE**: `cargo check` - errors must compile

### Task 5: UPDATE `src/core/config.rs` (add include patterns config)

- **ACTION**: ADD include patterns field to ShardsConfig
- **IMPLEMENT**: `include_patterns: Option<IncludeConfig>` field
- **MIRROR**: `src/core/config.rs:20-40` - follow existing config pattern
- **IMPORTS**: `use crate::files::types::IncludeConfig;`
- **GOTCHA**: Use Option to make it optional, add serde default
- **VALIDATE**: `cargo check` - config must compile

### Task 6: CREATE `src/files/operations.rs` (pattern matching logic)

- **ACTION**: CREATE core pattern matching and file copying functions
- **IMPLEMENT**: `find_matching_files()`, `copy_file_safely()`, `validate_patterns()`
- **MIRROR**: `src/sessions/operations.rs:80-120` - follow operations pattern
- **IMPORTS**: `use ignore::{WalkBuilder, overrides::Override}; use glob::Pattern;`
- **GOTCHA**: Handle gitignore overrides correctly, use atomic file operations
- **VALIDATE**: `cargo test src/files/operations.rs`

### Task 7: CREATE `src/files/handler.rs` (file copying orchestration)

- **ACTION**: CREATE high-level file copying handler
- **IMPLEMENT**: `copy_include_files()` function with logging and error handling
- **MIRROR**: `src/sessions/handler.rs:25-60` - follow handler pattern
- **PATTERN**: Use structured logging, handle errors gracefully
- **IMPORTS**: `use tracing::{info, warn, error}; use crate::files::{operations, types, errors};`
- **VALIDATE**: `cargo test src/files/handler.rs`

### Task 8: UPDATE `src/git/handler.rs` (integrate file copying)

- **ACTION**: ADD file copying step after worktree creation
- **IMPLEMENT**: Call file copying handler after `repo.worktree()` but before return
- **MIRROR**: `src/git/handler.rs:75-85` - follow existing integration pattern
- **PATTERN**: Add structured logging for file copying events
- **GOTCHA**: Only copy files if include patterns are configured
- **VALIDATE**: `cargo test src/git/handler.rs`

### Task 9: UPDATE `src/sessions/handler.rs` (session creation integration)

- **ACTION**: ADD file copying to session creation workflow
- **IMPLEMENT**: Pass config to git handler, handle file copying errors
- **MIRROR**: `src/sessions/handler.rs:20-80` - follow existing workflow pattern
- **PATTERN**: Log file copying events, handle errors without failing session creation
- **VALIDATE**: `cargo test src/sessions/handler.rs`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                | Test Cases                 | Validates      |
| ---------------------------------------- | -------------------------- | -------------- |
| `src/files/tests/operations.rs`         | pattern matching, file copying | Core file operations |
| `src/files/tests/handler.rs`            | integration, error handling | File copying orchestration |
| `src/core/tests/config.rs`              | include pattern loading | Configuration parsing |

### Edge Cases Checklist

- [ ] Invalid glob patterns in config
- [ ] File copy permission errors
- [ ] Source file doesn't exist
- [ ] Destination directory creation failure
- [ ] Pattern matches no files
- [ ] Pattern matches too many files (performance)
- [ ] Circular symlinks in source directory
- [ ] Config file missing include patterns section

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test files:: && cargo test config::include
```

**EXPECT**: All file-related tests pass

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

### Level 4: INTEGRATION_VALIDATION

```bash
# Create test config with include patterns
echo '[include_patterns]
patterns = [".env*", "*.local.json", "build/artifacts/*"]' > .shards.toml

# Create test files to copy
echo "TEST_VAR=test" > .env.local
mkdir -p build/artifacts && echo "test" > build/artifacts/test.txt

# Create shard and verify files copied
cargo run -- create test-include --agent claude
ls ~/.shards/worktrees/*/test-include/.env.local
ls ~/.shards/worktrees/*/test-include/build/artifacts/test.txt
```

**EXPECT**: Include pattern files present in new worktree

### Level 5: MANUAL_VALIDATION

1. Create .shards.toml with include patterns
2. Create ignored files matching patterns
3. Create shard with `shards create`
4. Verify matched files are copied to worktree
5. Verify non-matching ignored files are not copied
6. Test with invalid patterns (should show helpful error)

---

## Acceptance Criteria

- [ ] Include patterns configurable in .shards.toml
- [ ] Glob patterns correctly match ignored files
- [ ] Matched files copied to new worktrees during shard creation
- [ ] File copying integrated seamlessly into existing workflow
- [ ] Comprehensive error handling for file operations
- [ ] Structured logging for file copying events
- [ ] Configuration validation with helpful error messages
- [ ] No impact on performance when include patterns not configured
- [ ] All existing functionality continues to work
- [ ] Level 1-4 validation commands pass with exit 0

---

## Completion Checklist

- [ ] Dependencies added to Cargo.toml
- [ ] Files module created with complete structure
- [ ] Include pattern types defined with serde support
- [ ] Configuration extended with include patterns
- [ ] Pattern matching and file copying logic implemented
- [ ] File copying integrated into worktree creation
- [ ] Error handling for all file operations
- [ ] Structured logging for file copying events
- [ ] All unit tests pass
- [ ] Integration testing completed
- [ ] Manual validation successful

---

## Risks and Mitigations

| Risk               | Likelihood   | Impact       | Mitigation                              |
| ------------------ | ------------ | ------------ | --------------------------------------- |
| Performance impact with large repos | MEDIUM | MEDIUM | Limit pattern matching scope, add timeout |
| File permission errors | HIGH | LOW | Graceful error handling, continue on failure |
| Invalid glob patterns | MEDIUM | LOW | Pattern validation at config load time |
| Disk space usage | LOW | MEDIUM | Document storage implications, add size limits |
| Race conditions in file copying | LOW | HIGH | Use atomic operations, proper error handling |

---

## Notes

**Configuration Example**:
```toml
[include_patterns]
patterns = [
    ".env*",           # Environment files
    "*.local.json",    # Local config files
    "build/dist/*",    # Build artifacts
    "node_modules/.bin/*"  # Specific build tools
]
enabled = true
max_file_size = "10MB"  # Optional size limit
```

**Performance Considerations**: Pattern matching will only run when include patterns are configured. File copying happens after worktree creation but before terminal launch to minimize user-visible delay.

**Future Enhancements**: Consider adding exclude patterns within includes, file transformation capabilities, and bidirectional sync options.
