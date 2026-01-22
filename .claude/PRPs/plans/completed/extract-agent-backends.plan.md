# Feature: Extract Agent CLI Backends into Separate Modules

## Summary

Refactor scattered agent-specific logic into a centralized `agents/` module using an `AgentBackend` trait pattern. Each supported agent (Claude, Kiro, Gemini, Codex, Aether) gets its own backend implementation file, enabling polymorphic agent handling, isolated quirks management, and easy addition of new agents.

## User Story

As a developer maintaining SHARDS
I want agent-specific logic isolated in dedicated backend modules
So that I can add new agents by implementing a single trait without touching multiple files

## Problem Statement

Agent-specific logic is currently scattered across 4+ files with duplicate/conflicting definitions:
- `config/mod.rs:201` - Hardcoded valid agents list
- `config/mod.rs:316-323` - Default command mappings (e.g., `kiro` -> `"kiro-cli chat"`)
- `sessions/operations.rs:125-133` - **DUPLICATE** command mappings with different values (conflicts!)
- `process/operations.rs:180-192` - Agent-specific search patterns for process detection
- `sessions/types.rs:114-116` - Hardcoded default agent "claude"

Adding a new agent requires changes in 4+ locations, and the duplicate logic in `sessions/operations.rs` vs `config/mod.rs` creates bugs.

## Solution Statement

Create a new `agents/` module with:
1. `AgentBackend` trait defining the agent interface
2. Individual backend implementations in `backends/` directory
3. `AgentRegistry` to manage and lookup backends
4. Centralized constants and utilities

Existing code will delegate to the agents module instead of hardcoding agent knowledge.

## Metadata

