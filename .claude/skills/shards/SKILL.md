---
name: shards
description: Manage parallel AI development sessions in isolated Git worktrees. Use when creating isolated workspaces, managing multiple AI agents, checking session status, or cleaning up development environments.
allowed-tools: Bash(shards:*)
---

# Shards CLI - Parallel AI Development Manager

Shards creates isolated Git worktrees for parallel AI development sessions. Each shard runs in its own workspace with dedicated port ranges and process tracking.

## Core Commands

### Create a Shard
```bash
shards create <branch> [--agent <agent>] [--flags <flags>] [--terminal-type <type>] [--startup-command <command>]
```

Creates an isolated workspace with:
- New Git worktree in `~/.shards/worktrees/<project>/<branch>/`
- Unique port range (10 ports, starting from 3000)
- Native terminal with AI agent launched
- Process tracking (PID, name, start time)
- Session metadata saved to `~/.shards/sessions/`

**Supported agents**: claude, kiro, gemini, codex, aether
**Supported terminal types**: ghostty, iterm, terminal, native

**Note**: `--flags` accepts space-separated syntax: `--flags '--trust-all-tools'`

**Example**:
```bash
shards create feature-auth --agent kiro --terminal-type ghostty
shards create bug-fix-123 --agent claude --flags '--trust-all-tools'
```

### List All Shards
```bash
shards list
```

Shows table with:
- Branch name
- Agent type
- Status (active/stopped)
- Creation timestamp
- Port range allocation
- Process status (Running/Stopped with PID)
- Command executed (actual command with flags)

### Check Shard Status
```bash
shards status <branch>
```

Detailed view of specific shard:
- Branch and agent info
- Worktree path
- Process status and metadata
- Port range allocation

### Restart a Shard
```bash
shards restart <branch> [--agent <agent>]
```

Restarts agent process without destroying worktree:
- Kills existing process (if tracked)
- Launches new terminal with same or different agent
- Preserves worktree and uncommitted changes
- Updates session metadata with new process info

**Example**:
```bash
shards restart feature-auth
shards restart bug-fix-123 --agent claude
```

### Check Shard Status
```bash
shards status <branch>
```

Detailed view of specific shard:
- Branch and agent info
- Worktree path
- Process status and metadata
- Port range allocation

### Check Health Status
```bash
shards health [branch] [--json] [--all]
```

Shows health dashboard with:
- Process status (Working/Idle/Stuck/Crashed/Unknown)
- CPU and memory usage metrics
- Last activity tracking (planned feature)
- Summary statistics
- JSON output for programmatic use

**Example**:
```bash
shards health                    # Dashboard for current project
shards health feature-auth       # Specific shard details
shards health --json            # JSON output
```

### Destroy a Shard
```bash
shards destroy <branch>
```

Complete cleanup:
1. Kills tracked process (validates PID to prevent reuse attacks)
2. Removes Git worktree and branch
3. Deletes session file (frees port range)

### Cleanup Orphaned Resources
```bash
shards cleanup [--no-pid] [--stopped] [--older-than <days>] [--all]
```

Enhanced cleanup strategies:
- `--no-pid`: Clean sessions without PID tracking
- `--stopped`: Clean sessions with stopped processes  
- `--older-than N`: Clean sessions older than N days
- `--all`: Clean all orphaned resources (default)

**Example**:
```bash
shards cleanup                   # Clean all orphaned resources
shards cleanup --no-pid         # Clean sessions without PID tracking
shards cleanup --older-than 7   # Clean sessions older than 7 days
```

## Configuration

Hierarchical TOML config (later overrides earlier):
1. Hardcoded defaults
2. User config: `~/.shards/config.toml`
3. Project config: `./shards/config.toml`
4. CLI flags

**Example config**:
```toml
[agent]
default = "kiro"
startup_command = "kiro-cli chat"
flags = ""

[terminal]
preferred = "iterm2"

[agents.claude]
startup_command = "claude"
flags = "--yolo"
```

## When to Use Shards

**Perfect for**:
- Parallel feature development with multiple AI agents
- Isolated bug fixes without context switching
- Experimentation in separate environments
- Agent collaboration on different parts of a project

**Not suitable for**:
- Single-threaded development (use main branch)
- Non-Git projects (requires Git repository)
- Shared working directory needs

## Key Features

**Process Tracking**: Captures PID, process name, and start time at spawn. Validates identity before killing to prevent PID reuse attacks.

**Port Allocation**: Each shard gets unique port range (default: 10 ports from base 3000). Automatically freed on destroy.

**Session Persistence**: File-based storage in `~/.shards/sessions/<project-id>_<branch>.json` with all metadata for lifecycle management.

**Cross-Platform**: Works on macOS, Linux, and Windows with native terminal integration.

## Best Practices

**Naming**: Use descriptive branch names
- `feature-auth`, `bug-fix-123`, `refactor-api`
- Include issue numbers: `issue-456`, `ticket-789`
- Use agent prefixes: `kiro-debugging`, `claude-review`

**Lifecycle**: Always destroy shards when done to clean up resources

**Recovery**: Use `shards cleanup` after crashes or manual deletions

## Architecture Notes

- **Vertical slice architecture**: Features organized by domain
- **Handler/Operations pattern**: I/O separate from business logic
- **Structured logging**: JSON events for debugging
- **File-based persistence**: No database required

## Requirements

- Must run from within a Git repository
- Native terminal emulator required
- Rust 1.89.0+ for building from source
