# Investigation: Remove process module dependency on agents module

**Issue**: #326 (https://github.com/Wirasm/kild/issues/326)
**Type**: REFACTOR
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                                              |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------- |
| Priority   | LOW    | Architecture hygiene improvement; no user-facing impact, no blocking issues, identified during dependency analysis      |
| Complexity | LOW    | 2 files to modify (operations.rs + handler.rs), isolated change with clear pattern already used by `kill_process()`    |
| Confidence | HIGH   | Clear root cause at `operations.rs:6`, single coupling point via two function calls, existing pattern to mirror         |

---

## Problem Statement

The `process` module (`crates/kild-core/src/process/operations.rs`) imports the `agents` module to look up agent-specific process name patterns during process detection. This creates an upward dependency where a low-level utility layer (`process`) has domain-awareness of agent types (`agents`). The process module should be a generic utility that receives search patterns from callers rather than knowing about agents directly.

---

## Analysis

### Change Rationale

The `process` module is a low-level utility for PID tracking, process detection, and process lifecycle management. It should not have domain knowledge about agent types. The coupling exists at a single point: `generate_search_patterns()` calls `agents::get_process_patterns()` and `agents::valid_agent_names()` to enhance search patterns with agent-specific names.

The codebase already demonstrates the correct pattern: `kill_process()` at `operations.rs:68` receives `expected_name` and `expected_start_time` from its caller rather than looking them up internally. The same approach should be applied to `find_process_by_name()`.

### Evidence Chain

WHY: `process` module has domain awareness of agents
BECAUSE: `generate_search_patterns()` calls `agents::get_process_patterns()` and `agents::valid_agent_names()`
Evidence: `crates/kild-core/src/process/operations.rs:187,206` - direct calls to agents module

BECAUSE: `find_process_by_name()` calls `generate_search_patterns()` without accepting external patterns
Evidence: `crates/kild-core/src/process/operations.rs:265` - `let search_patterns = generate_search_patterns(name_pattern);`

ROOT CAUSE: `find_process_by_name()` signature doesn't accept additional search patterns, forcing the internal function to import agent knowledge.
Evidence: `crates/kild-core/src/process/operations.rs:257-259` - `pub fn find_process_by_name(name_pattern: &str, command_pattern: Option<&str>)`

### Affected Files

| File                                                  | Lines   | Action | Description                                                         |
| ----------------------------------------------------- | ------- | ------ | ------------------------------------------------------------------- |
| `crates/kild-core/src/process/operations.rs`          | 6, 171-219, 257-265 | UPDATE | Add `additional_patterns` param, remove `use crate::agents` import |
| `crates/kild-core/src/terminal/handler.rs`             | 31      | UPDATE | Pass agent patterns to `find_process_by_name()`                    |

### Integration Points

- `crates/kild-core/src/terminal/handler.rs:31` - Only caller of `find_process_by_name()` (via `find_agent_process_with_retry`)
- `crates/kild-core/src/process/mod.rs:8` - Re-exports `find_process_by_name`

### Git History

- **Last modified**: `574ea0f` - "fix: replace mutex unwrap with proper error handling (#333) (#339)"
- **Introduced**: `160314d` - "Rebrand Shards to KILD (#110)"
- **Implication**: Long-standing coupling since initial codebase, not a regression

---

## Implementation Plan

### Step 1: Add `additional_patterns` parameter to `find_process_by_name()`

**File**: `crates/kild-core/src/process/operations.rs`
**Lines**: 257-265
**Action**: UPDATE

**Current code:**

```rust
/// Find a process by name, optionally filtering by command line pattern
pub fn find_process_by_name(
    name_pattern: &str,
    command_pattern: Option<&str>,
) -> Result<Option<ProcessInfo>, ProcessError> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    // Try multiple search strategies
    let search_patterns = generate_search_patterns(name_pattern);
```

**Required change:**

```rust
/// Find a process by name, optionally filtering by command line pattern.
///
/// `additional_patterns` allows callers to provide domain-specific search patterns
/// (e.g., agent process names) without the process module needing domain awareness.
pub fn find_process_by_name(
    name_pattern: &str,
    command_pattern: Option<&str>,
    additional_patterns: Option<&[String]>,
) -> Result<Option<ProcessInfo>, ProcessError> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    // Try multiple search strategies
    let search_patterns = generate_search_patterns(name_pattern, additional_patterns);
```

**Why**: Shifts domain knowledge (agent patterns) to the caller, keeping process module generic.

---

### Step 2: Update `generate_search_patterns()` to accept external patterns

**File**: `crates/kild-core/src/process/operations.rs`
**Lines**: 171-219
**Action**: UPDATE

**Current code:**