| Field            | Value                                                                             |
| ---------------- | --------------------------------------------------------------------------------- |
| Type             | REFACTOR                                                                          |
| Complexity       | MEDIUM                                                                            |
| Systems Affected | config, sessions, process, lib.rs                                                 |
| Dependencies     | `which` crate (new) for CLI availability detection                                |
| Estimated Tasks  | 14                                                                                |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   config/mod.rs          sessions/ops.rs       process/ops.rs                 ║
║   ┌─────────────┐        ┌─────────────┐       ┌─────────────┐               ║
║   │ valid_agents│        │ get_agent_  │       │ generate_   │               ║
║   │ = ["claude",│        │ command()   │       │ search_     │               ║
║   │  "kiro"...] │        │ DUPLICATE!  │       │ patterns()  │               ║
║   │             │        │ Different   │       │ Hardcoded:  │               ║
║   │ get_agent_  │        │ values!     │       │ "kiro-cli"  │               ║
║   │ command()   │        │             │       │ "claude-    │               ║
║   └─────────────┘        └─────────────┘       │  code"      │               ║
║         │                      │               └─────────────┘               ║
║         │                      │                     │                        ║
║         └──────────────────────┴─────────────────────┘                        ║
║                          SCATTERED & DUPLICATED                               ║
║                                                                               ║
║   PAIN_POINTS:                                                                ║
║   - Adding new agent requires changes in 4+ files                             ║
║   - sessions/ops has different command for "claude" than config               ║
║   - No single source of truth for agent capabilities                          ║
║   - Agent-specific quirks (Claude version-as-process-name) buried in code     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   agents/                                                                     ║
║   ┌─────────────────────────────────────────────────────────────┐            ║
║   │  mod.rs (re-exports)                                         │            ║
║   │  types.rs (AgentType enum)                                   │            ║
║   │  registry.rs (AgentRegistry)                                 │            ║
║   │  trait.rs (AgentBackend trait)                               │            ║
║   │                                                              │            ║
║   │  backends/                                                   │            ║
║   │  ├── mod.rs                                                  │            ║
║   │  ├── claude.rs   impl AgentBackend for ClaudeBackend         │            ║
║   │  ├── kiro.rs     impl AgentBackend for KiroBackend           │            ║
║   │  ├── gemini.rs   impl AgentBackend for GeminiBackend         │            ║
║   │  ├── codex.rs    impl AgentBackend for CodexBackend          │            ║
║   │  └── aether.rs   impl AgentBackend for AetherBackend         │            ║
║   └─────────────────────────────────────────────────────────────┘            ║
║                          │                                                    ║
║                          ▼                                                    ║
║   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                          ║
║   │ config/     │  │ sessions/   │  │ process/    │                          ║
║   │ mod.rs      │  │ ops.rs      │  │ ops.rs      │                          ║
║   │             │  │             │  │             │                          ║
║   │ Delegates   │  │ REMOVED     │  │ Delegates   │                          ║
║   │ to agents   │  │ duplicate   │  │ to agents   │                          ║
║   │ module      │  │ function    │  │ module      │                          ║
║   └─────────────┘  └─────────────┘  └─────────────┘                          ║
║                                                                               ║
║   VALUE_ADD:                                                                  ║
║   - Single source of truth for all agent logic                               ║
║   - Add new agent = 1 new file + register                                    ║
║   - Agent quirks encapsulated (Claude version-name, Kiro chat subcommand)    ║
║   - Type-safe agent validation at compile time                               ║
║   - Easy to add future capabilities (health checks, telemetry, etc.)         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location                      | Before                           | After                                        | User Impact                         |
| ----------------------------- | -------------------------------- | -------------------------------------------- | ----------------------------------- |
| `config/mod.rs:validate()`    | Hardcoded `valid_agents` array   | `agents::is_valid_agent()`                   | Same validation, centralized        |
| `config/mod.rs:get_agent_cmd` | Hardcoded match statement        | `agents::get_default_command()`              | Same behavior, single source        |
| `sessions/ops.rs`             | Duplicate `get_agent_command()`  | REMOVED (use config's version)               | Fixes conflicting command values    |
| `process/ops.rs`              | Hardcoded search patterns        | `agents::get_search_patterns()`              | Same behavior, extensible           |
| New agent addition            | Edit 4+ files                    | Create 1 file in `backends/`, register it    | Much simpler to extend              |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File                                         | Lines   | Why Read This                              |
| -------- | -------------------------------------------- | ------- | ------------------------------------------ |
| P0       | `crates/shards-core/src/terminal/errors.rs`  | 1-57    | Error pattern to MIRROR for AgentError     |
| P0       | `crates/shards-core/src/errors/mod.rs`       | 1-57    | ShardsError trait to implement             |
| P0       | `crates/shards-core/src/config/mod.rs`       | 198-332 | Current agent handling to REPLACE          |
| P1       | `crates/shards-core/src/process/ops.rs`      | 165-195 | Search patterns to EXTRACT                 |
| P1       | `crates/shards-core/src/sessions/ops.rs`     | 125-133 | Duplicate code to REMOVE                   |
| P2       | `crates/shards-core/src/lib.rs`              | all     | Module registration pattern                |

**External Documentation:**

| Source                                                     | Section       | Why Needed                             |
| ---------------------------------------------------------- | ------------- | -------------------------------------- |
| [which crate v7.0](https://docs.rs/which/latest/which/)    | which::which  | CLI availability detection             |
| [thiserror v2](https://docs.rs/thiserror/latest/thiserror) | derive macros | Error enum pattern                     |

---

## Patterns to Mirror

**ERROR_HANDLING:**
```rust
// SOURCE: crates/shards-core/src/terminal/errors.rs:3-31
// COPY THIS PATTERN for AgentError:
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("No supported terminal found (tried: Ghostty, iTerm, Terminal.app)")]
    NoTerminalFound,

    #[error("Terminal '{terminal}' not found or not executable")]
    TerminalNotFound { terminal: String },
    // ...
}

impl ShardsError for TerminalError {
    fn error_code(&self) -> &'static str {
        match self {
            TerminalError::NoTerminalFound => "NO_TERMINAL_FOUND",
            // ...
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, TerminalError::NoTerminalFound | ...)
    }
}
```

**MODULE_STRUCTURE:**
```rust
// SOURCE: crates/shards-core/src/terminal/mod.rs
// COPY THIS PATTERN for agents/mod.rs:
pub mod errors;
pub mod handler;
pub mod operations;
pub mod types;
```

**TEST_PATTERN:**
```rust
// SOURCE: crates/shards-core/src/config/mod.rs:377-460
// COPY THIS PATTERN for inline tests:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_command_defaults() {
        let config = ShardsConfig::default();
        assert_eq!(config.get_agent_command("claude"), "claude");
        assert_eq!(config.get_agent_command("kiro"), "kiro-cli chat");
    }
}
```

---

## Files to Change

| File                                                | Action | Justification                                    |
| --------------------------------------------------- | ------ | ------------------------------------------------ |
| `Cargo.toml` (workspace)                            | UPDATE | Add `which = "7"` to workspace dependencies      |
| `crates/shards-core/Cargo.toml`                     | UPDATE | Add `which.workspace = true`                     |
| `crates/shards-core/src/agents/mod.rs`              | CREATE | Module re-exports and public API                 |
| `crates/shards-core/src/agents/types.rs`            | CREATE | AgentType enum, AgentInfo struct                 |
| `crates/shards-core/src/agents/errors.rs`           | CREATE | AgentError enum                                  |
| `crates/shards-core/src/agents/traits.rs`           | CREATE | AgentBackend trait definition                    |
| `crates/shards-core/src/agents/registry.rs`         | CREATE | AgentRegistry for backend management             |
| `crates/shards-core/src/agents/backends/mod.rs`     | CREATE | Backend module exports                           |
| `crates/shards-core/src/agents/backends/claude.rs`  | CREATE | Claude backend implementation                    |
| `crates/shards-core/src/agents/backends/kiro.rs`    | CREATE | Kiro backend implementation                      |
| `crates/shards-core/src/agents/backends/gemini.rs`  | CREATE | Gemini backend implementation                    |
| `crates/shards-core/src/agents/backends/codex.rs`   | CREATE | Codex backend implementation                     |
| `crates/shards-core/src/agents/backends/aether.rs`  | CREATE | Aether backend implementation                    |
| `crates/shards-core/src/lib.rs`                     | UPDATE | Add `pub mod agents;` and re-exports             |
| `crates/shards-core/src/config/mod.rs`              | UPDATE | Delegate to agents module                        |
| `crates/shards-core/src/sessions/operations.rs`     | UPDATE | Remove duplicate get_agent_command function      |
| `crates/shards-core/src/process/operations.rs`      | UPDATE | Delegate search patterns to agents module        |
| `crates/shards-core/src/errors/mod.rs`              | UPDATE | Update InvalidAgent error message                |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Agent health checks** - Future enhancement, not part of this refactor
- **Auto-detection of installed agents at startup** - Can be added later using `is_available()`
- **Per-agent telemetry/logging** - Future enhancement
- **Agent-specific environment variables** - Keep existing config approach
- **Custom agent support (user-defined backends)** - Config already allows custom commands
- **Windows/Linux agent detection** - Keep macOS focus for now

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: ADD `which` crate dependency

- **ACTION**: UPDATE workspace Cargo.toml to add `which = "7"` dependency
- **IMPLEMENT**: Add to `[workspace.dependencies]` section
- **FILE**: `Cargo.toml` (workspace root)
- **GOTCHA**: Use version "7" which is latest stable
- **VALIDATE**: `cargo check --workspace`

### Task 2: ADD `which` to shards-core dependencies

- **ACTION**: UPDATE shards-core Cargo.toml
- **IMPLEMENT**: Add `which.workspace = true` to dependencies
- **FILE**: `crates/shards-core/Cargo.toml`
- **VALIDATE**: `cargo check -p shards-core`

### Task 3: CREATE `agents/types.rs`

- **ACTION**: CREATE type definitions
- **IMPLEMENT**:
  ```rust
  /// Supported agent types
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub enum AgentType {
      Claude,
      Kiro,
      Gemini,
      Codex,
      Aether,
  }

  impl AgentType {
      pub fn as_str(&self) -> &'static str { ... }
      pub fn from_str(s: &str) -> Option<Self> { ... }
      pub fn all() -> &'static [AgentType] { ... }
  }

  /// Runtime information about an agent
  pub struct AgentInfo {
      pub agent_type: AgentType,
      pub is_available: bool,
      pub command: String,
  }
  ```
- **MIRROR**: Enum pattern from `crates/shards-core/src/terminal/types.rs:1-20` (TerminalType)
- **VALIDATE**: `cargo check -p shards-core`

### Task 4: CREATE `agents/errors.rs`

- **ACTION**: CREATE error enum with ShardsError impl
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum AgentError {
      #[error("Unknown agent '{name}'. Supported: claude, kiro, gemini, codex, aether")]
      UnknownAgent { name: String },

      #[error("Agent '{name}' CLI is not installed or not in PATH")]
      AgentNotAvailable { name: String },
  }

  impl ShardsError for AgentError { ... }
  ```
- **MIRROR**: `crates/shards-core/src/terminal/errors.rs:1-57`
- **IMPORTS**: `use crate::errors::ShardsError;`
- **VALIDATE**: `cargo check -p shards-core`

### Task 5: CREATE `agents/traits.rs`

- **ACTION**: CREATE AgentBackend trait
- **IMPLEMENT**:
  ```rust
  pub trait AgentBackend: Send + Sync {
      /// The canonical name of this agent (e.g., "claude", "kiro")
      fn name(&self) -> &'static str;

      /// The display name for this agent
      fn display_name(&self) -> &'static str;

      /// Check if this agent's CLI is installed and available
      fn is_available(&self) -> bool;

      /// Get the default command to launch this agent
      fn default_command(&self) -> &'static str;

      /// Get process name patterns for detection
      /// Handles quirks like Claude showing version as process name
      fn process_patterns(&self) -> Vec<String>;

      /// Get additional command line variations to search for
      fn command_patterns(&self) -> Vec<String> {
          vec![self.default_command().to_string()]
      }
  }
  ```
- **GOTCHA**: Trait must be object-safe for registry to hold `Box<dyn AgentBackend>`
- **VALIDATE**: `cargo check -p shards-core`

### Task 6: CREATE `agents/backends/claude.rs`

- **ACTION**: CREATE Claude backend implementation
- **IMPLEMENT**:
  ```rust
  pub struct ClaudeBackend;

  impl AgentBackend for ClaudeBackend {
      fn name(&self) -> &'static str { "claude" }
      fn display_name(&self) -> &'static str { "Claude Code" }

      fn is_available(&self) -> bool {
          which::which("claude").is_ok()
      }

      fn default_command(&self) -> &'static str { "claude" }

      fn process_patterns(&self) -> Vec<String> {
          // Claude's process can show as version number or "claude"
          vec![
              "claude".to_string(),
              "claude-code".to_string(),
          ]
      }
  }
  ```
- **GOTCHA**: Claude process sometimes shows version number (e.g., "2.1.15") - future enhancement
- **VALIDATE**: `cargo check -p shards-core`

### Task 7: CREATE `agents/backends/kiro.rs`

- **ACTION**: CREATE Kiro backend implementation
- **IMPLEMENT**:
  ```rust
  pub struct KiroBackend;

  impl AgentBackend for KiroBackend {
      fn name(&self) -> &'static str { "kiro" }
      fn display_name(&self) -> &'static str { "Kiro CLI" }

      fn is_available(&self) -> bool {
          which::which("kiro-cli").is_ok()
      }

      fn default_command(&self) -> &'static str { "kiro-cli chat" }

      fn process_patterns(&self) -> Vec<String> {
          vec![
              "kiro-cli".to_string(),
              "kiro".to_string(),
          ]
      }
  }
  ```
- **GOTCHA**: Kiro uses subcommand `chat`, not just `kiro-cli`
- **VALIDATE**: `cargo check -p shards-core`

### Task 8: CREATE remaining backends (gemini, codex, aether)

- **ACTION**: CREATE GeminiBackend, CodexBackend, AetherBackend
- **IMPLEMENT**: Same pattern as Claude/Kiro backends
- **FILES**:
  - `agents/backends/gemini.rs` - default_command: "gemini"
  - `agents/backends/codex.rs` - default_command: "codex"
  - `agents/backends/aether.rs` - default_command: "aether"
- **VALIDATE**: `cargo check -p shards-core`

### Task 9: CREATE `agents/backends/mod.rs`

- **ACTION**: CREATE backend module exports
- **IMPLEMENT**:
  ```rust
  mod claude;
  mod kiro;
  mod gemini;
  mod codex;
  mod aether;

  pub use claude::ClaudeBackend;
  pub use kiro::KiroBackend;
  pub use gemini::GeminiBackend;
  pub use codex::CodexBackend;
  pub use aether::AetherBackend;
  ```
- **VALIDATE**: `cargo check -p shards-core`

### Task 10: CREATE `agents/registry.rs`

- **ACTION**: CREATE AgentRegistry with static registration
- **IMPLEMENT**:
  ```rust
  use std::sync::LazyLock;
  use std::collections::HashMap;

  static REGISTRY: LazyLock<AgentRegistry> = LazyLock::new(AgentRegistry::new);

  pub struct AgentRegistry {
      backends: HashMap<&'static str, Box<dyn AgentBackend>>,
  }

  impl AgentRegistry {
      fn new() -> Self {
          let mut backends: HashMap<&'static str, Box<dyn AgentBackend>> = HashMap::new();
          backends.insert("claude", Box::new(ClaudeBackend));
          backends.insert("kiro", Box::new(KiroBackend));
          backends.insert("gemini", Box::new(GeminiBackend));
          backends.insert("codex", Box::new(CodexBackend));
          backends.insert("aether", Box::new(AetherBackend));
          Self { backends }
      }

      pub fn get(name: &str) -> Option<&'static dyn AgentBackend> { ... }
      pub fn is_valid_agent(name: &str) -> bool { ... }
      pub fn valid_agent_names() -> Vec<&'static str> { ... }
      pub fn default_agent() -> &'static str { "claude" }
  }

  // Public API functions
  pub fn get_agent(name: &str) -> Option<&'static dyn AgentBackend> { ... }
  pub fn is_valid_agent(name: &str) -> bool { ... }
  pub fn valid_agent_names() -> Vec<&'static str> { ... }
  pub fn default_agent_name() -> &'static str { ... }
  pub fn get_default_command(name: &str) -> Option<&'static str> { ... }
  pub fn get_process_patterns(name: &str) -> Vec<String> { ... }
  ```
- **GOTCHA**: Use `LazyLock` (Rust 1.80+) for static initialization
- **VALIDATE**: `cargo check -p shards-core`

### Task 11: CREATE `agents/mod.rs`

- **ACTION**: CREATE module with public API re-exports
- **IMPLEMENT**:
  ```rust
  pub mod backends;
  pub mod errors;
  pub mod registry;
  pub mod traits;
  pub mod types;

  // Re-export public API
  pub use errors::AgentError;
  pub use registry::{
      default_agent_name,
      get_agent,
      get_default_command,
      get_process_patterns,
      is_valid_agent,
      valid_agent_names,
  };
  pub use traits::AgentBackend;
  pub use types::AgentType;
  ```
- **VALIDATE**: `cargo check -p shards-core`

### Task 12: UPDATE `lib.rs` to export agents module

- **ACTION**: UPDATE lib.rs to add agents module
- **IMPLEMENT**: Add `pub mod agents;` and re-exports
- **FILE**: `crates/shards-core/src/lib.rs`
- **VALIDATE**: `cargo check -p shards-core`

### Task 13: UPDATE `config/mod.rs` to use agents module

- **ACTION**: REPLACE hardcoded agent logic with agents module calls
- **IMPLEMENT**:
  - Line 201: Replace `let valid_agents = [...]` with `agents::is_valid_agent(&self.agent.default)`
  - Line 316-323: Replace match statement with `agents::get_default_command(agent_name).unwrap_or(agent_name)`
  - Line 132-134: Replace `default_agent()` with `agents::default_agent_name().to_string()`
- **GOTCHA**: Keep backward compatibility - same behavior, just delegated
- **VALIDATE**: `cargo test -p shards-core config::tests`

### Task 14: UPDATE `sessions/operations.rs` - REMOVE duplicate function

- **ACTION**: REMOVE `get_agent_command()` function (lines 125-133)
- **IMPLEMENT**: Delete the function and update any callers to use `config.get_agent_command()`
- **GOTCHA**: This function has different values than config - verify callers use config version
- **VALIDATE**: `cargo test -p shards-core sessions::tests`

### Task 15: UPDATE `process/operations.rs` to use agents module

- **ACTION**: REPLACE hardcoded search patterns with agents module
- **IMPLEMENT**:
  - Replace lines 180-192 match statement with:
    ```rust
    // Add agent-specific patterns if this looks like an agent name
    if let Some(patterns) = agents::get_process_patterns(name_pattern) {
        for pattern in patterns {
            patterns_set.insert(pattern);
        }
    }
    ```
- **VALIDATE**: `cargo test -p shards-core process::tests`

### Task 16: ADD comprehensive tests for agents module

- **ACTION**: CREATE tests for all agent functionality
- **IMPLEMENT**: Add inline tests to each agents module file
- **TEST CASES**:
  - AgentType enum conversions
  - is_valid_agent() for all agents + invalid
  - get_default_command() returns correct commands
  - get_process_patterns() returns expected patterns
  - Registry contains all 5 agents
- **VALIDATE**: `cargo test -p shards-core agents`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                      | Test Cases                                  | Validates             |
| ---------------------------------------------- | ------------------------------------------- | --------------------- |
| `agents/types.rs`                              | as_str, from_str, all() for AgentType       | Type conversions      |
| `agents/errors.rs`                             | error messages, error codes                 | Error formatting      |
| `agents/registry.rs`                           | get_agent, is_valid, valid_names, defaults  | Registry correctness  |
| `agents/backends/*.rs`                         | Each backend returns correct values         | Backend implementations |
| `config/mod.rs` (existing)                     | Existing tests should still pass            | No regression         |

### Edge Cases Checklist

- [ ] Unknown agent name returns None/error appropriately
- [ ] Case sensitivity (should be lowercase)
- [ ] Empty string agent name
- [ ] Default agent is always "claude"
- [ ] Process patterns include variations (kiro/kiro-cli, claude/claude-code)
- [ ] `is_available()` returns false when CLI not installed (don't error)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo clippy --workspace -- -D warnings && cargo fmt --check
```

**EXPECT**: Exit 0, no warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p shards-core
```

**EXPECT**: All tests pass including new agents tests

### Level 3: FULL_SUITE

```bash
cargo test --workspace && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

---

## Acceptance Criteria

- [ ] Each agent backend is in its own file under `agents/backends/`
- [ ] `AgentBackend` trait defines the full interface
- [ ] Process detection uses agent-specific patterns via `get_process_patterns()`
- [ ] Command building uses `get_default_command()` from agents module
- [ ] Adding a new agent only requires: new file in backends/ + register in registry
- [ ] All existing tests pass
- [ ] Duplicate `get_agent_command()` in sessions/operations.rs is removed
- [ ] No hardcoded agent lists remain outside agents module

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (clippy + fmt) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk                                            | Likelihood | Impact | Mitigation                                                |
| ----------------------------------------------- | ---------- | ------ | --------------------------------------------------------- |
| Behavioral regression from refactor             | LOW        | HIGH   | Keep exact same return values, comprehensive tests        |
| sessions/ops `get_agent_command` callers broken | MED        | MED    | Grep for all callers, update to use config version        |
| `which` crate version compatibility             | LOW        | LOW    | Use v7 (latest), test on CI                               |
| LazyLock not available (pre-Rust 1.80)          | LOW        | MED    | Use `once_cell::sync::Lazy` if needed (already a pattern) |

---

## Notes

- The `sessions/operations.rs:get_agent_command()` function returns DIFFERENT values than `config/mod.rs:get_agent_command()`:
  - sessions: `"claude"` -> `"cc"`, `"kiro"` -> `"kiro-cli"`
  - config: `"claude"` -> `"claude"`, `"kiro"` -> `"kiro-cli chat"`

  This is a BUG. The config version should be authoritative. When removing the sessions version, verify no code depends on the `"cc"` alias.

- The `which` crate is standard for CLI availability detection in Rust. Version 7.x is stable.

- Future enhancement: Add `is_available()` checks during shard creation to warn users if an agent CLI isn't installed.

- The trait pattern allows future additions like:
  - `fn health_check(&self) -> HealthStatus`
  - `fn supports_flag(&self, flag: &str) -> bool`
  - `fn required_env_vars(&self) -> Vec<&str>`
