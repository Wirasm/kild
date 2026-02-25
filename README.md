<p align="center">
  <img src="assets/kild-hero.png" alt="KILD - Manage parallel AI development agents" />
</p>

# KILD

Manage parallel AI development agents in isolated Git worktrees.

## Overview

KILD eliminates context switching between scattered terminals when working with multiple AI coding assistants. Each kild runs in its own Git worktree with automatic branch creation, allowing you to manage parallel AI development sessions from a centralized interface.

## Features

- **Isolated Worktrees**: Each kild gets its own Git worktree with unique `kild/<branch>` branch
- **Dual Runtime Modes**: Choose between external terminal windows or daemon-owned PTYs
- **Session Persistence**: Daemon-managed sessions survive terminal restarts (experimental)
- **Agent Teams**: Daemon sessions support Claude Code agent teams via tmux-compatible shim
- **Session Tracking**: Persistent registry tracks all active kilds
- **Cross-Platform**: Works on macOS, Linux, and Windows
- **Agent-Friendly**: Designed for programmatic use by AI assistants
- **Visual Verification**: Companion `kild-peek` tool for capturing screenshots and inspecting native UI (see `.claude/skills/kild-peek/SKILL.md`)

## GUI (Experimental)

A native graphical interface is under development using GPUI. The UI provides visual kild management as an alternative to the CLI.

```bash
# Build and run the experimental GPUI GUI
cargo run -p kild-ui
```

The GUI currently supports:
- Multi-project management: Project rail (48px) with icon-based switcher and badge counts
- Sidebar navigation: Kilds grouped by Active/Stopped status with nested terminal tab names
- KILD listing with status indicators (running, stopped, git dirty state)
- Creating new kilds with agent selection
- Opening new agents in existing kilds
- Stopping agents without destroying kilds
- Destroying kilds with confirmation dialog
- Quick actions: Copy path to clipboard, open in editor, focus terminal window
- Live terminal rendering with multiple tabs per kild
- Keyboard navigation: Ctrl+1-9 (jump to kild by index), Cmd+Shift+[/] (cycle workspaces), Cmd+J/K (next/prev kild), Cmd+D (toggle Control/Dashboard view), Ctrl+Escape (move focus from terminal to sidebar) — all configurable via `~/.kild/keybindings.toml`

See the [PRD](.claude/PRPs/prds/gpui-native-terminal-ui.prd.md) for the development roadmap.

## Installation

```bash
cargo install --path crates/kild
```

## Usage

### Global flags

```bash
# Enable verbose logging output (shows JSON logs)
kild -v <command>
kild --verbose <command>
```

### Create a new kild
```bash
kild create <branch> --agent <agent>

# Examples:
kild create kiro-session --agent kiro
kild create claude-work --agent claude
kild create gemini-task --agent gemini

# Branch names with slashes are supported
kild create feature/auth --agent claude
kild create bugfix/login-error --agent kiro

# Add a description with --note
kild create feature-auth --agent claude --note "Implementing JWT authentication"

# Enable autonomous mode (skip all permission prompts)
kild create my-branch --agent claude --yolo

# Create without launching an agent (opens bare terminal with $SHELL)
kild create debug-session --no-agent

# Launch in daemon-owned PTY (experimental)
kild create my-branch --agent claude --daemon

# Force external terminal window (override config default)
kild create my-branch --agent claude --no-daemon

# Run from project root without creating a worktree (for supervisory sessions)
kild create honryu --agent claude --daemon --main
```

### List active kilds
```bash
kild list

# Machine-readable JSON output (object with sessions array and fleet_summary)
kild list --json
```

### Navigate to a kild (shell integration)
```bash
# Print worktree path
kild cd <branch>

# Shell function for quick navigation
kcd() { cd "$(kild cd "$1")"; }

# Usage with shell function
kcd my-branch
```

### Open a new agent in an existing kild
```bash
# Open with same agent (additive - doesn't close existing terminals)
# Auto-detects runtime mode (daemon vs terminal) from how the session was created
kild open <branch>

# Open with different agent
kild open <branch> --agent <agent>

# Resume previous agent session (restore conversation context)
# Currently only supported by Claude Code agent
kild open <branch> --resume
kild open <branch> -r  # Short form

# Enable autonomous mode (skip all permission prompts)
kild open <branch> --yolo

# Open bare terminal with $SHELL instead of an agent
kild open <branch> --no-agent

# Override: force daemon-owned PTY (ignores session's stored mode)
kild open <branch> --daemon

# Override: force external terminal window (ignores session's stored mode)
kild open <branch> --no-daemon

# Open agents in all stopped kilds
kild open --all

# Open all stopped kilds with specific agent
kild open --all --agent <agent>

# Resume all stopped kilds with previous session context
kild open --all --resume

# Open all stopped kilds with autonomous mode enabled
kild open --all --yolo

# Open bare terminals in all stopped kilds
kild open --all --no-agent

# Open daemon session without launching a Ghostty window (for programmatic use)
kild open <branch> --daemon --no-attach

# Open and resume without attaching (brain reopening workers headlessly)
kild open <branch> --daemon --no-attach --resume
```

