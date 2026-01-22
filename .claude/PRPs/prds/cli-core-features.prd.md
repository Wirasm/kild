# CLI Core Features PRD

**Status**: READY FOR IMPLEMENTATION
**Priority**: High - Foundation for CLI and UI usability

---

## Meta: How to Think About This PRD

**This document is for building the CLI features that make Shards genuinely useful.**

The current CLI has solid infrastructure (create, list, destroy, restart, status, health, cleanup) but lacks the "human workflow" features that turn it from a tool into a productivity multiplier.

### First Principles

1. **Information at a glance**: Users should understand shard state without extra commands
2. **Fast context switching**: Moving between shards should be instant
3. **Don't lose work**: Git activity should be visible, accidental destruction prevented
4. **Scriptable**: Everything should have JSON output for automation

### What's Already Built (Don't Duplicate)

- 7 commands: create, list, destroy, restart, status, cleanup, health
- Health monitoring with status classification (Working/Idle/Stuck/Crashed)
- Watch mode (`health --watch --interval N`)
- Process lifecycle tracking with PID validation
- Terminal management (iTerm, Terminal.app, Ghostty)
- Configuration hierarchy (global + project)

### What This PRD Adds

The missing pieces that make the CLI feel complete:
- Session metadata (notes)
- Quick navigation (cd, open, focus)
- Git visibility (diff, commits)
- Output control (--json, --quiet)
- Safety features (--force, confirmation)
- Bulk operations (--all)
- Quality of life (fuzzy matching)

---

## Problem Statement

Users managing multiple shards lack visibility and navigation tools:

1. **Can't remember what each shard is for** - no notes/descriptions
2. **Slow context switching** - must manually find worktree paths, hunt for terminal windows
3. **Can't see work done** - no git activity visibility without entering each worktree
4. **Verbose output** - JSON logs clutter terminal during normal use
5. **No scripting support** - can't pipe shard data to other tools
6. **Dangerous operations** - destroy has no confirmation, no bulk operations

## Evidence

- ideas.md Quick Wins section identifies these as "Low Effort, High Impact"
- Session struct has no note/description field despite being in the vision
- No `shards cd`, `shards open`, `shards focus` commands exist
- `--json` only exists for health command, not list/status
- No `--quiet` mode to suppress logs
- No `--force` flag for destroy

---

## Solution: MoSCoW Prioritization

### Must Have (Phase 1)

Essential features that significantly improve daily usability.

| Feature | Command/Flag | Value |
|---------|--------------|-------|
| Session notes | `--note` on create, shown in list | Know what each shard is doing |
| Print worktree path | `shards cd <branch>` | Fast navigation |
| Open in editor | `shards open <branch>` | One command to start working |
| JSON output | `--json` on list, status | Scriptability |
| Quiet mode | `-q` / `--quiet` globally | Clean output |
| Force destroy | `--force` on destroy | Skip confirmation for scripts |

### Should Have (Phase 2)

Important features that improve workflow significantly.

| Feature | Command/Flag | Value |
|---------|--------------|-------|
| Focus terminal | `shards focus <branch>` | Quick window switching |
| Git diff | `shards diff <branch>` | See changes without entering worktree |
| Git commits | `shards commits <branch>` | See work done |
| Bulk destroy | `shards destroy --all` | Clean slate |
| Bulk restart | `shards restart --all` | Refresh all agents |
| Fuzzy matching | Partial branch names | Less typing |

### Could Have (Phase 3)

Nice to have features that polish the experience.

| Feature | Command/Flag | Value |
|---------|--------------|-------|
| Session history | `shards history` | See past sessions |
| Quick aliases | `s1`, `s2` shortcuts | Even less typing |
| Branch sync | `shards sync <branch>` | Keep branches updated |
| Clone shard | `shards clone <from> <to>` | Safe experimentation |

### Won't Have (This PRD)

Out of scope - separate PRDs or future work.

| Feature | Why Not |
|---------|---------|
| Templates | Tier 3, needs design work |
| GitHub integration | Tier 3, separate PRD |
| Tags | Tier 4, notes sufficient for now |
| Output logging | Tier 4, complex implementation |
| Shard groups | Tier 5, overkill for now |
| tmux mode | Tier 5, different architecture |

---

## Implementation Phases

