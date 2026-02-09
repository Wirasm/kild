---
name: kild
description: |
  Create and manage parallel AI development sessions in isolated Git worktrees.

  TRIGGERS - Use this skill when user says:
  - Creating: "create a kild", "spin up kilds", "new kild for", "create worktrees"
  - Listing: "list kilds", "show kilds", "active kilds", "kilds list"
  - Status: "kild status", "check kild", "kild health", "how are my kilds"
  - Navigation: "cd to kild", "go to kild", "path to kild", "open in editor", "edit kild", "code kild"
  - Lifecycle: "stop kild", "open kild", "destroy kild", "complete kild", "clean up kilds"
  - Output: "list as json", "json output", "verbose mode"

  KILD creates isolated Git worktrees where AI agents work independently without
  affecting your main branch. Each kild gets its own terminal window, port range,
  and process tracking.

  IMPORTANT: Always use defaults from user's config. Only specify --agent, --terminal,
  or --flags when the user explicitly requests a different value.

  EXAMPLES

  User says "Create a kild for the auth feature"
  Command - kild create feature-auth
  Result - Creates worktree using default agent from config, opens in default terminal

  User says "Create a kild with kiro instead"
  Command - kild create feature-auth --agent kiro
  Result - Overrides default agent with kiro

  User says "Show me all active kilds"
  Command - kild list
  Result - Table showing branch, agent, status, note, ports, and process info

  User says "Open the auth kild in my editor"
  Command - kild code feature-auth
  Result - Opens worktree in user's default editor ($EDITOR or zed)

  User says "Go to the auth kild directory"
  Command - kild cd feature-auth
  Result - Prints path for shell integration: /Users/x/.kild/worktrees/project/feature-auth

allowed-tools: Bash, Read, Glob, Grep
---

# KILD CLI - Parallel AI Development Manager

KILD creates isolated Git worktrees for parallel AI development sessions. Each kild runs in its own workspace with dedicated port ranges and process tracking.

## Important: Use Defaults

**Always use the user's configured defaults.** Users set their preferences in config files:
- `~/.kild/config.toml` (user-level)
- `./.kild/config.toml` (project-level)

**DO NOT specify `--agent`, `--terminal`, or `--flags` unless the user explicitly asks to override.**

```bash
# CORRECT - use defaults
kild create feature-auth

# CORRECT - user asked for kiro specifically
kild create feature-auth --agent kiro

# WRONG - don't assume agent
kild create feature-auth --agent claude
```

**When to override:**
- User says "use kiro" → add `--agent kiro`
- User says "use iTerm" → add `--terminal iterm`
- User says "with trust all tools" → add `--flags '--trust-all-tools'`

## Core Commands

### Create a Kild
```bash
kild create <branch> [--agent <agent>] [--terminal <terminal>] [--flags <flags>] [--note <note>] [--no-agent]
```

Creates an isolated workspace with:
- New Git worktree in `~/.kild/worktrees/<project>/<branch>/`
- Unique port range (10 ports, starting from 3000)
- Native terminal with AI agent launched (or bare terminal with `--no-agent`)
- Process tracking (PID, name, start time)
- Session metadata saved to `~/.kild/sessions/`

**Supported agents** - claude, kiro, gemini, codex, amp, opencode
**Supported terminals** - ghostty, iterm, terminal, native

**Examples**
```bash
# Basic - uses defaults from config
kild create feature-auth
# Result: Creates kild with default agent/terminal from config

# With description
kild create feature-auth --note "Implementing JWT authentication"
# Result: Creates kild with note shown in list/status output

# Override agent (only when user requests)
kild create feature-auth --agent kiro
# Result: Uses kiro instead of default agent

# Override terminal (only when user requests)
kild create feature-auth --terminal iterm
# Result: Opens in iTerm instead of default terminal

# Create without agent (opens bare terminal with $SHELL)
kild create debug-session --no-agent
# Result: Creates kild but opens bare terminal instead of launching an agent
```

### List All Kilds
```bash
kild list [--json]
```

Shows table with branch, agent, status, timestamps, port range, process status, command, and note.

**Examples**
```bash
# Human-readable table
kild list
# Result: Formatted table with all kild info

# JSON for scripting
kild list --json
# Result: JSON array of session objects

# Filter with jq
kild list --json | jq '.[] | select(.status == "Active") | .branch'
# Result: List of active branch names
```

