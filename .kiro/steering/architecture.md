# Architecture

## Vertical Slice Architecture

**Core Principle**: Organize by features, not layers. Each slice is self-contained with everything needed to understand and modify that feature.

**Note**: Project uses Cargo workspace structure with core library and CLI binary separated.

```
crates/
├── shards-core/       # Core library with all business logic
│   └── src/
│       ├── config/    # Foundation: configuration
│       ├── logging/   # Foundation: structured logging
│       ├── errors/    # Foundation: base error traits
│       ├── events/    # Foundation: application lifecycle
│       ├── sessions/  # Feature slice: session lifecycle
│       ├── git/       # Feature slice: worktree management
│       ├── terminal/  # Feature slice: terminal launching
│       ├── health/    # Feature slice: health monitoring
│       ├── cleanup/   # Feature slice: cleanup operations
│       ├── process/   # Feature slice: process management
│       └── files/     # Feature slice: file operations
├── shards/            # CLI binary
│   └── src/
│       ├── main.rs    # CLI entry point
│       ├── app.rs     # Clap application definition
│       ├── commands.rs # CLI command handlers
│       └── table.rs   # Table formatting utilities
└── shards-ui/         # Future: GPUI-based UI
```

### Feature Slice Structure
Each feature contains its complete implementation:

```
crates/shards-core/src/sessions/
├── mod.rs             # Public API exports
├── handler.rs         # Orchestration (uses infrastructure)
├── operations.rs      # Pure business logic (no I/O)
├── types.rs           # Feature-specific types
├── errors.rs          # Feature-specific errors
└── tests.rs           # Feature tests
```

**Key Pattern**: `handler.rs` orchestrates I/O, `operations.rs` contains pure logic. This makes business logic trivially testable.

### Core Infrastructure
Universal foundation that exists before features:

```
crates/shards-core/src/
├── config/            # Application configuration
├── logging/           # Structured logging setup
├── errors/            # Base error traits
└── events/            # Application lifecycle
```

**Rule**: If removing any feature slice would still require this code, it belongs in core infrastructure.

### Three-Feature Rule
Code moves to `shared/` only when three features need it. Until then, duplicate it. Prefer coupling to feature over premature abstraction.

## Logging Strategy

**Event Naming Convention**: `{domain}.{component}.{action}_{state}`

### Standard Events

| Event | When |
|-------|------|
| `session.create_started` | Handler begins |
| `session.create_completed` | Success |
| `session.create_failed` | Failure |
| `git.worktree.create_completed` | Worktree added |
| `terminal.spawn_completed` | Terminal launched |

### Implementation Pattern

```rust
use tracing::{info, error};

pub fn create_session(name: &str, command: &str) -> Result<Session, SessionError> {
    info!(
        event = "session.create_started",
        name = name,
        command = command
    );
    
    match operations::create_session(name, command) {
        Ok(session) => {
            info!(
                event = "session.create_completed",
                session_id = session.id,
                name = name
            );
            Ok(session)
        }
        Err(e) => {
            error!(
                event = "session.create_failed",
                name = name,
                error = %e,
                error_type = std::any::type_name::<SessionError>()
            );
            Err(e)
        }
    }
}
```

### Required Fields by Context

- **Session ops**: `name`, `session_id`, `command`
- **Git ops**: `repo_path`, `branch`, `worktree_path`
- **Terminal ops**: `terminal_type`, `command`
- **Errors**: `error`, `error_type`

### Log Levels

| Level | Use For |
|-------|---------|
| `error!` | Operation failed, needs attention |
| `warn!` | Unexpected but recoverable |
| `info!` | Key lifecycle events |
| `debug!` | Detailed operation info |
| `trace!` | Very verbose (command output, paths) |

### Structured Logging Setup

```rust
// crates/shards-core/src/logging/mod.rs
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(false)
                .with_span_list(false)
        )
        .with(EnvFilter::from_default_env().add_directive("shards=info".parse().unwrap()))
        .init();
}
```

**Key Benefits**:
- Grep-friendly: `grep "_failed"` finds all failures
- Hierarchical: `grep "session\."` finds all session events
- AI-parseable: Structured JSON for log analysis
- OpenTelemetry-aligned: Standard semantic conventions

## Handler/Operations Pattern

### Handler Layer (I/O Orchestration)
```rust
// crates/shards-core/src/sessions/handler.rs
pub fn create_session(name: &str, command: &str) -> Result<Session, SessionError> {
    info!(event = "session.create_started", name = name);
    
    // 1. Validate input (pure)
    let validated = operations::validate_session_request(name, command)?;
    
    // 2. Check existing (I/O)
    if registry::session_exists(&validated.name)? {
        return Err(SessionError::AlreadyExists { name: validated.name });
    }
    
    // 3. Create worktree (I/O)
    let worktree = git::create_worktree(&validated.name)?;
    
    // 4. Launch terminal (I/O)
    terminal::spawn(&validated.command, &worktree.path)?;
    
    // 5. Save session (I/O)
    let session = registry::save_session(validated, worktree)?;
    
    info!(event = "session.create_completed", session_id = session.id);
    Ok(session)
}
```

### Operations Layer (Pure Logic)
```rust
// crates/shards-core/src/sessions/operations.rs
pub fn validate_session_request(name: &str, command: &str) -> Result<ValidatedRequest, SessionError> {
    if name.is_empty() {
        return Err(SessionError::InvalidName);
    }
    
    if command.is_empty() {
        return Err(SessionError::InvalidCommand);
    }
    
    Ok(ValidatedRequest {
        name: name.to_string(),
        command: command.to_string(),
    })
}

pub fn calculate_port_range(session_index: u32) -> (u16, u16) {
    let base_port = 3000 + (session_index * 100);
    (base_port, base_port + 99)
}
```

**Benefits**:
- `operations.rs` is trivially testable (no I/O)
- `handler.rs` orchestrates the workflow
- Clear separation of concerns
- AI can understand the flow easily

## Error Handling

Feature-specific errors with grep-able variants:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },
    
    #[error("Session '{name}' not found")]
    NotFound { name: String },
    
    #[error("Invalid session name")]
    InvalidName,
    
    #[error("Git operation failed: {source}")]
    GitError { 
        #[from]
        source: git2::Error 
    },
}
```

**No `unwrap()` or `expect()` in production code** - use `?` or explicit error handling.

## Testing Strategy

### Unit Tests (Collocated)
```rust
// In operations.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_session_request_success() {
        let result = validate_session_request("test", "echo hello");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_calculate_port_range() {
        assert_eq!(calculate_port_range(0), (3000, 3099));
        assert_eq!(calculate_port_range(1), (3100, 3199));
    }
}
```

### Integration Tests
```rust
// tests/session_lifecycle.rs
#[test]
fn test_create_and_cleanup_session() {
    // Test sessions + git + terminal integration
}
```

## Migration Path

Current → Target:
1. Extract `core/logging.rs` with structured logging
2. Create feature slices (`sessions/`, `git/`, `terminal/`)
3. Split each feature into `handler.rs` + `operations.rs`
4. Add comprehensive logging to all handlers
5. Move tests to be collocated with code

**No backwards compatibility needed** - we can break things to get architecture right.
