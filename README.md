<p align="center">
  <img src="assets/shards-hero.png" alt="Shards - Manage parallel AI development agents" />
</p>

# Shards

Manage parallel AI development agents in isolated Git worktrees.

## Overview

Shards eliminates context switching between scattered terminals when working with multiple AI coding assistants. Each shard runs in its own Git worktree with automatic branch creation, allowing you to manage parallel AI development sessions from a centralized interface.

## Features

- **Isolated Worktrees**: Each shard gets its own Git worktree with unique `shard_<hash>` branch
- **Native Terminal Integration**: Launches AI agents in native terminal windows
- **Session Tracking**: Persistent registry tracks all active shards
- **Cross-Platform**: Works on macOS, Linux, and Windows
- **Agent-Friendly**: Designed for programmatic use by AI assistants

## Installation

```bash
cargo install --path .
```

## Usage

### Start a new shard
```bash
shards start <name> <agent-command>

# Examples:
shards start kiro-session "kiro-cli chat"
shards start claude-work "claude-code"
shards start gemini-task "gemini-cli"
```

### List active shards
```bash
shards list
```

### Get shard information
```bash
shards info <name>
```

### Stop a shard
```bash
shards stop <name>
```

### Clean up orphaned shards
```bash
shards cleanup
```

## How It Works

1. **Worktree Creation**: Creates a new Git worktree in `.shards/<name>` with a unique branch
2. **Agent Launch**: Launches the specified agent command in a native terminal window
3. **Session Tracking**: Records session metadata in `~/.shards/registry.json`
4. **Lifecycle Management**: Provides commands to monitor, stop, and clean up sessions

## Requirements

- Rust 1.89.0 or later
- Git repository (shards must be run from within a Git repository)
- Native terminal emulator (Terminal.app on macOS, gnome-terminal/konsole on Linux, etc.)

## Agent Integration

Shards is designed to be used by AI agents themselves. For example, an AI assistant can create a new shard for a specific task:

```bash
# AI agent creates isolated workspace for bug fix
shards start bug-fix-123 "kiro-cli chat"
```

This enables parallel AI workflows without manual terminal management.

## Architecture

- **CLI**: Built with clap for structured command parsing
- **Git Operations**: Uses git2 crate for worktree management
- **Terminal Launching**: Platform-specific terminal integration
- **Session Registry**: JSON-based persistent storage
- **Cross-Platform**: Conditional compilation for platform features

## License

MIT License - see LICENSE file for details.