### Open kild in code editor
```bash
# Open worktree in editor
# Precedence: CLI flag > config > $VISUAL > $EDITOR > OS default > PATH scan
kild code <branch>

# Use specific editor (CLI override has highest priority)
kild code <branch> --editor vim

# Configure default editor in ~/.kild/config.toml or ./.kild/config.toml
# [editor]
# default = "code"
# flags = "--new-window"
# terminal = false  # Set to true for terminal editors (nvim, vim, helix)
```

### Focus on a kild
```bash
# Bring terminal window to foreground
kild focus <branch>
```

**Note:** Daemon-managed sessions automatically open an attach window. If auto-attach fails, use `kild attach <branch>` to connect manually.

### Hide a kild
```bash
# Minimize/hide terminal window
kild hide <branch>

# Hide all active kild windows (skips daemon-managed sessions)
kild hide --all
```

**Note:** Daemon-managed sessions have no persistent window to hide (attach windows are ephemeral). Use `kild attach <branch>` to reconnect if needed.

### View git changes in a kild
```bash
# Show uncommitted changes
kild diff <branch>

# Show only staged changes
kild diff <branch> --staged

# Show diffstat summary
kild diff <branch> --stat
```

### Show recent commits
```bash
# Show last 10 commits (default)
kild commits <branch>

# Show last 5 commits
kild commits <branch> -n 5
kild commits <branch> --count 5
```

### View branch health
```bash
# Show branch health and merge readiness
kild stats <branch>

# JSON output
kild stats <branch> --json

# Override base branch
kild stats <branch> --base dev

# View health for all kilds (fleet summary)
kild stats --all

# JSON output for all kilds
kild stats --all --json
```

### Detect file overlaps
```bash
# Detect when multiple kilds modify the same files
kild overlaps

# JSON output
kild overlaps --json

# Override base branch
kild overlaps --base dev
kild overlaps -b dev
```

### Show PR status
```bash
# Show cached PR status
kild pr <branch>

# Force refresh from GitHub
kild pr <branch> --refresh

# Machine-readable JSON output
kild pr <branch> --json
```

### Daemon management (experimental)
```bash
# Start daemon in background
kild daemon start

# Start daemon in foreground (for debugging)
kild daemon start --foreground

# Stop running daemon
kild daemon stop

# Show daemon status
kild daemon status
kild daemon status --json

# Attach to daemon-managed session (if auto-attach window was closed)
kild attach <branch>
# Press Ctrl+C to detach
```

**Note**: Daemon mode is experimental (Phase 1b). The daemon runtime supports background and foreground modes, auto-start via config, scrollback replay on attach, PTY exit notification with automatic session state updates, and works with both `kild create` and `kild open` commands. When creating or opening daemon sessions, KILD automatically spawns a terminal attach window for immediate visual feedback. Daemon sessions automatically enable Claude Code agent teams by injecting a tmux-compatible shim.

### Inject a message to a running worker
```bash
# Send text to a daemon worker's PTY stdin (works with all agents)
kild inject <branch> "implement the auth module"

# Force Claude Code inbox protocol instead of PTY stdin
kild inject <branch> "implement the auth module" --inbox
```

**Note**: For Claude daemon sessions, inject uses the inbox polling protocol by default (message delivered as a new user turn within ~1s). For all other agents, it writes to PTY stdin. The worker should be idle before injecting.

### Inspect fleet dropbox state
```bash
# Show dropbox protocol state for a worker session
kild inbox <branch>

# Show all fleet sessions in a table
kild inbox --all

# Filter output
kild inbox <branch> --task    # task content only
kild inbox <branch> --report  # report content only
kild inbox <branch> --status  # ack status line only
kild inbox <branch> --json    # machine-readable JSON
```

