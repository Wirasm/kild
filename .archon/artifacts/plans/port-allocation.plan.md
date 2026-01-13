# Feature: Port Allocation for Shards

## Summary

Add dynamic port allocation to Shards CLI to prevent port conflicts between multiple shards running on the same project. Each shard gets a configurable number of ports (default 10) from non-overlapping ranges, with automatic cleanup during shard destruction and pruning operations.

## User Story

As a developer using multiple AI agents in parallel shards
I want each shard to have its own allocated port range
So that microservices in different shards don't conflict and I can run multiple development environments simultaneously

## Problem Statement

When multiple shards run microservices that need ports (e.g., web servers, databases, APIs), they currently have no port coordination mechanism. This leads to port conflicts, failed service startups, and manual port management overhead.

## Solution Statement

Implement a port allocation system that assigns non-overlapping port ranges to each shard, tracks allocations in session state, and automatically cleans up ports during shard lifecycle operations.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | MEDIUM                                            |
| Systems Affected | sessions, core/config, cli                        |
| Dependencies     | serde, serde_json (existing)                      |
| Estimated Tasks  | 8                                                 |

---

## UX Design

### Before State
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              BEFORE STATE                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │   Shard A   │ ──────► │ Port 3000   │ ──────► │   CONFLICT  │              │
│   │ (microservice)│       │ (hardcoded) │         │   ERROR     │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                                                                 │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │   Shard B   │ ──────► │ Port 3000   │ ──────► │   CONFLICT  │              │
│   │ (microservice)│       │ (hardcoded) │         │   ERROR     │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                                                                 │
│   USER_FLOW: Create shard → Start microservice → Port conflict → Manual fix    │
│   PAIN_POINT: No port coordination, manual port management required            │
│   DATA_FLOW: No port tracking, conflicts discovered at runtime                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│                               AFTER STATE                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │   Shard A   │ ──────► │ Ports       │ ──────► │   SUCCESS   │              │
│   │ (microservice)│       │ 3000-3009   │         │   RUNNING   │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                   │                                             │
│                                   ▼                                             │
│                          ┌─────────────┐                                        │
│                          │PORT_TRACKER │  ◄── Allocation registry               │
│                          └─────────────┘                                        │
│                                   │                                             │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │   Shard B   │ ──────► │ Ports       │ ──────► │   SUCCESS   │              │
│   │ (microservice)│       │ 3100-3109   │         │   RUNNING   │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                                                                 │
│   USER_FLOW: Create shard → Auto port allocation → Start microservice → Success│
│   VALUE_ADD: Zero port conflicts, automatic cleanup, configurable ranges       │
│   DATA_FLOW: Port allocation → Session storage → Environment variables         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes
| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `shards create` | No port info | Shows allocated port range | User knows available ports |
| `shards list` | No port column | Port range column | User sees all allocations |
| `shards destroy` | Only removes worktree | Removes worktree + frees ports | Automatic cleanup |
| Environment | Manual port config | Auto PORT_RANGE env vars | Services auto-configure |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/sessions/types.rs` | 1-50 | Session struct to EXTEND with port fields |
| P0 | `src/sessions/operations.rs` | 80-120 | Port calculation pattern to MIRROR |
| P0 | `src/core/config.rs` | 1-50 | Config pattern to EXTEND with port settings |
| P1 | `src/sessions/handler.rs` | 20-60 | Handler pattern to FOLLOW for port allocation |
| P1 | `src/sessions/errors.rs` | 1-40 | Error pattern to EXTEND with port errors |
| P2 | `src/cli/commands.rs` | 60-100 | Display pattern to FOLLOW for port info |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [IANA Port Numbers](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml) | Dynamic/Private Ports | Port range selection guidelines |
| [Rust serde docs](https://docs.rs/serde/1.0/serde/) | Serialization | Session persistence with new fields |

---

## Patterns to Mirror

**SESSION_STRUCT_EXTENSION:**
```rust
// SOURCE: src/sessions/types.rs:5-20
// COPY THIS PATTERN:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub agent: String,
    pub status: SessionStatus,
    pub created_at: String,
    // ADD: port allocation fields here
}
```

**CONFIG_EXTENSION:**
```rust
// SOURCE: src/core/config.rs:5-15
// COPY THIS PATTERN:
#[derive(Debug, Clone)]
pub struct Config {
    pub shards_dir: PathBuf,
    pub log_level: String,
    // ADD: port configuration fields here
}
```

**PORT_CALCULATION:**
```rust
// SOURCE: src/sessions/operations.rs:25-30
// COPY THIS PATTERN:
pub fn calculate_port_range(session_index: u32) -> (u16, u16) {
    let base_port = 3000u16 + (session_index as u16 * 100);
    (base_port, base_port + 99)
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/sessions/errors.rs:5-20
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },
    // ADD: port-related errors here
}
```

**HANDLER_LOGGING:**
```rust
// SOURCE: src/sessions/handler.rs:25-35
// COPY THIS PATTERN:
info!(
    event = "session.create_started",
    branch = request.branch,
    agent = agent,
    command = agent_command
);
```

**CLI_DISPLAY:**
```rust
// SOURCE: src/cli/commands.rs:60-80
// COPY THIS PATTERN:
println!("┌──────────────────┬─────────┬─────────┬─────────────────────┐");
println!("│ Branch           │ Agent   │ Status  │ Created             │");
// ADD: port range column
```

---

## Files to Change

| File                             | Action | Justification                            |
| -------------------------------- | ------ | ---------------------------------------- |
| `src/sessions/types.rs`          | UPDATE | Add port allocation fields to Session    |
| `src/sessions/operations.rs`     | UPDATE | Add port allocation and tracking logic   |
| `src/sessions/handler.rs`        | UPDATE | Integrate port allocation in lifecycle   |
| `src/sessions/errors.rs`         | UPDATE | Add port-related error variants          |
| `src/core/config.rs`             | UPDATE | Add port configuration options           |
| `src/cli/commands.rs`            | UPDATE | Display port info in list/create output  |
| `src/core/ports/mod.rs`          | CREATE | Port allocation module (if needed)       |
| `src/core/ports/registry.rs`     | CREATE | Port registry for conflict detection    |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Network port scanning** - not validating if ports are actually free on the system
- **Port forwarding** - not setting up automatic port forwarding or proxying
- **Service discovery** - not implementing service registry or discovery mechanisms
- **Container integration** - not integrating with Docker port mapping
- **Remote port allocation** - only handling local development environment
- **Port persistence across reboots** - ports are session-scoped only

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `src/sessions/types.rs` (extend Session struct)

- **ACTION**: ADD port allocation fields to Session struct
- **IMPLEMENT**: `port_range_start: u16`, `port_range_end: u16`, `port_count: u16`
- **MIRROR**: `src/sessions/types.rs:5-20` - follow existing field pattern
- **IMPORTS**: No new imports needed (u16 is primitive)
- **GOTCHA**: Use u16 for ports (valid range 1-65535), add serde annotations
- **VALIDATE**: `cargo check` - types must compile

### Task 2: UPDATE `src/core/config.rs` (add port configuration)

- **ACTION**: ADD port configuration fields to Config struct
- **IMPLEMENT**: `default_port_count: u16`, `base_port_range: u16`
- **MIRROR**: `src/core/config.rs:5-15` - follow existing field pattern
- **DEFAULTS**: `default_port_count: 10`, `base_port_range: 3000`
- **GOTCHA**: Add environment variable support for configuration
- **VALIDATE**: `cargo check`

### Task 3: UPDATE `src/sessions/operations.rs` (port allocation logic)

- **ACTION**: ADD port allocation and conflict detection functions
- **IMPLEMENT**: `allocate_port_range()`, `find_next_available_range()`, `is_port_range_available()`
- **MIRROR**: `src/sessions/operations.rs:25-30` - extend existing port calculation
- **PATTERN**: Load existing sessions, find gaps in port allocations
- **GOTCHA**: Handle wraparound when port ranges are exhausted
- **VALIDATE**: `cargo test src/sessions/operations.rs`

### Task 4: UPDATE `src/sessions/errors.rs` (port-related errors)

- **ACTION**: ADD port allocation error variants
- **IMPLEMENT**: `PortRangeExhausted`, `PortAllocationFailed`
- **MIRROR**: `src/sessions/errors.rs:5-20` - follow existing error pattern
- **PATTERN**: Use thiserror with descriptive messages
- **VALIDATE**: `cargo check`

### Task 5: UPDATE `src/sessions/handler.rs` (integrate port allocation)

- **ACTION**: ADD port allocation to create_session workflow
- **IMPLEMENT**: Call port allocation before worktree creation
- **MIRROR**: `src/sessions/handler.rs:25-60` - follow existing handler pattern
- **PATTERN**: Add structured logging for port allocation events
- **GOTCHA**: Allocate ports early, clean up on failure
- **VALIDATE**: `cargo test src/sessions/handler.rs`

### Task 6: UPDATE `src/sessions/handler.rs` (port cleanup in destroy)

- **ACTION**: ADD port deallocation to destroy_session workflow
- **IMPLEMENT**: Free port range when destroying session
- **MIRROR**: `src/sessions/handler.rs:80-100` - follow existing cleanup pattern
- **PATTERN**: Log port deallocation events
- **VALIDATE**: `cargo test src/sessions/handler.rs`

### Task 7: UPDATE `src/cli/commands.rs` (display port information)

- **ACTION**: ADD port range column to list command output
- **IMPLEMENT**: Show port range in table format
- **MIRROR**: `src/cli/commands.rs:60-80` - follow existing table pattern
- **PATTERN**: Add port range to create command success message
- **GOTCHA**: Handle port range display formatting (e.g., "3000-3009")
- **VALIDATE**: `cargo run -- list` - verify table formatting

### Task 8: UPDATE `src/sessions/operations.rs` (environment variable export)

- **ACTION**: ADD function to generate port environment variables
- **IMPLEMENT**: `generate_port_env_vars()` for shell export
- **PATTERN**: Return Vec<(String, String)> for env var pairs
- **EXAMPLE**: `[("PORT_RANGE_START", "3000"), ("PORT_RANGE_END", "3009")]`
- **VALIDATE**: `cargo test src/sessions/operations.rs`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                | Test Cases                 | Validates      |
| ---------------------------------------- | -------------------------- | -------------- |
| `src/sessions/tests/port_allocation.rs`  | port range allocation, conflicts | Port allocation logic |
| `src/sessions/tests/operations.rs`       | port env vars, range validation | Port operations |
| `src/core/tests/config.rs`               | port config defaults | Configuration |

### Edge Cases Checklist

- [ ] Port range exhaustion (all ports allocated)
- [ ] Session destruction with missing port allocation
- [ ] Invalid port configuration (port count = 0)
- [ ] Port range overflow (base + count > 65535)
- [ ] Concurrent session creation (race conditions)
- [ ] Session file corruption with port data

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test sessions::operations::port && cargo test sessions::handler::port
```

