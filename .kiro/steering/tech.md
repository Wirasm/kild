# Technical Architecture

## Technology Stack
- **Language**: Rust (2024 edition)
- **CLI Framework**: clap 4.0 with derive macros
- **Git Operations**: git2 crate for worktree management
- **Terminal Integration**: Platform-specific terminal launching (osascript for macOS, gnome-terminal/konsole for Linux, Windows Terminal/cmd for Windows)
- **Session Storage**: File-based persistence in `.shards/sessions/` (planned)
- **Logging**: Structured JSON logging with tracing and tracing-subscriber
- **Error Handling**: thiserror for feature-specific error types
- **Cross-platform Support**: Conditional compilation for platform-specific features

## Architecture Overview
```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   CLI Parser    │───▶│  Sessions        │───▶│  Git Handler    │
│   (clap)        │    │  Handler         │    │  (git2)         │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         │                       ▼                       ▼
         │              ┌──────────────────┐    ┌─────────────────┐
         │              │  Terminal        │    │  Worktree       │
         │              │  Handler         │    │  (.shards/*)    │
         │              └──────────────────┘    └─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐    ┌──────────────────┐
│  Core Logging   │    │ Native Terminal  │
│  & Events       │    │ (agent process)  │
└─────────────────┘    └──────────────────┘
```

## Vertical Slice Architecture

### Feature Slices
- **sessions/**: Session lifecycle management with handler/operations pattern
- **git/**: Git worktree operations with structured logging
- **terminal/**: Cross-platform terminal launching with async spawning
- **cli/**: Command-line interface with clap integration

### Core Infrastructure
- **config.rs**: Application configuration with environment variables
- **logging.rs**: Structured JSON logging setup with tracing
- **errors.rs**: Base error traits and common error handling
- **events.rs**: Application lifecycle events (startup, shutdown, errors)

## Development Environment
- Rust 1.89.0 or later
- Git repository (required for worktree operations)
- Platform-specific terminal emulator

## Code Standards
- **Vertical slice architecture**: Features organized by domain, not layers
- **Handler/Operations pattern**: I/O orchestration separate from pure business logic
- **Structured logging**: Event-based logging with consistent naming conventions
- **Feature-specific errors**: thiserror-based error types with helpful messages
- **No unwrap/expect**: Explicit error handling with `?` operator
- **Cross-platform compatibility**: Conditional compilation for platform features

## Testing Strategy
- **Unit tests**: Collocated with code, especially in `operations.rs` modules
- **Integration tests**: Cross-feature workflows testing complete CLI commands
- **Manual testing**: Platform-specific terminal launching and Git operations

## Deployment Process
- Cargo build for local development
- Future: Binary releases for multiple platforms

## Performance Requirements
- Fast startup time for CLI operations
- Efficient Git operations for worktree management
- Minimal resource usage for session tracking
- Async terminal spawning to prevent blocking

## Security Considerations
- No sensitive data storage
- Safe file system operations with proper error handling
- Proper cleanup of temporary resources
- Atomic file operations for session persistence
