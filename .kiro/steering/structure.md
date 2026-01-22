# Project Structure

## Directory Layout
```
SHARDS/
├── crates/                # Cargo workspace crates
│   ├── shards-core/       # Core library with all business logic
│   │   ├── src/
│   │   │   ├── lib.rs     # Library root with public exports
│   │   │   ├── config/    # Foundation: configuration
│   │   │   │   └── mod.rs # Application configuration
│   │   │   ├── logging/   # Foundation: structured logging
│   │   │   │   └── mod.rs # Logging setup
│   │   │   ├── errors/    # Foundation: base error traits
│   │   │   │   └── mod.rs # Error definitions
│   │   │   ├── events/    # Foundation: application lifecycle
│   │   │   │   └── mod.rs # Lifecycle events
│   │   │   ├── sessions/  # Feature slice: session lifecycle
│   │   │   │   ├── mod.rs         # Public API exports
│   │   │   │   ├── handler.rs     # I/O orchestration
│   │   │   │   ├── operations.rs  # Pure business logic
│   │   │   │   ├── types.rs       # Feature-specific types
│   │   │   │   └── errors.rs      # Feature-specific errors
│   │   │   ├── git/       # Feature slice: worktree management
│   │   │   │   ├── mod.rs         # Public API exports
│   │   │   │   ├── handler.rs     # Git I/O operations
│   │   │   │   ├── operations.rs  # Pure git logic
│   │   │   │   ├── types.rs       # Git data structures
│   │   │   │   └── errors.rs      # Git-specific errors
│   │   │   ├── terminal/  # Feature slice: terminal launching
│   │   │   │   ├── mod.rs         # Public API exports
│   │   │   │   ├── handler.rs     # Terminal spawning
│   │   │   │   ├── operations.rs  # Terminal detection logic
│   │   │   │   ├── types.rs       # Terminal data structures
│   │   │   │   └── errors.rs      # Terminal-specific errors
│   │   │   ├── health/    # Feature slice: health monitoring
│   │   │   ├── cleanup/   # Feature slice: cleanup operations
│   │   │   ├── process/   # Feature slice: process management
│   │   │   └── files/     # Feature slice: file operations
│   │   └── Cargo.toml     # Core library dependencies
│   ├── shards/            # CLI binary
│   │   ├── src/
│   │   │   ├── main.rs    # CLI entry point
│   │   │   ├── app.rs     # Clap application definition
│   │   │   ├── commands.rs # CLI command handlers
│   │   │   └── table.rs   # Table formatting utilities
│   │   └── Cargo.toml     # CLI binary dependencies
│   └── shards-ui/         # Future: GPUI-based UI (placeholder)
│       └── Cargo.toml     # UI dependencies
├── .shards/               # Local worktrees directory (created at runtime)
│   └── <branch-name>/     # Individual shard worktrees
├── .kiro/                 # Kiro CLI configuration and steering docs
│   └── steering/          # Project steering documentation
├── target/                # Cargo build artifacts
├── Cargo.toml             # Workspace manifest
├── Cargo.lock             # Dependency lock file
└── README.md              # Project documentation
```

## Architecture Principles

### Cargo Workspace Structure
- **Multi-crate workspace**: Core library (`shards-core`) + CLI binary (`shards`) + UI placeholder (`shards-ui`)
- **Clear separation**: Business logic in core library, CLI implementation in binary crate
- **Code sharing**: Both CLI and future UI can depend on the same core library

### Vertical Slice Architecture
- **Feature-based organization**: Each feature (sessions, git, terminal, health, cleanup, etc.) is self-contained
- **Handler/Operations pattern**: `handler.rs` for I/O orchestration, `operations.rs` for pure logic
- **Feature-specific types and errors**: Each slice defines its own domain types
- **Minimal coupling**: Features interact through well-defined interfaces

### Core Infrastructure
- **Foundation services**: Configuration, logging, base errors, lifecycle events
- **Shared only when needed**: Code moves to shared utilities only when 3+ features need it
- **No premature abstraction**: Prefer duplication over wrong abstraction

## File Naming Conventions
- **Rust modules**: Snake case (e.g., `handler.rs`, `operations.rs`, `types.rs`)
- **Branch names**: User-defined branch names for shards
- **Worktree directories**: `.shards/<branch-name>/` in repository root
- **Session files**: `.shards/sessions/<session-id>.json` (planned for persistence)

## Module Organization

### CLI Binary (`crates/shards/src/`)
- **main.rs**: Entry point, initializes logging and runs CLI
- **app.rs**: Clap application definition with command structure
- **commands.rs**: Command handlers that delegate to `shards-core` library
- **table.rs**: Table formatting utilities for CLI output
- **Thin layer**: Minimal logic, delegates to core library

### Core Library (`crates/shards-core/src/`)
- **lib.rs**: Library root with public API exports
- **config/**: Application configuration and environment setup
- **logging/**: Structured JSON logging with tracing
- **errors/**: Base error traits and common error handling
- **events/**: Application lifecycle events (startup, shutdown, errors)

### Feature Slices (in `shards-core`)
Each feature follows the same pattern:
- **mod.rs**: Public API exports for the feature
- **handler.rs**: I/O orchestration with structured logging
- **operations.rs**: Pure business logic (no I/O, easily testable)
- **types.rs**: Feature-specific data structures
- **errors.rs**: Feature-specific error types with thiserror

Feature slices include: `sessions/`, `git/`, `terminal/`, `health/`, `cleanup/`, `process/`, `files/`

## Configuration Files
- **Cargo.toml** (root): Workspace manifest with shared dependencies
- **crates/shards-core/Cargo.toml**: Core library dependencies
- **crates/shards/Cargo.toml**: CLI binary dependencies
- **crates/shards-ui/Cargo.toml**: UI placeholder dependencies
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