**EXPECT**: All port-related tests pass

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, release build succeeds

### Level 4: INTEGRATION_VALIDATION

```bash
# Create multiple shards and verify port allocation
cargo run -- create test-shard-1 --agent claude
cargo run -- create test-shard-2 --agent kiro
cargo run -- list
cargo run -- destroy test-shard-1
cargo run -- destroy test-shard-2
```

**EXPECT**: Different port ranges allocated, proper cleanup

### Level 5: MANUAL_VALIDATION

1. Create 3 shards with different agents
2. Verify `shards list` shows different port ranges
3. Check session files contain port allocation data
4. Destroy middle shard, create new one - should reuse freed ports
5. Verify environment variables are available in worktree

---

## Acceptance Criteria

- [ ] Each shard gets unique, non-overlapping port range
- [ ] Port allocation persisted in session files
- [ ] Port ranges displayed in `shards list` and `shards create` output
- [ ] Port cleanup happens automatically during `shards destroy`
- [ ] Configurable default port count (default: 10 ports)
- [ ] Port allocation starts from configurable base (default: 3000)
- [ ] Environment variables available for services in worktree
- [ ] Graceful handling of port range exhaustion
- [ ] All existing functionality continues to work
- [ ] Level 1-4 validation commands pass with exit 0

---