### Phase 1: Quick Wins (Must Have)

**Goal**: High-impact features with minimal implementation effort.

Each feature is a small, focused PR.

---

#### 1.1 Session Notes (`--note`)

**What**: Add optional description when creating a shard.

**Changes Required**:

1. **Session struct** (`src/sessions/types.rs`):
```rust
pub struct Session {
    // ... existing fields ...

    /// Optional description of what this shard is for
    pub note: Option<String>,
}
```

2. **Create command** (`src/cli/app.rs`):
```rust
#[arg(long, help = "Description of what this shard is for")]
note: Option<String>,
```

3. **List table** (`src/cli/table.rs`):
   - Add "Note" column (truncated to ~30 chars)
   - Show full note in `shards status`

4. **Session JSON**: Include note in serialization (already handled by serde)

**Validation**:
```bash
shards create feature-auth --note "Implementing JWT authentication"
shards list
# Shows: feature-auth | claude | Active | ... | Implementing JWT...

shards status feature-auth
# Shows full note
```

**Files to modify**:
- `src/sessions/types.rs` - Add field
- `src/cli/app.rs` - Add flag
- `src/cli/commands/create.rs` - Pass note to session
- `src/cli/table.rs` - Add column
- `src/cli/commands/status.rs` - Show full note

---

#### 1.2 Print Worktree Path (`shards cd`)

**What**: Print the worktree path for shell integration.

**Usage**:
```bash
# Print path
shards cd feature-auth
# Output: /Users/x/.shards/worktrees/project/feature-auth

# Shell integration (user adds to .zshrc)
scd() { cd "$(shards cd "$1")" }

# Then:
scd feature-auth  # Actually changes directory
```

**Why print, not change directory**: A subprocess can't change the parent shell's directory. Printing lets users integrate with their shell.

**Changes Required**:

1. **New command** (`src/cli/commands/cd.rs`):
```rust
pub fn execute(branch: &str) -> Result<()> {
    let session = sessions::get_by_branch(branch)?;
    println!("{}", session.worktree_path.display());
    Ok(())
}
```

2. **CLI registration** (`src/cli/app.rs`):
```rust
/// Print worktree path for a shard (for shell integration)
Cd {
    /// Branch name
    branch: String,
},
```

**Validation**:
```bash
shards cd feature-auth
# Prints: /path/to/worktree

cd "$(shards cd feature-auth)"
# Actually changes directory
```

**Files to modify**:
- `src/cli/app.rs` - Add command
- `src/cli/commands/mod.rs` - Add module
- `src/cli/commands/cd.rs` - New file

---

#### 1.3 Open in Editor (`shards open`)

**What**: Open shard's worktree in the user's editor.

**Usage**:
```bash
shards open feature-auth           # Uses $EDITOR or defaults to 'code'
shards open feature-auth --editor vim
```

**Changes Required**:

1. **New command** (`src/cli/commands/open.rs`):
```rust
pub fn execute(branch: &str, editor: Option<&str>) -> Result<()> {
    let session = sessions::get_by_branch(branch)?;
    let editor = editor
        .map(String::from)
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "code".to_string());

    Command::new(&editor)
        .arg(&session.worktree_path)
        .spawn()?;

    Ok(())
}
```

2. **CLI registration**:
```rust
/// Open shard's worktree in editor
Open {
    /// Branch name
    branch: String,

    /// Editor to use (defaults to $EDITOR or 'code')
    #[arg(long)]
    editor: Option<String>,
},
```

**Validation**:
```bash
shards open feature-auth
# VS Code opens with worktree

EDITOR=vim shards open feature-auth
# Vim opens with worktree
```

**Files to modify**:
- `src/cli/app.rs` - Add command
- `src/cli/commands/mod.rs` - Add module
- `src/cli/commands/open.rs` - New file

---

#### 1.4 JSON Output (`--json`)

**What**: Machine-readable output for list and status commands.

**Usage**:
```bash
shards list --json
# [{"branch": "feature-auth", "agent": "claude", "status": "Active", ...}]

shards status feature-auth --json
# {"branch": "feature-auth", "agent": "claude", ...}

# Scriptable:
shards list --json | jq '.[] | select(.status == "Active") | .branch'
```

**Changes Required**:

