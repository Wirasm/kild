# Project Structure

## Directory Layout
```
SHARDS/
├── src/                    # Rust source code (vertical slice architecture)
│   ├── main.rs            # CLI entry point
│   ├── lib.rs             # Library root with public exports
│   ├── cli/               # CLI interface slice
│   │   ├── mod.rs         # CLI module exports
│   │   ├── app.rs         # Clap application definition
│   │   └── commands.rs    # CLI command handlers
│   ├── core/              # Foundation infrastructure
│   │   ├── mod.rs         # Core module exports
│   │   ├── config.rs      # Application configuration
│   │   ├── logging.rs     # Structured logging setup
│   │   ├── errors.rs      # Base error traits
│   │   └── events.rs      # Application lifecycle events
│   ├── sessions/          # Feature slice: session lifecycle
│   │   ├── mod.rs         # Public API exports
│   │   ├── handler.rs     # I/O orchestration
│   │   ├── operations.rs  # Pure business logic
│   │   ├── types.rs       # Feature-specific types
│   │   └── errors.rs      # Feature-specific errors
│   ├── git/               # Feature slice: worktree management
│   │   ├── mod.rs         # Public API exports
│   │   ├── handler.rs     # Git I/O operations
│   │   ├── operations.rs  # Pure git logic
│   │   ├── types.rs       # Git data structures
│   │   └── errors.rs      # Git-specific errors
│   └── terminal/          # Feature slice: terminal launching
│       ├── mod.rs         # Public API exports
│       ├── handler.rs     # Terminal spawning
│       ├── operations.rs  # Terminal detection logic
│       ├── types.rs       # Terminal data structures
│       └── errors.rs      # Terminal-specific errors
├── .shards/               # Local worktrees directory (created at runtime)
│   └── <branch-name>/     # Individual shard worktrees
├── .kiro/                 # Kiro CLI configuration and steering docs
│   └── steering/          # Project steering documentation
├── target/                # Cargo build artifacts
├── Cargo.toml             # Rust project configuration
├── Cargo.lock             # Dependency lock file
└── README.md              # Project documentation
```

## Architecture Principles

### Vertical Slice Architecture
- **Feature-based organization**: Each feature (sessions, git, terminal) is self-contained
- **Handler/Operations pattern**: `handler.rs` for I/O orchestration, `operations.rs` for pure logic
- **Feature-specific types and errors**: Each slice defines its own domain types
- **Minimal coupling**: Features interact through well-defined interfaces

### Core Infrastructure
- **Foundation services**: Configuration, logging, base errors, lifecycle events
- **Shared only when needed**: Code moves to `shared/` only when 3+ features need it
- **No premature abstraction**: Prefer duplication over wrong abstraction

## File Naming Conventions
- **Rust modules**: Snake case (e.g., `handler.rs`, `operations.rs`, `types.rs`)
- **Branch names**: User-defined branch names for shards
- **Worktree directories**: `.shards/<branch-name>/` in repository root
- **Session files**: `.shards/sessions/<session-id>.json` (planned for persistence)

## Module Organization

### CLI Layer (`src/cli/`)
- **app.rs**: Clap application definition with command structure
- **commands.rs**: Command handlers that delegate to feature handlers
- **Thin layer**: Minimal logic, delegates to feature slices

### Core Infrastructure (`src/core/`)
- **config.rs**: Application configuration and environment setup
- **logging.rs**: Structured JSON logging with tracing
- **errors.rs**: Base error traits and common error handling
- **events.rs**: Application lifecycle events (startup, shutdown, errors)

### Feature Slices
Each feature follows the same pattern:
- **mod.rs**: Public API exports for the feature
- **handler.rs**: I/O orchestration with structured logging
- **operations.rs**: Pure business logic (no I/O, easily testable)
- **types.rs**: Feature-specific data structures
- **errors.rs**: Feature-specific error types with thiserror

## Configuration Files
- **Cargo.toml**: Rust project dependencies and metadata
- **.shards/sessions/**: Session persistence files (planned)
- **.gitignore**: Excludes build artifacts and local worktrees
- **No complex config files**: Keep configuration minimal and environment-based

## Documentation Structure
- **README.md**: User-facing documentation with usage examples
- **.kiro/steering/**: Project steering documentation
  - `architecture.md`: Complete architecture specification
  - `product.md`: Product requirements and objectives
  - `progress.md`: Current implementation status
  - `tech.md`: Technical stack and implementation details
  - `structure.md`: This file - project organization
  - `ai-instruction.md`: AI agent usage instructions
- **Inline documentation**: Rust doc comments for public APIs

## Build Artifacts
- **target/**: Cargo build output directory
  - `target/debug/`: Development builds
  - `target/release/`: Optimized release builds
- **Cargo.lock**: Dependency resolution lock file
- Build artifacts are excluded from version control

## Testing Strategy
- **Unit tests**: Collocated with code, especially in `operations.rs` modules
- **Integration tests**: Cross-feature workflows in `tests/` directory
- **Manual testing**: CLI command validation and platform-specific testing