```rust
fn generate_search_patterns(name_pattern: &str) -> Vec<String> {
    let mut patterns = std::collections::HashSet::new();
    patterns.insert(name_pattern.to_string());

    // Add partial matches
    if name_pattern.contains('-') {
        patterns.insert(
            name_pattern
                .split('-')
                .next()
                .unwrap_or(name_pattern)
                .to_string(),
        );
    }

    // Add agent-specific patterns if this is a known agent name or pattern
    if let Some(agent_patterns) = agents::get_process_patterns(name_pattern) {
        // ... agent lookup code ...
    }

    // Also check if this pattern matches known agent process patterns and add the agent name
    for agent_name in agents::valid_agent_names() {
        // ... reverse lookup code ...
    }

    patterns.into_iter().collect()
}
```

**Required change:**

```rust
fn generate_search_patterns(name_pattern: &str, additional_patterns: Option<&[String]>) -> Vec<String> {
    let mut patterns = std::collections::HashSet::new();
    patterns.insert(name_pattern.to_string());

    // Add partial matches
    if name_pattern.contains('-') {
        patterns.insert(
            name_pattern
                .split('-')
                .next()
                .unwrap_or(name_pattern)
                .to_string(),
        );
    }

    // Add caller-provided patterns (e.g., agent-specific process names)
    if let Some(extra) = additional_patterns {
        for pattern in extra {
            patterns.insert(pattern.clone());
        }
    }

    patterns.into_iter().collect()
}
```

**Why**: Removes all agent-specific logic (both forward and reverse lookup). The caller now provides any domain-specific patterns.

---

### Step 3: Remove agents import

**File**: `crates/kild-core/src/process/operations.rs`
**Line**: 6
**Action**: UPDATE

**Current code:**

```rust
use crate::agents;
```

**Required change:**

Remove this line entirely.

**Why**: No longer needed after moving agent pattern resolution to callers.

---

### Step 4: Update caller in terminal/handler.rs

**File**: `crates/kild-core/src/terminal/handler.rs`
**Line**: 31
**Action**: UPDATE

**Current code:**

```rust
use crate::process::{
    ensure_pid_dir, get_pid_file_path, get_process_info, is_process_running,
    read_pid_file_with_retry, wrap_command_with_pid_capture,
};
// ...
match crate::process::find_process_by_name(agent_name, Some(command)) {
```

**Required change:**

```rust
use crate::agents;
use crate::process::{
    ensure_pid_dir, find_process_by_name, get_pid_file_path, get_process_info,
    is_process_running, read_pid_file_with_retry, wrap_command_with_pid_capture,
};
// ...
```

And in `find_agent_process_with_retry`, resolve agent patterns once before the retry loop and pass them:

```rust
fn find_agent_process_with_retry(
    agent_name: &str,
    command: &str,
    config: &KildConfig,
) -> ProcessSearchResult {
    let max_attempts = config.terminal.max_retry_attempts;
    let mut delay_ms = config.terminal.spawn_delay_ms;

    // Resolve agent-specific process patterns once, before the retry loop
    let agent_patterns = agents::get_all_process_patterns(agent_name);
    let patterns_ref = if agent_patterns.is_empty() {
        None
    } else {
        Some(agent_patterns.as_slice())
    };

    for attempt in 1..=max_attempts {
        // ...
        match find_process_by_name(agent_name, Some(command), patterns_ref) {
```

**Why**: Moves agent awareness to the terminal/handler layer where it belongs - this layer already knows it's spawning an agent.

---

### Step 5: Add `get_all_process_patterns()` helper to agents module

**File**: `crates/kild-core/src/agents/registry.rs`
**Action**: UPDATE

Add a convenience function that resolves patterns in both directions (agent name → patterns AND pattern → all agent patterns), consolidating the logic that was in `generate_search_patterns()`:

```rust
/// Get all process patterns for an agent, including bidirectional resolution.
///
/// Given a name, this:
/// 1. Looks up patterns if `name` is a known agent name
/// 2. Looks up which agent owns `name` if it's a known process pattern
/// Returns deduplicated combined patterns, or empty vec if no match.
pub fn get_all_process_patterns(name: &str) -> Vec<String> {
    let mut patterns = Vec::new();

    // Forward: name is an agent name → get its patterns
    if let Some(agent_patterns) = get_process_patterns(name) {
        patterns.extend(agent_patterns);
    }

    // Reverse: name is a process pattern → find owning agent's patterns
    for agent_name in valid_agent_names() {
        if let Some(agent_patterns) = get_process_patterns(agent_name) {
            if agent_patterns.iter().any(|p| p == name) {
                patterns.extend(agent_patterns);
            }
        }
    }

    // Deduplicate
    patterns.sort();
    patterns.dedup();
    patterns
}
```