1. **List command** (`src/cli/commands/list.rs`):
```rust
#[arg(long, help = "Output as JSON")]
json: bool,

// In execute:
if args.json {
    println!("{}", serde_json::to_string_pretty(&sessions)?);
} else {
    print_table(&sessions);
}
```

2. **Status command** (`src/cli/commands/status.rs`):
```rust
#[arg(long, help = "Output as JSON")]
json: bool,

// In execute:
if args.json {
    println!("{}", serde_json::to_string_pretty(&session)?);
} else {
    print_status(&session);
}
```

**Validation**:
```bash
shards list --json | jq '.[0].branch'
# "feature-auth"
```

**Files to modify**:
- `src/cli/app.rs` - Add flag to List and Status
- `src/cli/commands/list.rs` - Handle json flag
- `src/cli/commands/status.rs` - Handle json flag

---

#### 1.5 Quiet Mode (`-q` / `--quiet`)

**What**: Suppress log output, show only essential information.

**Usage**:
```bash
shards create feature-auth          # Shows logs
shards -q create feature-auth       # Clean output, just success/failure

# For scripting:
BRANCH=$(shards -q create feature-auth)
```

**Changes Required**:

1. **Global flag** (`src/cli/app.rs`):
```rust
#[derive(Parser)]
struct Cli {
    #[arg(short, long, global = true, help = "Suppress log output")]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}
```

2. **Log initialization** (`src/main.rs` or logging setup):
```rust
if cli.quiet {
    // Set log level to error only, or disable tracing subscriber
    std::env::set_var("SHARDS_LOG", "error");
}
```

**Validation**:
```bash
shards create feature-auth 2>&1 | wc -l
# Many lines (logs)

shards -q create feature-auth 2>&1 | wc -l
# 1-2 lines (just result)
```

**Files to modify**:
- `src/cli/app.rs` - Add global flag
- `src/main.rs` - Check flag before log init

---

#### 1.6 Force Destroy (`--force`)

**What**: Skip confirmation prompt for destroy command.

**Usage**:
```bash
shards destroy feature-auth
# Prompt: "Destroy shard 'feature-auth'? This removes the worktree. [y/N]"

shards destroy --force feature-auth
# No prompt, immediate destruction

# For scripts:
shards destroy --force feature-auth || echo "Failed"
```

**Changes Required**:

1. **Destroy command** (`src/cli/commands/destroy.rs`):
```rust
#[arg(long, short = 'f', help = "Skip confirmation prompt")]
force: bool,

// In execute:
if !args.force {
    print!("Destroy shard '{}'? This removes the worktree. [y/N] ", branch);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Cancelled.");
        return Ok(());
    }
}

// Proceed with destruction
```

**Validation**:
```bash
echo "n" | shards destroy feature-auth
# Cancelled.

shards destroy --force feature-auth
# Destroyed immediately
```

**Files to modify**:
- `src/cli/app.rs` - Add flag to Destroy
- `src/cli/commands/destroy.rs` - Add confirmation logic

---

### Phase 2: Navigation & Visibility (Should Have)

**Goal**: Features that significantly improve workflow.

---

#### 2.1 Focus Terminal (`shards focus`)

**What**: Bring a shard's terminal window to foreground.

**Usage**:
```bash
shards focus feature-auth
# Terminal window with feature-auth comes to front
```

**Implementation**: Use AppleScript (macOS) to activate window by stored window ID.

**Changes Required**:

1. **New command** (`src/cli/commands/focus.rs`):
```rust
pub fn execute(branch: &str) -> Result<()> {
    let session = sessions::get_by_branch(branch)?;

    let window_id = session.terminal_window_id
        .ok_or_else(|| anyhow!("No terminal window ID stored"))?;

    terminal::focus_window(&session.terminal_type, &window_id)?;
    Ok(())
}
```

2. **Terminal module** (`src/terminal/operations.rs`):
```rust
pub fn focus_window(terminal_type: &TerminalType, window_id: &str) -> Result<()> {
    match terminal_type {
        TerminalType::ITerm => iterm::focus_window(window_id),
        TerminalType::Ghostty => ghostty::focus_window(window_id),
        // etc.
    }
}
```

**Validation**:
```bash
# With multiple terminal windows open
shards focus feature-auth
# feature-auth terminal comes to front
```

