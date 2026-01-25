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

## GUI (Experimental)

A native graphical interface is under development using GPUI. The UI provides visual shard management as an alternative to the CLI.

```bash
# Build and run the experimental GPUI GUI
cargo run -p shards-ui
```

The GUI currently supports:
- Shard listing with status indicators
- Creating new shards with agent selection
- Opening new agents in existing shards
- Stopping agents without destroying shards
- Destroying shards with confirmation dialog

See the [PRD](.claude/PRPs/prds/gpui-native-terminal-ui.prd.md) for the development roadmap.

## Installation

```bash
cargo install --path .
```

## Usage

### Global flags

```bash
# Suppress JSON log output (show only user-facing output)
shards -q <command>
shards --quiet <command>
```

### Create a new shard
```bash
shards create <branch> --agent <agent>

# Examples:
shards create kiro-session --agent kiro
shards create claude-work --agent claude
shards create gemini-task --agent gemini

# Add a description with --note
shards create feature-auth --agent claude --note "Implementing JWT authentication"
```

### List active shards
```bash
shards list

# Machine-readable JSON output
shards list --json
```

### Navigate to a shard (shell integration)
```bash
# Print worktree path
shards cd <branch>

# Shell function for quick navigation
scd() { cd "$(shards cd "$1")"; }

# Usage with shell function
scd my-branch
```

### Open a new agent in an existing shard
```bash
# Open with same agent (additive - doesn't close existing terminals)
shards open <branch>

# Open with different agent
shards open <branch> --agent <agent>
```

### Stop a shard
```bash
# Stop agent, preserve worktree
shards stop <branch>
```

### Get shard information
```bash
shards status <branch>

# Machine-readable JSON output
shards status <branch> --json
```

### Destroy a shard
```bash
shards destroy <branch>

# Force destroy (bypass git uncommitted changes check)
shards destroy <branch> --force
```

### Note on deprecated commands

The `restart` command is deprecated. Use `open` instead:
```bash
# Old (deprecated, still works with warning)
shards restart <branch>

# New (preferred)
shards open <branch>
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