Also add to `crates/kild-core/src/agents/mod.rs` re-exports:

```rust
pub use registry::get_all_process_patterns;
```

**Why**: Preserves the bidirectional resolution logic (previously in `generate_search_patterns()`) in the agents module where it belongs.

---

### Step 6: Update tests

**File**: `crates/kild-core/src/process/operations.rs`
**Lines**: 399-438
**Action**: UPDATE

Update `test_generate_search_patterns` to pass patterns explicitly instead of relying on agent module:

```rust
#[test]
fn test_generate_search_patterns() {
    // With additional patterns provided by caller
    let extra = vec!["kiro-cli".to_string(), "kiro".to_string()];
    let patterns = generate_search_patterns("kiro-cli", Some(&extra));
    assert!(patterns.contains(&"kiro-cli".to_string()));
    assert!(patterns.contains(&"kiro".to_string()));

    // With no additional patterns, only generic matching
    let patterns = generate_search_patterns("claude", None);
    assert_eq!(patterns.len(), 1);
    assert!(patterns.contains(&"claude".to_string()));

    // Dash splitting still works generically
    let patterns = generate_search_patterns("no-match-agent", None);
    assert!(patterns.contains(&"no-match-agent".to_string()));
    assert!(patterns.contains(&"no".to_string()));
    assert_eq!(patterns.len(), 2);

    let patterns = generate_search_patterns("simple", None);
    assert_eq!(patterns.len(), 1);
    assert!(patterns.contains(&"simple".to_string()));

    // Edge cases
    let patterns = generate_search_patterns("", None);
    assert!(patterns.contains(&"".to_string()));

    let patterns = generate_search_patterns("very-long-agent-name-with-many-dashes", None);
    assert!(patterns.contains(&"very-long-agent-name-with-many-dashes".to_string()));
    assert!(patterns.contains(&"very".to_string()));
    assert_eq!(patterns.len(), 2);
}
```

Also update `test_find_process_by_name_with_partial_match`:

```rust
#[test]
fn test_find_process_by_name_with_partial_match() {
    let result = find_process_by_name("nonexistent", None, None);
    assert!(result.is_ok());
}
```

**File**: `crates/kild-core/src/agents/registry.rs`
**Action**: UPDATE

Add test for `get_all_process_patterns`:

```rust
#[test]
fn test_get_all_process_patterns() {
    // Forward lookup: agent name → patterns
    let patterns = get_all_process_patterns("claude");
    assert!(patterns.contains(&"claude".to_string()));
    assert!(patterns.contains(&"claude-code".to_string()));

    // Reverse lookup: process pattern → all agent patterns
    let patterns = get_all_process_patterns("claude-code");
    assert!(patterns.contains(&"claude".to_string()));
    assert!(patterns.contains(&"claude-code".to_string()));

    // Unknown name: empty
    let patterns = get_all_process_patterns("unknown");
    assert!(patterns.is_empty());
}
```

---

## Patterns to Follow

**From codebase - mirror `kill_process()` pattern:**

```rust
// SOURCE: crates/kild-core/src/process/operations.rs:68-72
// Pattern: caller passes expected metadata, process module stays generic
pub fn kill_process(
    pid: u32,
    expected_name: Option<&str>,
    expected_start_time: Option<u64>,
) -> Result<(), ProcessError> {
```

---

## Edge Cases & Risks

| Risk/Edge Case                    | Mitigation                                                                 |
| --------------------------------- | -------------------------------------------------------------------------- |
| Callers forget to pass patterns   | `additional_patterns: Option` defaults to `None`, generic matching still works |
| Performance of resolving patterns | Done once before retry loop, not per-attempt                               |
| Debug logging changes             | Agent pattern debug events move from process module to be implicit in caller |

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

1. `cargo run -p kild -- create test-branch --agent claude` - verify process detection still works
2. `cargo run -p kild -- stop test-branch && cargo run -p kild -- destroy test-branch` - cleanup

---

## Scope Boundaries

**IN SCOPE:**

- Remove `use crate::agents` from `process/operations.rs`
- Add `additional_patterns` parameter to `find_process_by_name()` and `generate_search_patterns()`
- Move agent pattern resolution to `terminal/handler.rs` (the only caller)
- Add `get_all_process_patterns()` to agents module for bidirectional resolution
- Update tests

**OUT OF SCOPE (do not touch):**

- `kill_process()` - already follows the correct pattern
- `is_process_running()`, `get_process_info()`, `get_process_metrics()` - don't use agents
- Agent backend definitions or registry structure
- Other callers of process module functions (they don't use `find_process_by_name`)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-326.md`