**Files to modify**:
- `src/cli/app.rs` - Add command
- `src/cli/commands/focus.rs` - New file
- `src/terminal/operations.rs` - Add focus function
- `src/terminal/iterm.rs` - Add iTerm focus
- `src/terminal/ghostty.rs` - Add Ghostty focus

---

#### 2.2 Git Diff (`shards diff`)

**What**: Show git diff for a shard without entering the worktree.

**Usage**:
```bash
shards diff feature-auth
# Shows: git diff output for that worktree

shards diff feature-auth --staged
# Shows: staged changes only
```

**Changes Required**:

1. **New command** (`src/cli/commands/diff.rs`):
```rust
pub fn execute(branch: &str, staged: bool) -> Result<()> {
    let session = sessions::get_by_branch(branch)?;

    let mut cmd = Command::new("git");
    cmd.current_dir(&session.worktree_path);
    cmd.arg("diff");

    if staged {
        cmd.arg("--staged");
    }

    let output = cmd.output()?;
    io::stdout().write_all(&output.stdout)?;

    Ok(())
}
```

**Validation**:
```bash
shards diff feature-auth
# Shows uncommitted changes

shards diff feature-auth --staged
# Shows staged changes
```

**Files to modify**:
- `src/cli/app.rs` - Add command
- `src/cli/commands/diff.rs` - New file

---

#### 2.3 Git Commits (`shards commits`)

**What**: Show recent commits made in a shard.

**Usage**:
```bash
shards commits feature-auth
# Shows: recent commits in that branch

shards commits feature-auth --count 5
# Shows: last 5 commits
```

**Changes Required**:

1. **New command** (`src/cli/commands/commits.rs`):
```rust
pub fn execute(branch: &str, count: usize) -> Result<()> {
    let session = sessions::get_by_branch(branch)?;

    let output = Command::new("git")
        .current_dir(&session.worktree_path)
        .args(&["log", "--oneline", "-n", &count.to_string()])
        .output()?;

    io::stdout().write_all(&output.stdout)?;

    Ok(())
}
```

**Validation**:
```bash
shards commits feature-auth
# Shows recent commits

shards commits feature-auth --count 3
# Shows last 3 commits
```

**Files to modify**:
- `src/cli/app.rs` - Add command
- `src/cli/commands/commits.rs` - New file

---

#### 2.4 Bulk Destroy (`--all`)

**What**: Destroy all shards for current project.

**Usage**:
```bash
shards destroy --all
# Prompt: "Destroy ALL 5 shards? [y/N]"

shards destroy --all --force
# No prompt
```

**Changes Required**:

1. **Destroy command** modification:
```rust
#[arg(long, help = "Destroy all shards")]
all: bool,

// In execute:
if args.all {
    let sessions = sessions::list_for_project()?;

    if !args.force {
        print!("Destroy ALL {} shards? [y/N] ", sessions.len());
        // ... confirmation logic
    }

    for session in sessions {
        destroy_session(&session)?;
    }
} else {
    // Existing single-branch logic
}
```

**Validation**:
```bash
shards list
# Shows 3 shards

shards destroy --all --force
shards list
# Shows 0 shards
```

**Files to modify**:
- `src/cli/app.rs` - Add --all flag to Destroy
- `src/cli/commands/destroy.rs` - Handle --all

---

#### 2.5 Bulk Restart (`--all`)

**What**: Restart all shards.

**Usage**:
```bash
shards restart --all
# Restarts all active shards
```

**Changes Required**: Similar to bulk destroy.

**Files to modify**:
- `src/cli/app.rs` - Add --all flag to Restart
- `src/cli/commands/restart.rs` - Handle --all

---

#### 2.6 Fuzzy Branch Matching

**What**: Match partial branch names when unambiguous.

**Usage**:
```bash
shards list
# feature-auth, feature-api, bugfix-login

shards status auth
# Matches feature-auth (unambiguous)

shards status feature
# Error: Multiple matches: feature-auth, feature-api. Be more specific.
```

**Changes Required**:

