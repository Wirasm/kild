# Feature: Bootstrap Vertical Slice Architecture

## Feature Description

Complete rebuild of Shards CLI using vertical slice architecture with structured logging, handler/operations pattern, and AI-optimized code organization. This replaces the existing layered POC with a production-ready architecture that follows the patterns defined in `.kiro/steering/architecture.md`.

## User Story

As a developer using AI coding agents
I want a clean, vertical slice architecture with structured logging
So that AI agents can easily understand, debug, and extend the codebase

## Problem Statement

The current POC uses layered architecture (CLI → Core → DB) which creates:
- Mixed I/O and business logic (hard to test)
- No structured logging (hard to debug)
- Generic error handling (poor UX)
- Cross-layer coupling (hard to maintain)
- No clear feature boundaries (confusing for AI agents)

## Solution Statement

Rebuild using vertical slice architecture where each feature (sessions, git, terminal) is self-contained with:
- Handler/operations separation for testability
- Structured logging with consistent event naming
- Feature-specific error types
- Clear boundaries and minimal coupling

## Feature Metadata

**Feature Type**: Refactor/Rebuild
**Estimated Complexity**: High
**Primary Systems Affected**: All (complete rebuild)
**Dependencies**: clap, git2, rusqlite, thiserror, tracing, tracing-subscriber

---

## CONTEXT REFERENCES

### Architecture Documentation IMPORTANT: READ BEFORE IMPLEMENTING!

- `.kiro/steering/architecture.md` - Complete architecture specification
- `.kiro/steering/product.md` - Product requirements and objectives  
- `.kiro/steering/progress.md` - Current feature overview
- `PRD.md` - Original POC requirements (CLI interface to preserve)
- `Cargo.toml` - Dependencies and project metadata

### CLI Interface to Preserve

From PRD.md - maintain exact same user interface:
```bash
shards <branch> --agent <name>  # Create shard
shards list                     # List shards  
shards destroy <branch>         # Remove shard
```

### Patterns to Follow

**Event Naming Convention**: `{domain}.{action}.{state}`
- `session.create_started`
- `session.create_completed` 
- `session.create_failed`
- `git.worktree.create_completed`
- `terminal.spawn_completed`

**Handler Pattern**:
```rust
// handler.rs - I/O orchestration + logging
pub fn create_session(name: &str, command: &str) -> Result<Session, SessionError> {
    info!(event = "session.create_started", name = name);
    
    let validated = operations::validate_session_request(name, command)?;
    // ... I/O operations
    
    info!(event = "session.create_completed", session_id = session.id);
    Ok(session)
}
```

**Operations Pattern**:
```rust  
// operations.rs - Pure business logic (no I/O)
pub fn validate_session_request(name: &str, command: &str) -> Result<ValidatedRequest, SessionError> {
    if name.is_empty() {
        return Err(SessionError::InvalidName);
    }
    // ... pure validation logic
}
```

**Error Pattern**:
```rust
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },
    
    #[error("Session '{name}' not found")]  
    NotFound { name: String },
}
```

---

## IMPLEMENTATION PLAN

### Phase 1: Foundation Infrastructure

Set up core infrastructure that exists before any features.

### Phase 2: Feature Slices  

Implement each feature as a complete vertical slice with handler/operations separation.

### Phase 3: CLI Integration

Wire up the CLI interface to use the new feature slices.

### Phase 4: Testing & Validation

Add comprehensive tests and validate the complete system.

---

## STEP-BY-STEP TASKS

Execute every task in order, top to bottom. Each task is atomic and independently testable.

### CREATE src/lib.rs

- **IMPLEMENT**: Library root with public exports
- **PATTERN**: Standard Rust library structure
- **IMPORTS**: Re-export main modules
- **VALIDATE**: `cargo check`

### CREATE src/main.rs

- **IMPLEMENT**: CLI entry point that calls lib functions
- **PATTERN**: Minimal main.rs that delegates to lib
- **IMPORTS**: Use clap for CLI parsing
- **VALIDATE**: `cargo run --help`

### CREATE src/core/mod.rs

- **IMPLEMENT**: Core module exports
- **PATTERN**: Foundation infrastructure only
- **EXPORTS**: config, logging, errors, events
- **VALIDATE**: `cargo check`

### CREATE src/core/config.rs

- **IMPLEMENT**: Application configuration with environment variables
- **PATTERN**: Settings struct with defaults
- **IMPORTS**: Use serde for serialization if needed
- **GOTCHA**: Keep simple for POC, no complex config files yet
- **VALIDATE**: `cargo test core::config`

### CREATE src/core/logging.rs

- **IMPLEMENT**: Structured JSON logging setup with tracing
- **PATTERN**: Follow architecture.md logging strategy exactly
- **IMPORTS**: tracing, tracing-subscriber
- **GOTCHA**: Use JSON format for AI parsing, include request correlation
- **VALIDATE**: `cargo test core::logging`