### Generate fleet context for agent bootstrapping
```bash
# Output protocol + current task + fleet status as a markdown blob
kild prime <branch>

# Fleet status table only (compact)
kild prime <branch> --status

# Machine-readable JSON output
kild prime <branch> --json

# Fleet-wide: concatenated prime blobs for all fleet sessions
kild prime --all

# Fleet-wide: single deduplicated fleet status table
kild prime --all --status

# Fleet-wide: JSON array of per-session prime contexts
kild prime --all --json
```

**Note**: Returns an error if fleet mode is not active. Designed for use in brain→worker injection: `kild inject worker "$(kild prime worker)"`.

### Manage the project registry
```bash
# Register a git repo in the project registry
kild project add <path>
kild project add <path> --name "My Repo"

# List all registered projects (* marks active)
kild project list
kild project list --json

# Show details for a project (accepts path or hex project ID)
kild project info <id|path>

# Set the active project
kild project default <id|path>

# Remove a project from the registry
kild project remove <id|path>
```

### Stop a kild
```bash
# Stop agent, preserve worktree
kild stop <branch>

# Stop all running kilds
kild stop --all
```

### Get kild information
```bash
kild status <branch>

# Machine-readable JSON output
kild status <branch> --json
```

### Destroy a kild
```bash
# Destroy with safety checks (blocks on uncommitted changes, warns on unpushed commits)
kild destroy <branch>

# Force destroy (bypass all git safety checks)
kild destroy <branch> --force

# Destroy all kilds (with confirmation prompt and safety checks)
kild destroy --all

# Force destroy all (skip confirmation and all git safety checks)
kild destroy --all --force
```


### Clean up orphaned kilds
```bash
kild cleanup
```

## Configuration

KILD uses a hierarchical TOML configuration system:

- **User config**: `~/.kild/config.toml` (global settings)
- **Project config**: `./.kild/config.toml` (project-specific settings)
- **User keybindings**: `~/.kild/keybindings.toml` (UI keyboard shortcuts)
- **Project keybindings**: `./.kild/keybindings.toml` (project-specific overrides)
- **Defaults**: Built-in sensible defaults

See `.kild/config.example.toml` for all config options. Keybindings follow the same hierarchy — project overrides user, missing keys fall back to defaults.

### Key Configuration Features

**File Include Patterns**: By default, KILD copies certain files to new worktrees even if gitignored:
- `.env*` - Environment files
- `*.local.json` - Local config files
- `.claude/**` - Claude AI context files
- `.cursor/**` - Cursor AI context files

Configure additional patterns in `[include_patterns]` section. Your patterns extend the defaults.

**Agent Settings**: Configure default agent, startup commands, and flags per agent.

**Terminal Preferences**: Set preferred terminal emulator (Ghostty, iTerm2, Terminal.app on macOS; Alacritty on Linux).

**Editor Settings**: Configure default editor for `kild code` command with optional flags and terminal mode for terminal-based editors.

**Daemon Runtime**: Control whether sessions run in daemon-owned PTYs by default:
```toml
[daemon]
enabled = false      # Use daemon mode by default
auto_start = true    # Auto-start daemon when needed
```

## How It Works

1. **Worktree Creation**: Creates a new Git worktree in `.kild/<name>` with a unique branch
2. **File Copying**: Copies configured patterns (env files, AI context) to worktree
3. **Agent Launch**: Launches the specified agent command in a native terminal window
4. **Session Tracking**: Records session metadata in `~/.kild/sessions/<session>/`
5. **Lifecycle Management**: Provides commands to monitor, stop, and clean up sessions

## Requirements

- Rust 1.89.0 or later
- Git repository (kild must be run from within a Git repository)
- Native terminal emulator (Ghostty/iTerm2/Terminal.app on macOS, Alacritty + Hyprland on Linux)

## Agent Integration

KILD is designed to be used by AI agents themselves. For example, an AI assistant can create a new kild for a specific task:

```bash
# AI agent creates isolated workspace for bug fix
kild create bug-fix-123 --agent claude
```

This enables parallel AI workflows without manual terminal management.

## Architecture

- **CLI**: Built with clap for structured command parsing
- **Git Operations**: Uses git2 crate for worktree management
- **Terminal Launching**: Platform-specific terminal integration
- **Session Registry**: JSON-based persistent storage
- **Cross-Platform**: Conditional compilation for platform features

## License

Apache License 2.0 — free to use, modify, and distribute.

The name "KILD", logo, and associated branding are trademarks of Widinglabs OÜ and are not covered by the Apache 2.0 license. See [LICENSE.md](LICENSE.md) for details.