1. **Session lookup** (`src/sessions/mod.rs`):
```rust
pub fn get_by_branch_fuzzy(query: &str) -> Result<Session> {
    let sessions = list_for_project()?;

    // Exact match first
    if let Some(session) = sessions.iter().find(|s| s.branch == query) {
        return Ok(session.clone());
    }

    // Fuzzy match
    let matches: Vec<_> = sessions
        .iter()
        .filter(|s| s.branch.contains(query))
        .collect();

    match matches.len() {
        0 => Err(anyhow!("No shard matching '{}' found", query)),
        1 => Ok(matches[0].clone()),
        _ => {
            let names: Vec<_> = matches.iter().map(|s| &s.branch).collect();
            Err(anyhow!("Multiple matches: {}. Be more specific.", names.join(", ")))
        }
    }
}
```

2. **Update all commands** to use fuzzy lookup.

**Validation**:
```bash
shards status auth
# Works for feature-auth

shards status feature
# Error: Multiple matches
```

**Files to modify**:
- `src/sessions/mod.rs` - Add fuzzy lookup function
- All command files - Use fuzzy lookup

---

### Phase 3: Quality of Life (Could Have)

Brief descriptions - detailed specs if we get here.

#### 3.1 Session History (`shards history`)

Show past destroyed sessions. Requires storing sessions on destroy instead of deleting.

#### 3.2 Quick Aliases

Auto-assign `s1`, `s2`, etc. based on creation order. Store in session metadata.

#### 3.3 Branch Sync (`shards sync`)

Run `git fetch && git merge origin/main` (or rebase) in worktree.

#### 3.4 Clone Shard (`shards clone`)

Create new worktree from existing shard's current state.

---

## Validation Summary

### Phase 1 Validation

```bash
# 1.1 Notes
shards create test --note "Testing notes feature"
shards list | grep "Testing"
shards status test | grep "Testing notes feature"

# 1.2 CD
path=$(shards cd test)
[ -d "$path" ] && echo "CD works"

# 1.3 Open
shards open test  # VS Code opens

# 1.4 JSON
shards list --json | jq '.[0].branch'
shards status test --json | jq '.note'

# 1.5 Quiet
lines=$(shards -q create test2 2>&1 | wc -l)
[ "$lines" -lt 5 ] && echo "Quiet works"

# 1.6 Force
shards destroy --force test
shards destroy --force test2
```

### Phase 2 Validation

```bash
# Create test shards
shards create test1
shards create test2

# 2.1 Focus
shards focus test1  # Window comes to front

# 2.2 Diff
echo "change" >> "$(shards cd test1)/README.md"
shards diff test1 | grep "change"

# 2.3 Commits
shards commits test1

# 2.4/2.5 Bulk
shards restart --all
shards destroy --all --force

# 2.6 Fuzzy
shards create feature-auth
shards status auth  # Should work
```

---

## Files Summary

### Phase 1 Files

| File | Change |
|------|--------|
| `src/sessions/types.rs` | Add `note: Option<String>` |
| `src/cli/app.rs` | Add commands and flags |
| `src/cli/commands/cd.rs` | New file |
| `src/cli/commands/open.rs` | New file |
| `src/cli/commands/create.rs` | Pass note |
| `src/cli/commands/destroy.rs` | Add --force, confirmation |
| `src/cli/commands/list.rs` | Add --json |
| `src/cli/commands/status.rs` | Add --json |
| `src/cli/table.rs` | Add Note column |
| `src/main.rs` | Handle --quiet |

### Phase 2 Files

| File | Change |
|------|--------|
| `src/cli/commands/focus.rs` | New file |
| `src/cli/commands/diff.rs` | New file |
| `src/cli/commands/commits.rs` | New file |
| `src/terminal/operations.rs` | Add focus_window |
| `src/sessions/mod.rs` | Add fuzzy lookup |

---

## Dependencies

No new crates needed. Uses existing:
- `clap` for CLI parsing
- `serde` / `serde_json` for JSON output
- `std::process::Command` for git operations

---

## Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| `shards cd` prints path | Can't change parent shell | Standard Unix pattern |
| `--force` before branch | Consistent with rm -f | Familiar to users |
| Fuzzy matching | Contains match | Simple, predictable |
| No tags (yet) | Notes are enough | YAGNI |
| No templates (yet) | Tier 3, needs design | Keep scope focused |

---

*Status: READY FOR IMPLEMENTATION*
*Priority: Phase 1 first, each feature is a small PR*
*Created: 2026-01-22*