### CREATE src/core/errors.rs

- **IMPLEMENT**: Base error traits and common error types
- **PATTERN**: Use thiserror for structured errors
- **IMPORTS**: thiserror, std::error
- **GOTCHA**: Keep base traits minimal, features define specific errors
- **VALIDATE**: `cargo check`

### CREATE src/core/events.rs

- **IMPLEMENT**: Application lifecycle events (startup, shutdown)
- **PATTERN**: Simple event system for app lifecycle
- **IMPORTS**: tracing for logging events
- **VALIDATE**: `cargo check`

### CREATE src/sessions/mod.rs

- **IMPLEMENT**: Sessions feature module exports
- **PATTERN**: Complete feature slice structure
- **EXPORTS**: handler, operations, types, errors
- **VALIDATE**: `cargo check`

### CREATE src/sessions/types.rs

- **IMPLEMENT**: Session data structures and domain types
- **PATTERN**: Simple structs with serde derives
- **IMPORTS**: serde, chrono for timestamps
- **GOTCHA**: Keep aligned with database schema from PRD.md
- **VALIDATE**: `cargo test sessions::types`

### CREATE src/sessions/errors.rs

- **IMPLEMENT**: Session-specific error types
- **PATTERN**: Follow SessionError pattern from architecture.md
- **IMPORTS**: thiserror
- **GOTCHA**: Make errors grep-able with descriptive variants
- **VALIDATE**: `cargo test sessions::errors`

### CREATE src/sessions/operations.rs

- **IMPLEMENT**: Pure business logic for session operations
- **PATTERN**: No I/O, only pure functions that can be easily tested
- **IMPORTS**: Local types and errors only
- **GOTCHA**: Validation logic, business rules, calculations only
- **VALIDATE**: `cargo test sessions::operations`

### CREATE src/sessions/handler.rs

- **IMPLEMENT**: Session I/O orchestration with structured logging
- **PATTERN**: Follow handler pattern from architecture.md exactly
- **IMPORTS**: operations, git, terminal, database modules
- **GOTCHA**: Every operation must have started/completed/failed events
- **VALIDATE**: `cargo test sessions::handler`

### CREATE src/git/mod.rs

- **IMPLEMENT**: Git feature module exports
- **PATTERN**: Complete feature slice structure
- **EXPORTS**: handler, operations, types, errors
- **VALIDATE**: `cargo check`

### CREATE src/git/types.rs

- **IMPLEMENT**: Git-related data structures (worktree info, branch info)
- **PATTERN**: Simple structs representing git concepts
- **IMPORTS**: std::path::PathBuf for paths
- **VALIDATE**: `cargo test git::types`

### CREATE src/git/errors.rs

- **IMPLEMENT**: Git-specific error types
- **PATTERN**: Wrap git2::Error with context
- **IMPORTS**: thiserror, git2
- **GOTCHA**: Provide helpful error messages for common git issues
- **VALIDATE**: `cargo test git::errors`

### CREATE src/git/operations.rs

- **IMPLEMENT**: Pure git business logic (path calculations, validation)
- **PATTERN**: No actual git operations, just logic
- **IMPORTS**: std::path, local types
- **GOTCHA**: Worktree path generation, branch name validation
- **VALIDATE**: `cargo test git::operations`

### CREATE src/git/handler.rs

- **IMPLEMENT**: Git I/O operations with structured logging
- **PATTERN**: Actual git2 operations with proper error handling
- **IMPORTS**: git2, operations, tracing
- **GOTCHA**: Log all git operations with repo_path, branch, worktree_path
- **VALIDATE**: `cargo test git::handler`

### CREATE src/terminal/mod.rs

- **IMPLEMENT**: Terminal feature module exports
- **PATTERN**: Complete feature slice structure
- **EXPORTS**: handler, operations, types, errors
- **VALIDATE**: `cargo check`

### CREATE src/terminal/types.rs

- **IMPLEMENT**: Terminal-related data structures
- **PATTERN**: Terminal type enum, spawn configuration
- **IMPORTS**: std::process for command handling
- **VALIDATE**: `cargo test terminal::types`

### CREATE src/terminal/errors.rs

- **IMPLEMENT**: Terminal-specific error types
- **PATTERN**: Process spawn errors, terminal detection errors
- **IMPORTS**: thiserror, std::io
- **VALIDATE**: `cargo test terminal::errors`

### CREATE src/terminal/operations.rs

- **IMPLEMENT**: Pure terminal logic (command building, detection)
- **PATTERN**: Build commands without executing them
- **IMPORTS**: Local types only
- **GOTCHA**: Terminal detection logic, command string building
- **VALIDATE**: `cargo test terminal::operations`