### Status (Detailed View)
```bash
kild status <branch> [--json]
```

Shows detailed info for a specific kild including worktree path, process metadata, port allocation, and note.

**Examples**
```bash
# Human-readable
kild status feature-auth
# Result: Detailed status box with all kild info

# JSON for scripting
kild status feature-auth --json
# Result: JSON object with full session data
```

### Print Worktree Path (Shell Integration)
```bash
kild cd <branch>
```

Prints the worktree path for shell integration. Use with shell wrapper for actual directory change.

**Examples**
```bash
# Print path
kild cd feature-auth
# Result: /Users/x/.kild/worktrees/project/feature-auth

# Shell integration (user adds to .zshrc/.bashrc)
kcd() { cd "$(kild cd "$1")" }

# Then use:
kcd feature-auth
# Result: Actually changes directory to the worktree
```

### Open in Editor
```bash
kild code <branch> [--editor <editor>]
```

Opens the kild's worktree in the user's editor. Priority: `--editor` flag > `$EDITOR` env var > "zed" default.

**Examples**
```bash
# Use default editor
kild code feature-auth
# Result: Opens worktree in $EDITOR or zed

# Override editor
kild code feature-auth --editor vim
# Result: Opens worktree in vim
```

### Open a New Agent in a Kild
```bash
kild open <branch> [--agent <agent>]
```

Opens a new agent terminal in an existing kild. This is **additive** - it doesn't close existing terminals, allowing multiple agents to work in the same kild.

**Examples**
```bash
# Reopen with default agent
kild open feature-auth
# Result: New terminal opens with default agent

# Open with different agent
kild open feature-auth --agent kiro
# Result: New terminal opens with kiro (original agent still running if any)
```

### Stop a Kild
```bash
kild stop <branch>
```

Stops the agent process and closes the terminal, but preserves the kild (worktree and uncommitted changes remain). Can be reopened later with `kild open`.

**Example**
```bash
kild stop feature-auth
# Result: Terminal closes, worktree preserved, status changes to "Stopped"
```

### Destroy a Kild
```bash
kild destroy <branch> [--force]
```

Completely removes a kild - closes terminal, kills process, removes worktree and branch, deletes session.

**Safety Checks** (before destroying):
- **Blocks** on uncommitted changes (staged, modified, or untracked files)
- **Warns** about unpushed commits
- **Warns** if branch has never been pushed to remote
- **Warns** if no PR exists for the branch

**Flags**
- `--force` / `-f` - Bypass all git safety checks

**Examples**
```bash
# Normal destroy (shows warnings, blocks on uncommitted changes)
kild destroy feature-auth
# Result: Blocks if uncommitted changes exist, warns about unpushed commits

# Force destroy (bypasses all git safety checks)
kild destroy feature-auth --force
# Result: Removes kild immediately, no safety checks
```

### Complete a Kild (PR Cleanup)
```bash
kild complete <branch>
```

Completes a kild by destroying it and cleaning up the remote branch if the PR was merged.

Use this when finishing work on a PR. The command adapts to your workflow:
- If PR was already merged (you ran `gh pr merge` first), it also deletes the orphaned remote branch
- If PR hasn't been merged yet, it just destroys the kild so `gh pr merge --delete-branch` can work

**Note:** Always blocks on uncommitted changes (use `kild destroy --force` for forced removal). Requires `gh` CLI to detect merged PRs. If `gh` is not installed, the command still works but won't auto-delete remote branches.

**Workflow A: Complete first, then merge**
```bash
kild complete my-feature    # Destroys kild
gh pr merge 123 --delete-branch  # Merges PR, deletes remote (now works!)
```

**Workflow B: Merge first, then complete**
```bash
gh pr merge 123 --squash    # Merges PR (can't delete remote due to worktree)
kild complete my-feature    # Destroys kild AND deletes orphaned remote
```

### Health Monitoring
```bash
kild health [branch] [--json] [--watch] [--interval <seconds>]
```

Shows health dashboard with process status, CPU/memory metrics, and summary statistics.

