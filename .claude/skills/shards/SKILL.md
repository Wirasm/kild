---
name: shards
description: |
  Create and manage parallel AI development sessions in isolated Git worktrees.

  TRIGGERS - Use this skill when user says "create a shard", "spin up shards",
  "use shards for this", "create worktrees for features", "run multiple agents",
  "isolated workspace", "shards create", "shards list", "shards health",
  "destroy the shard", "clean up shards", "check shard status".

  Shards creates isolated Git worktrees where AI agents work independently without
  affecting your main branch. Each shard gets its own terminal window, port range,
  and process tracking.

  EXAMPLES

  User says "Create a shard for the auth feature"
  Command - shards create feature-auth --agent claude
  Result - Creates worktree at ~/.shards/worktrees/<project>/feature-auth/ and opens Claude in new terminal

  User says "Show me all active shards"
  Command - shards list
  Result - Table showing branch, agent, status, ports, and process info

  User says "Create 3 shards for feature-a, feature-b, and bug-fix"
  Commands - Run shards create for each branch
  Result - Three isolated worktrees, each with its own agent in separate terminals

  User says "Check health of my shards"
  Command - shards health
  Result - Dashboard with CPU, memory, and status for all shards

allowed-tools: Bash, Read, Glob, Grep
---

# Shards CLI - Parallel AI Development Manager

Shards creates isolated Git worktrees for parallel AI development sessions. Each shard runs in its own workspace with dedicated port ranges and process tracking.

## Core Commands

### Create a Shard
```bash
shards create <branch> [--agent <agent>] [--flags <flags>] [--terminal <terminal>]
```

Creates an isolated workspace with:
- New Git worktree in `~/.shards/worktrees/<project>/<branch>/`
- Unique port range (10 ports, starting from 3000)
- Native terminal with AI agent launched
- Process tracking (PID, name, start time)
- Session metadata saved to `~/.shards/sessions/`

**Supported agents** - claude, kiro, gemini, codex, aether
**Supported terminals** - ghostty, iterm, terminal, native

**Examples**
```bash
shards create feature-auth --agent kiro --terminal ghostty
shards create bug-fix-123 --agent claude
```

### Autonomous Mode (YOLO / Trust All Tools)

Each agent has its own flag for skipping permission prompts. Pass via `--flags`:

**Claude Code** - `--dangerously-skip-permissions`
```bash
shards create feature-x --agent claude --flags '--dangerously-skip-permissions'
```

**Kiro CLI** - `--trust-all-tools` (or `--trust-tools <list>` for specific tools)
```bash
shards create feature-x --agent kiro --flags '--trust-all-tools'
```

**Codex CLI** - `--yolo` / `--dangerously-bypass-approvals-and-sandbox`

**IMPORTANT**: `--full-auto` is NOT fully autonomous. It still prompts for:
- File writes outside the workspace
- Commands requiring network access

For true autonomous mode, use `--yolo` (alias for `--dangerously-bypass-approvals-and-sandbox`):
```bash
# TRUE autonomous mode (no approval prompts)
shards create feature-x --agent codex --flags '--yolo'

# Equivalent long form
shards create feature-x --agent codex --flags '--dangerously-bypass-approvals-and-sandbox'

# Semi-autonomous (still prompts for some operations)
shards create feature-x --agent codex --flags '--full-auto'
```

Or set in config for persistent use:
```toml
# ~/.shards/config.toml
[agents.claude]
flags = "--dangerously-skip-permissions"

[agents.kiro]
flags = "--trust-all-tools"

[agents.codex]
flags = "--yolo"  # or "--dangerously-bypass-approvals-and-sandbox"
```

**Codex v0.20+ Note**: If `--yolo` still prompts, you may need to mark the workspace as trusted.
See: https://github.com/openai/codex/discussions/2138

### List All Shards
```bash
shards list
```

Shows table with branch, agent, status, timestamps, port range, process status, and command.

### Open a New Agent in a Shard
```bash
shards open <branch> [--agent <agent>]
```

Opens a new agent terminal in an existing shard. This is **additive** - it doesn't close existing terminals, allowing multiple agents to work in the same shard.

### Stop a Shard
```bash
shards stop <branch>
```

Stops the agent process and closes the terminal, but preserves the shard (worktree and uncommitted changes remain). Can be reopened later with `shards open`.

### Restart a Shard (Deprecated)
```bash
shards restart <branch> [--agent <agent>]
```

**DEPRECATED**: Use `shards open` instead. This command is kept for backward compatibility but will show a deprecation warning.

### Status (Detailed View)
```bash
shards status <branch>
```

Shows detailed info for a specific shard including worktree path, process metadata, port allocation.

### Health Monitoring
```bash
shards health [branch] [--json] [--watch] [--interval <seconds>]
```

Shows health dashboard with process status, CPU/memory metrics, and summary statistics.

### Destroy a Shard
```bash
shards destroy <branch> [--force]
```

Completely removes a shard - closes terminal, kills process, removes worktree and branch, deletes session.

**Flags**
- `--force` / `-f` - Force destroy even with uncommitted changes (bypasses git safety checks)

### Cleanup Orphaned Resources
```bash
shards cleanup [--all] [--orphans] [--no-pid] [--stopped] [--older-than <days>]
```

Cleans up resources that got out of sync (crashes, manual deletions, etc.).

**Flags**
- `--all` - Clean all orphaned resources (default)
- `--orphans` - Clean worktrees with no matching session
- `--no-pid` - Clean sessions without PID tracking
- `--stopped` - Clean sessions with dead processes
- `--older-than <days>` - Clean sessions older than N days

## Configuration

Hierarchical TOML config (later overrides earlier):
1. Hardcoded defaults
2. User config - `~/.shards/config.toml`
3. Project config - `./.shards/config.toml`
4. CLI flags

## Key Features

- **Process Tracking** - Captures PID, process name, start time. Validates identity before killing.
- **Port Allocation** - Unique port range per shard (default 10 ports from base 3000).
- **Session Persistence** - File-based storage in `~/.shards/sessions/`
- **Cross-Platform** - macOS, Linux, Windows with native terminal integration.

## Best Practices

- Use descriptive branch names like `feature-auth`, `bug-fix-123`, `issue-456`
- Always destroy shards when done to clean up resources
- Use `shards cleanup` after crashes or manual deletions

## Additional Resources

- For installation and updating, see [cookbook/installation.md](cookbook/installation.md)
- For E2E testing, see [cookbook/e2e-testing.md](cookbook/e2e-testing.md)