### CREATE src/terminal/handler.rs

- **IMPLEMENT**: Terminal spawning with structured logging
- **PATTERN**: Actual process spawning with proper error handling
- **IMPORTS**: std::process, operations, tracing
- **GOTCHA**: Log terminal_type, command, working_directory
- **VALIDATE**: `cargo test terminal::handler`

### CREATE src/cli/mod.rs

- **IMPLEMENT**: CLI module exports
- **PATTERN**: Simple module structure
- **EXPORTS**: app, commands
- **VALIDATE**: `cargo check`

### CREATE src/cli/app.rs

- **IMPLEMENT**: Clap application definition
- **PATTERN**: Match exact CLI interface from PRD.md
- **IMPORTS**: clap with derive features
- **GOTCHA**: Preserve exact command structure and help text
- **VALIDATE**: `cargo run -- --help`

### CREATE src/cli/commands.rs

- **IMPLEMENT**: CLI command handlers that call feature handlers
- **PATTERN**: Thin layer that delegates to feature handlers
- **IMPORTS**: All feature handlers, clap
- **GOTCHA**: Initialize logging before any operations
- **VALIDATE**: `cargo run -- list` (should not crash)

### UPDATE Cargo.toml

- **ADD**: All required dependencies with correct versions
- **PATTERN**: Match dependencies from PRD.md
- **IMPORTS**: clap, git2, rusqlite, thiserror, tracing, tracing-subscriber
- **GOTCHA**: Use bundled feature for rusqlite
- **VALIDATE**: `cargo check`

### CREATE src/database/mod.rs

- **IMPLEMENT**: Database module (if needed as separate feature)
- **PATTERN**: Could be in shared/ or separate feature
- **DECISION**: Determine if database operations belong in sessions or separate
- **VALIDATE**: `cargo check`

---

## TESTING STRATEGY

### Unit Tests

Each feature slice has comprehensive unit tests:
- `operations.rs` tests (pure logic, easy to test)
- `types.rs` tests (data structure validation)
- `errors.rs` tests (error formatting and conversion)

### Integration Tests

- Full CLI command workflows
- Cross-feature integration (sessions + git + terminal)
- Database persistence and retrieval

### Edge Cases

- Invalid git repositories
- Missing terminal applications
- Duplicate session names
- Filesystem permission errors

---

## VALIDATION COMMANDS

Execute every command to ensure zero regressions and 100% feature correctness.

### Level 1: Syntax & Style

```bash
cargo check
cargo clippy -- -D warnings
cargo fmt --check
```

### Level 2: Unit Tests

```bash
cargo test --lib
cargo test --doc
```

### Level 3: Integration Tests

```bash
cargo test --test '*'
```

### Level 4: Manual Validation

```bash
# Help text works
cargo run -- --help
cargo run -- create --help
cargo run -- list --help
cargo run -- destroy --help

# Basic commands don't crash
cargo run -- list
```

### Level 5: End-to-End Validation

```bash
# Full workflow (requires git repo)
cd /tmp && git init test-repo && cd test-repo
cargo run -- create test-session --agent claude
cargo run -- list
cargo run -- destroy test-session
```

---

## ACCEPTANCE CRITERIA

- [ ] All CLI commands from PRD.md work identically
- [ ] Vertical slice architecture implemented per architecture.md
- [ ] Structured logging with consistent event naming
- [ ] Handler/operations separation in all features
- [ ] Feature-specific error types with helpful messages
- [ ] All validation commands pass with zero errors
- [ ] Unit test coverage >80% for operations modules
- [ ] Integration tests verify end-to-end workflows
- [ ] Code follows architecture.md patterns exactly
- [ ] No unwrap() or expect() in production code
- [ ] All I/O operations have proper error handling
- [ ] Logging includes all required context fields

---

## COMPLETION CHECKLIST

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] All validation commands executed successfully
- [ ] Full test suite passes (unit + integration)
- [ ] No clippy warnings or formatting issues
- [ ] Manual CLI testing confirms all commands work
- [ ] Architecture.md patterns followed exactly
- [ ] Structured logging implemented throughout
- [ ] Feature boundaries are clear and respected

---

## NOTES

**Key Architectural Decisions:**
- Handler/operations split enables easy testing of business logic
- Structured logging with event naming enables AI debugging
- Feature-specific errors provide better user experience
- Vertical slices reduce coupling and improve maintainability

**Implementation Order Rationale:**
- Core infrastructure first (logging, config, errors)
- Feature slices in dependency order (sessions depends on git + terminal)
- CLI integration last (thin layer over features)
- Testing throughout to catch issues early

**Success Metrics:**
- Same user experience as POC
- Clean architecture for future AI development
- Comprehensive logging for debugging
- Testable business logic
- Clear feature boundaries