**Examples**
```bash
# Dashboard view
kild health
# Result: Table with CPU, memory, status for all kilds

# Watch mode (auto-refresh)
kild health --watch --interval 5
# Result: Live dashboard updating every 5 seconds

# JSON output
kild health --json
# Result: JSON with health metrics
```

### Cleanup Orphaned Resources
```bash
kild cleanup [--all] [--orphans] [--no-pid] [--stopped] [--older-than <days>]
```

Cleans up resources that got out of sync (crashes, manual deletions, etc.).

**Flags**
- `--all` - Clean all orphaned resources (default)
- `--orphans` - Clean worktrees with no matching session
- `--no-pid` - Clean sessions without PID tracking
- `--stopped` - Clean sessions with dead processes
- `--older-than <days>` - Clean sessions older than N days

## Global Flags

### Verbose Mode
```bash
kild -v <command>
kild --verbose <command>
```

Enables JSON log output for debugging. By default, logs are suppressed for clean output.

**Examples**
```bash
# Normal (clean output, no JSON logs)
kild list
# Result: Just the table, no logs

# Verbose (shows JSON logs)
kild -v list
# Result: JSON logs + table

# Scripting (logs suppressed by default)
kild list --json | jq '.[] | .branch'
# Result: Clean JSON without log noise
```

## Configuration

KILD uses hierarchical TOML config. Later sources override earlier:

1. **Hardcoded defaults** - Built into kild
2. **User config** - `~/.kild/config.toml`
3. **Project config** - `./.kild/config.toml`
4. **CLI flags** - Always win

### Config File Structure

```toml
# ~/.kild/config.toml or ./.kild/config.toml

[agent]
default = "claude"  # Default agent for new kilds

[terminal]
preferred = "ghostty"  # Default terminal: ghostty, iterm, terminal, native
spawn_delay_ms = 1000   # Wait time for terminal spawn
max_retry_attempts = 5  # Retries for PID capture

[ports]
base = 3000       # Starting port number
range_size = 10   # Ports per kild

[agents.claude]
command = "claude"
flags = "--dangerously-skip-permissions"  # Auto-apply these flags

[agents.kiro]
command = "kiro"
flags = "--trust-all-tools"

[agents.codex]
command = "codex"
flags = "--yolo"
```

### Helping Users with Config

If a user wants to change defaults, help them edit their config:

**User wants claude with auto-permissions by default:**
```toml
# ~/.kild/config.toml
[agent]
default = "claude"

[agents.claude]
flags = "--dangerously-skip-permissions"
```

**User wants to use iTerm instead of Ghostty:**
```toml
# ~/.kild/config.toml
[terminal]
preferred = "iterm"
```

**Project-specific agent:**
```toml
# ./.kild/config.toml (in project root)
[agent]
default = "kiro"  # This project uses kiro
```

## Autonomous Mode (YOLO / Trust All Tools)

Each agent has its own flag for skipping permission prompts. These should be set in config, not passed every time.

**Claude Code** - `--dangerously-skip-permissions`
**Kiro CLI** - `--trust-all-tools`
**Codex CLI** - `--yolo` or `--dangerously-bypass-approvals-and-sandbox`

**Recommended:** Set in config once:
```toml
[agents.claude]
flags = "--dangerously-skip-permissions"
```

Then just: `kild create feature-x` (flags auto-applied)

**Override only when needed:**
```bash
# User explicitly wants no flags this time
kild create feature-x --flags ''
```

## Key Features

- **Process Tracking** - Captures PID, process name, start time. Validates identity before killing.
- **Port Allocation** - Unique port range per kild (default 10 ports from base 3000).
- **Session Persistence** - File-based storage in `~/.kild/sessions/`
- **Session Notes** - Document what each kild is for with `--note`
- **JSON Output** - Scriptable output with `--json` flag
- **Verbose Mode** - Debug output with `-v` flag

## Best Practices

- Use descriptive branch names like `feature-auth`, `bug-fix-123`, `issue-456`
- Add notes to remember what each kild is for: `--note "Working on auth"`
- Always destroy kilds when done to clean up resources
- Use `kild cleanup` after crashes or manual deletions
- Set your preferred defaults in `~/.kild/config.toml` once

## Additional Resources

- For installation and updating, see [cookbook/installation.md](cookbook/installation.md)
- For E2E testing, see [cookbook/e2e-testing.md](cookbook/e2e-testing.md)