## Completion Checklist

- [ ] Session struct extended with port fields
- [ ] Config struct extended with port settings
- [ ] Port allocation logic implemented and tested
- [ ] Port cleanup integrated into destroy workflow
- [ ] CLI output updated to show port information
- [ ] Environment variable generation implemented
- [ ] Error handling for port-related failures
- [ ] All unit tests pass
- [ ] Integration testing completed
- [ ] Documentation updated

---

## Risks and Mitigations

| Risk               | Likelihood   | Impact       | Mitigation                              |
| ------------------ | ------------ | ------------ | --------------------------------------- |
| Port range exhaustion | MEDIUM | HIGH | Implement wraparound and gap detection |
| Session file corruption | LOW | MEDIUM | Atomic file writes, validation on load |
| Race conditions in allocation | LOW | HIGH | File-based locking or sequential allocation |
| Environment variable conflicts | LOW | LOW | Use SHARD_ prefix for all variables |

---

## Notes

**Port Range Strategy**: Use 100-port blocks per shard (e.g., 3000-3099, 3100-3199) to provide plenty of room for microservices while keeping allocation simple.

**Environment Variables**: Export PORT_RANGE_START, PORT_RANGE_END, and PORT_COUNT to make port information easily accessible to services running in the shard.

**Future Enhancements**: Consider adding port usage monitoring, automatic port conflict detection, and integration with container orchestration tools.

**Backward Compatibility**: Existing sessions without port allocation will be handled gracefully by assigning ports on first access or during next operation.
