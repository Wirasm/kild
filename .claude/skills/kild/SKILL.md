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
- User says "use kiro" -> add `--agent kiro`
- User says "use iTerm" -> add `--terminal iterm`
- User says "with trust all tools" -> add `--flags '--trust-all-tools'`

## Core Commands

### Create a Kild
```bash
kild create <branch> [options]
```

Creates an isolated workspace with:
- New Git worktree in `~/.kild/worktrees/<project>/<branch>/`
- Unique port range (10 ports, starting from 3000)
- Native terminal with AI agent launched (or bare terminal with `--no-agent`)
- Process tracking (PID, name, start time)
- Session metadata saved to `~/.kild/sessions/`

**Flags:**
- `--agent <agent>` / `-a` - Override default agent (amp, claude, kiro, gemini, codex, opencode)
- `--terminal <terminal>` / `-t` - Override default terminal (ghostty, iterm, terminal, native)
- `--startup-command <cmd>` - Override agent startup command
- `--flags <flags>` - Additional flags for agent (use `--flags 'value'` or `--flags='value'`)
- `--note <text>` / `-n` - Description shown in list/status output
- `--base <branch>` / `-b` - Base branch to create worktree from (default: main)
- `--no-fetch` - Skip fetching from remote before creating worktree
- `--yolo` - Enable full autonomy mode (skip all permission prompts). Conflicts with `--no-agent`
- `--no-agent` - Open bare terminal with $SHELL instead of launching an agent. Conflicts with `--agent`, `--startup-command`, `--flags`
- `--daemon` - Launch in daemon-owned PTY (overrides config). Conflicts with `--no-daemon`
- `--no-daemon` - Force external terminal window (overrides config). Conflicts with `--daemon`

**Examples:**
```bash
# Basic - uses defaults from config
kild create feature-auth

# With description
kild create feature-auth --note "Implementing JWT authentication"

# Override agent (only when user requests)
kild create feature-auth --agent kiro

# Override terminal (only when user requests)
kild create feature-auth --terminal iterm

# Autonomous mode
kild create feature-auth --yolo

# Create without agent (opens bare terminal with $SHELL)
kild create debug-session --no-agent

# Create from a different base branch
kild create feature-auth --base develop

# Launch in daemon-owned PTY
kild create feature-auth --daemon
```

### List All Kilds
```bash
kild list [--json]
```

Shows table with branch, agent, status, timestamps, port range, process status, command, and note.

**Examples:**
```bash
# Human-readable table
kild list

# JSON for scripting
kild list --json

# Filter with jq
kild list --json | jq '.sessions[] | select(.status == "Active") | .branch'
```

### Status (Detailed View)
```bash
kild status <branch> [--json]
```

Shows detailed info for a specific kild including worktree path, process metadata, port allocation, and note.

**Examples:**
```bash
kild status feature-auth
kild status feature-auth --json
```

### Print Worktree Path (Shell Integration)
```bash
kild cd <branch>
```

Prints the worktree path for shell integration. Use with shell wrapper for actual directory change.

**Examples:**
```bash
# Print path
kild cd feature-auth
# Result: /Users/x/.kild/worktrees/project/feature-auth

# Shell integration (user adds to .zshrc/.bashrc)
kcd() { cd "$(kild cd "$1")" }

# Then use:
kcd feature-auth
```

### Open in Editor
```bash
kild code <branch> [--editor <editor>]
```

Opens the kild's worktree in the user's editor. Priority: `--editor` flag > config default > $VISUAL > $EDITOR > OS default > PATH scan.

**Examples:**
```bash
kild code feature-auth
kild code feature-auth --editor vim
```

### Open a New Agent in a Kild
```bash
kild open <branch> [options]
kild open --all [options]
```

Opens a new agent terminal in an existing kild. This is **additive** - it doesn't close existing terminals, allowing multiple agents to work in the same kild.

**Flags:**
- `--agent <agent>` / `-a` - Agent to launch (default: kild's original agent)
- `--no-agent` - Open bare terminal with default shell instead of agent
- `--all` - Open agents in all stopped kilds. Conflicts with `<branch>`
- `--resume` / `-r` - Resume previous agent conversation instead of starting fresh. Conflicts with `--no-agent`
- `--yolo` - Enable full autonomy mode. Conflicts with `--no-agent`
- `--daemon` - Force daemon-owned PTY (overrides config). Conflicts with `--no-daemon`
- `--no-daemon` - Force external terminal window (overrides config). Conflicts with `--daemon`

**Examples:**
```bash
# Reopen with default agent
kild open feature-auth

# Open with different agent
kild open feature-auth --agent kiro

# Resume previous conversation
kild open feature-auth --resume

# Open bare terminal
kild open feature-auth --no-agent

# Enable autonomous mode
kild open feature-auth --yolo

# Open all stopped kilds
kild open --all

# Open all with specific agent
kild open --all --agent claude

# Resume all stopped kilds
kild open --all --resume

# Open all with autonomous mode
kild open --all --yolo

# Force daemon mode
kild open feature-auth --daemon
```

### Stop a Kild
```bash
kild stop <branch>
kild stop --all
```

Stops the agent process and closes the terminal, but preserves the kild (worktree and uncommitted changes remain). Can be reopened later with `kild open`.

**Flags:**
- `--all` - Stop all running kilds. Conflicts with `<branch>`

**Examples:**
```bash
kild stop feature-auth
kild stop --all
```

### Destroy a Kild
```bash
kild destroy <branch> [--force]
kild destroy --all [--force]
```

Completely removes a kild - closes terminal, kills process, removes worktree and branch, deletes session.

**Safety Checks** (before destroying):
- **Blocks** on uncommitted changes (staged, modified, or untracked files)
- **Warns** about unpushed commits
- **Warns** if branch has never been pushed to remote
- **Warns** if no PR exists for the branch

**Flags:**
- `--force` / `-f` - Bypass all git safety checks
- `--all` - Destroy all kilds for current project (with confirmation). Conflicts with `<branch>`

**CRITICAL: Never force-destroy without inspecting first.**
When `kild destroy` or `kild complete` blocks on uncommitted changes, the warning exists for a reason - there is work in that worktree that was NOT part of any PR. Before using `--force`:

1. **Check what's there**: Run `git -C $(kild cd <branch>) status` and `git -C $(kild cd <branch>) diff` to see what the uncommitted changes are.
2. **Ask the user** whether those changes should be committed, stashed, or discarded.
3. **Only then** use `--force` if the user confirms the changes can be lost.

Never silently bypass the safety check. Uncommitted changes may be in-progress work, follow-up fixes, or investigation artifacts that the user wants to keep.

**Examples:**
```bash
# Normal destroy (shows warnings, blocks on uncommitted changes)
kild destroy feature-auth

# Destroy all kilds (prompts for confirmation)
kild destroy --all

# Force destroy all (skip confirmation and git checks)
kild destroy --all --force

# CORRECT flow when blocked on uncommitted changes
git -C $(kild cd feature-auth) status   # See what's uncommitted
git -C $(kild cd feature-auth) diff     # Inspect the actual changes
# Ask user: "There are uncommitted changes in feature-auth: [summary]. Discard them?"
# Only after user confirms:
kild destroy feature-auth --force
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

### Focus a Kild's Terminal
```bash
kild focus <branch>
```

Brings the kild's terminal window to the foreground.

**Example:**
```bash
kild focus feature-auth
```

### Hide a Kild's Terminal
```bash
kild hide <branch>
kild hide --all
```

Minimizes/hides a kild's terminal window.

**Flags:**
- `--all` - Hide all active kild terminal windows. Conflicts with `<branch>`

**Examples:**
```bash
kild hide feature-auth
kild hide --all
```

### Git Diff for a Kild
```bash
kild diff <branch> [--staged] [--stat]
```

Shows git diff for a kild's worktree.

**Flags:**
- `--staged` - Show only staged changes (git diff --staged)
- `--stat` - Show unstaged diffstat summary instead of full diff

**Examples:**
```bash
kild diff feature-auth
kild diff feature-auth --staged
kild diff feature-auth --stat
```

### Recent Commits
```bash
kild commits <branch> [-n <count>]
```

Shows recent commits in a kild's branch.

**Flags:**
- `-n` / `--count` - Number of commits to show (default: 10)

**Examples:**
```bash
kild commits feature-auth
kild commits feature-auth -n 5
```

### Branch Health & Merge Readiness
```bash
kild stats <branch> [--json] [-b <base>]
kild stats --all [--json]
```

Shows branch health and merge readiness for a kild.

**Flags:**
- `--json` - Output in JSON format
- `-b` / `--base` - Base branch to compare against (overrides config, default: main)
- `--all` - Show stats for all kilds. Conflicts with `<branch>`

**Examples:**
```bash
kild stats feature-auth
kild stats feature-auth --json
kild stats feature-auth -b dev
kild stats --all
kild stats --all --json
```

### File Overlap Detection
```bash
kild overlaps [--json] [-b <base>]
```

Detects file overlaps across kilds in the current project.

**Flags:**
- `--json` - Output in JSON format
- `-b` / `--base` - Base branch to compare against (overrides config, default: main)

**Examples:**
```bash
kild overlaps
kild overlaps --json
kild overlaps -b dev
```

### PR Status
```bash
kild pr <branch> [--json] [--refresh]
```

Shows PR status for a kild.

**Flags:**
- `--json` - Output in JSON format
- `--refresh` - Force refresh PR data from GitHub

**Examples:**
```bash
kild pr feature-auth
kild pr feature-auth --json
kild pr feature-auth --refresh
```

### Rebase a Kild
```bash
kild rebase <branch> [-b <base>]
kild rebase --all
```

Rebases a kild's branch onto the base branch.

**Flags:**
- `-b` / `--base` - Base branch to rebase onto (overrides config, default: main)
- `--all` - Rebase all active kilds. Conflicts with `<branch>`

**Examples:**
```bash
kild rebase feature-auth
kild rebase feature-auth --base dev
kild rebase --all
```

### Sync a Kild (Fetch + Rebase)
```bash
kild sync <branch> [-b <base>]
kild sync --all
```

Fetches from remote and rebases a kild's branch onto the base branch.

**Flags:**
- `-b` / `--base` - Base branch to rebase onto (overrides config, default: main)
- `--all` - Fetch and rebase all active kilds. Conflicts with `<branch>`

**Examples:**
```bash
kild sync feature-auth
kild sync feature-auth --base dev
kild sync --all
```

### Agent Status (Hook Integration)
```bash
kild agent-status <branch> <status> [--notify] [--json]
kild agent-status --self <status> [--notify] [--json]
```

Reports agent activity status. Used by agent hooks (e.g., Codex notify hook) to update KILD session state.

**Flags:**
- `--self` - Auto-detect session from current working directory
- `--notify` - Send desktop notification when status is 'waiting' or 'error'
- `--json` - Output in JSON format

**Status values:** `working`, `idle`, `waiting`, `error`

**Examples:**
```bash
kild agent-status feature-auth working
kild agent-status --self idle
kild agent-status feature-auth waiting --notify
```

### Daemon Management
```bash
kild daemon start [--foreground]
kild daemon stop
kild daemon status [--json]
```

Manages the KILD daemon for PTY-based session management.

**Examples:**
```bash
kild daemon start              # Start in background
kild daemon start --foreground # Start in foreground (for debugging)
kild daemon stop               # Stop running daemon
kild daemon status             # Show daemon status
kild daemon status --json      # JSON output
```

### Attach to Daemon Session
```bash
kild attach <branch>
```

Attaches to a daemon-managed kild session. Press Ctrl+C to detach.

**Example:**
```bash
kild attach feature-auth
```

### Health Monitoring
```bash
kild health [branch] [--json] [--watch] [--interval <seconds>]
```

Shows health dashboard with process status, CPU/memory metrics, and summary statistics.

**Flags:**
- `--json` - Output in JSON format
- `--watch` / `-w` - Continuously refresh health display
- `--interval` / `-i` - Refresh interval in seconds (default: 5)

**Examples:**
```bash
kild health
kild health --watch --interval 5
kild health --json
```

### Cleanup Orphaned Resources
```bash
kild cleanup [--all] [--orphans] [--no-pid] [--stopped] [--older-than <days>]
```

Cleans up resources that got out of sync (crashes, manual deletions, etc.).

**Flags:**
- `--all` - Clean all orphaned resources (default)
- `--orphans` - Clean worktrees in kild directory that have no session
- `--no-pid` - Clean only sessions without PID tracking
- `--stopped` - Clean only sessions with stopped processes
- `--older-than <days>` - Clean sessions older than N days

**Examples:**
```bash
kild cleanup
kild cleanup --orphans
kild cleanup --older-than 7
```

### Shell Completions
```bash
kild completions <shell>
```

Generates shell completion scripts. Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

**Examples:**
```bash
kild completions fish > ~/.config/fish/completions/kild.fish
kild completions bash > /etc/bash_completion.d/kild
kild completions zsh > ~/.zfunc/_kild
```

### Initialize Agent Hooks
```bash
kild init-hooks <agent> [--no-install]
```

Initializes agent integration hooks in the current project.

**Flags:**
- `--no-install` - Skip running bun install after generating files

**Currently supported:** `opencode`

**Example:**
```bash
kild init-hooks opencode
kild init-hooks opencode --no-install
```

## Global Flags

### Verbose Mode
```bash
kild -v <command>
kild --verbose <command>
```

Enables JSON log output for debugging. By default, logs are suppressed for clean output.

### No Color Mode
```bash
kild --no-color <command>
```

Disables colored output. Also respects the `NO_COLOR` environment variable.

**Examples:**
```bash
# Normal (clean output, no JSON logs)
kild list

# Verbose (shows JSON logs)
kild -v list

# Scripting (logs suppressed by default)
kild list --json | jq '.sessions[] | .branch'
```

## Configuration

KILD uses hierarchical TOML config. Later sources override earlier:

1. **Hardcoded defaults** - Built into kild
2. **User config** - `~/.kild/config.toml`
3. **Project config** - `./.kild/config.toml`
4. **CLI flags** - Always win

**All config options are documented in `.kild/config.example.toml`.** Copy it to get started:

```bash
# User-wide config
cp .kild/config.example.toml ~/.kild/config.toml

# Project-specific config
cp .kild/config.example.toml .kild/config.toml
```

### Helping Users with Config

When a user wants to change defaults, read `.kild/config.example.toml` for the full list of options and help them edit their config file (`~/.kild/config.toml` for user-wide, `.kild/config.toml` for project-specific).

**Common config changes:**

| User wants | Config key | Example value |
|------------|-----------|---------------|
| Default agent | `[agent] default` | `"claude"` |
| Auto-permissions | `[agents.claude] flags` | `"--dangerously-skip-permissions"` |
| Different terminal | `[terminal] preferred` | `"iterm"` |
| Default editor | `[editor] default` | `"zed"` |
| Daemon mode by default | `[daemon] enabled` | `true` |

### Autonomous Mode (YOLO / Trust All Tools)

Each agent has its own flag for skipping permission prompts. These should be set in config, not passed every time.

**Claude Code** - `--dangerously-skip-permissions`
**Kiro CLI** - `--trust-all-tools`
**Codex CLI** - `--yolo` or `--dangerously-bypass-approvals-and-sandbox`

**Recommended:** Set in config once, then just `kild create feature-x` (flags auto-applied).

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
