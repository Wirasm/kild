# Shards: Architecture & Improvement Ideas

## Core Vision

**Shards is an environment manager for AI coding agents.**

The human works interactively with agents. Shards makes it effortless to:
1. **Create** isolated environments (worktrees) with agents running in terminals
2. **Track** what's open - sessions, processes, terminals
3. **Manage** lifecycle - restart, check status, monitor health
4. **Clean up** - destroy sessions, remove orphaned resources

```
One shard per task. One terminal per shard.

Request 1: "Create shard for auth"     →  Terminal 1 (auth)
Request 2: "Create shard for bugfix"   →  Terminal 2 (bugfix)
Request 3: "Create shard for issue 42" →  Terminal 3 (issue-42)

┌──────────────────────────────────────────────────────────────┐
│  HUMAN (in main Claude Code session)                         │
│                                                              │
│  "Create a shard for the auth feature"                       │
│         │                                                    │
│         ▼                                                    │
│  ┌────────────────────────────────────┐                     │
│  │  SHARDS CLI (via skill)            │                     │
│  │  $ shards create feature-auth      │                     │
│  └────────────────────────────────────┘                     │
│         │                                                    │
│         ▼                                                    │
│  ┌────────────────────────────────────┐                     │
│  │  NEW TERMINAL WINDOW               │                     │
│  │  ┌──────────────────────────────┐  │                     │
│  │  │  Agent (claude/kiro/etc)     │  │                     │
│  │  │  Working in: feature-auth    │  │  ← Human works here │
│  │  │  Worktree: ~/.shards/...     │  │    independently    │
│  │  └──────────────────────────────┘  │                     │
│  └────────────────────────────────────┘                     │
│                                                              │
│  Human can now ask for more shards:                          │
│  "Also create one for the login bug"                         │
│         │                                                    │
│         ▼                                                    │
│  $ shards create fix-login-bug  →  Another terminal opens    │
└──────────────────────────────────────────────────────────────┘
```

**Key Principle**: One shard = one task = one terminal. Human works directly in each terminal. Shards just manages the containers.

---

## What Shards Controls

| Aspect | Control Level | How |
|--------|--------------|-----|
| Worktree creation | Full | Git operations |
| Terminal window | Full | AppleScript/native |
| Agent launch | Full | Command execution |
| Session tracking | Full | JSON metadata |
| Process monitoring | Partial | PID tracking, health checks |
| Terminal close | Full | Window ID tracking |

## What Shards Does NOT Control

| Aspect | Why |
|--------|-----|
| Agent conversation | Interactive CLI, human-driven | research if we can later?
| What agent does | Autonomous within session |
| When agent "finishes" | Human decides |
| Agent's context/memory | Internal to agent |

---

## The Skill Concept

A simple `skill.md` file that teaches Claude Code (or any agent with skill support) how to help humans manage shards:

```markdown
# Shards Skill

Help the user manage parallel development environments.

## When to Use
- User wants to work on multiple features in parallel
- User needs isolated environments for different tasks
- User wants to spin up an agent for a specific task

## Commands
- `shards create <branch>` - Create new environment with agent
- `shards list` - Show all active environments
- `shards status <branch>` - Detailed status of one environment
- `shards destroy <branch>` - Clean up environment
- `shards restart <branch>` - Restart agent in existing environment
- `shards health` - Health dashboard for all environments

## Example Interactions

User: "Create a shard for the auth feature"
→ Run: `shards create feature-auth`
→ Opens ONE terminal with agent in feature-auth worktree

User: "I also need to fix that login bug"
→ Run: `shards create fix-login-bug`
→ Opens ANOTHER terminal with agent in fix-login-bug worktree

User: "What shards do I have open?"
→ Run: `shards list`
→ Shows: feature-auth, fix-login-bug

User: "Close the auth one, I'm done"
→ Run: `shards destroy feature-auth`
→ Closes terminal, removes worktree
```

**One shard per task.** The skill helps manage environments, not orchestrate work.

---

## Feature Ideas (Prioritized)

### Tier 1: Core Experience Improvements

#### 1.1 Session Notes/Description
Store what each shard is for:
```bash
shards create feature-auth --note "Implementing JWT authentication"
shards list  # Shows notes in table
```

**Value**: Context preservation, know what each shard is doing at a glance.

#### 1.2 Quick Navigation
```bash
shards cd feature-auth  # Prints: cd /path/to/worktree
shards open feature-auth  # Opens in VS Code/editor
```

**Value**: Fast switching between environments.

#### 1.3 Git Activity Tracking
```bash
shards diff feature-auth   # Show changes since shard created
shards commits feature-auth  # List commits made in this shard
```

**Value**: See what work was done without entering the worktree.

#### 1.4 Bulk Operations
```bash
shards destroy --all
shards restart --all
shards list --json  # For scripting
```

**Value**: Manage multiple shards efficiently.

---

### Tier 2: Better Visibility

#### 2.1 Enhanced Status Display
```bash
shards status feature-auth

# Shows:
# - Branch, agent, created time
# - Process status (running/stopped)
# - Git status (uncommitted changes, commits ahead)
# - Resource usage (CPU, memory)
# - Last activity timestamp
```

#### 2.2 Watch Mode
```bash
shards watch  # Live-updating dashboard
```

**Value**: Real-time visibility into all shards.

#### 2.3 Session History
```bash
shards history  # Past sessions (archived on destroy)
shards history feature-auth  # Details of past session
```

**Value**: Learn from past work, recall what was done.

---

### Tier 3: Workflow Enhancements

#### 3.1 Session Templates
```bash
shards create feature-auth --template feature
shards create bugfix-123 --template bugfix

# Templates define:
# - Default agent
# - Pre-configured flags
# - Initial files to copy
```

#### 3.2 GitHub Integration
```bash
shards create --issue 123  # Branch named from issue, links tracked
shards pr feature-auth     # Create PR from shard's branch
```

**Value**: Tighter integration with GitHub workflow.

#### 3.3 Auto-Cleanup
```bash
# In config:
[cleanup]
auto_destroy_after_days = 7
warn_before_destroy = true
```

**Value**: Prevent orphaned resources accumulating.

---

### Tier 4: Advanced Features

#### 4.1 Session Tagging
```bash
shards create feature-auth --tags "auth,security,p1"
shards list --tag security  # Filter by tag
```

#### 4.2 Output Logging (Optional)
```bash
shards create feature-auth --log
shards logs feature-auth
shards logs feature-auth --follow
```

**Implementation**: Wrap with `script` command to capture terminal output.

**Value**: Audit trail, debugging, learning from agent interactions.

#### 4.3 Conflict Detection
```bash
shards conflicts  # Warn if multiple shards editing same files
```

**Value**: Prevent merge conflicts before they happen.

---

### Tier 5: Out-of-the-Box Ideas

#### 5.1 Focus/Switch Terminal
```bash
shards focus feature-auth  # Bring terminal window to foreground
shards switch feature-auth  # Same, shorter alias
```

**Implementation**: AppleScript to activate specific window by ID.

**Value**: Quick context switching without hunting for windows.

#### 5.2 Clone Shard
```bash
shards clone feature-auth experiment-auth
# Creates new shard from current state of feature-auth
# Useful for: "let me try a different approach without losing this"
```

**Value**: Safe experimentation without losing work.

#### 5.3 Terminal Arrangement
```bash
shards arrange tile     # Tile all shard terminals
shards arrange stack    # Stack them
shards arrange left     # All on left half of screen
```

**Implementation**: AppleScript window positioning.

**Value**: Manage visual chaos when running many shards.

#### 5.4 Branch Sync
```bash
shards sync feature-auth        # Merge/rebase main into shard's branch
shards sync --all               # Sync all shards
shards sync feature-auth --rebase  # Use rebase instead of merge
```

**Value**: Keep long-running shards up to date without manual git work.

#### 5.5 Quick Aliases
```bash
# Auto-generated aliases based on creation order
shards list
# 1. feature-auth (s1)
# 2. fix-login (s2)
# 3. refactor-api (s3)

shards focus s1  # Quick reference
shards destroy s2
```

**Value**: Faster commands when you have many shards.

#### 5.6 Export Summary
```bash
shards export feature-auth > summary.md
# Generates:
# - Branch name, created time
# - Git log of commits made
# - Files changed
# - Current diff (if uncommitted changes)
```

**Value**: Documentation, handoff to others, or feeding back to another agent.

#### 5.7 Shard Groups
```bash
shards group create backend feature-auth feature-api feature-db
shards group list backend
shards group destroy backend  # Destroys all in group
shards group sync backend     # Syncs all in group
```

**Value**: Manage related shards as a unit.

#### 5.8 Remote/SSH Support (tmux mode)
```bash
shards create feature-auth --tmux
# Instead of native terminal, creates tmux session
# Works over SSH, persistent across disconnects
```

**Implementation**: Use `tmux new-session` instead of AppleScript.

**Value**: Remote development, persistence, SSH workflows.

#### 5.9 Stash on Destroy
```bash
shards destroy feature-auth --stash
# If uncommitted changes exist:
# 1. Creates git stash
# 2. Stores stash ref in history
# 3. Can restore later: shards restore feature-auth
```

**Value**: Never lose uncommitted work accidentally.

#### 5.10 Daily Digest
```bash
shards digest
# Shows:
# - Shards worked on today
# - Total commits across all shards
# - Files changed
# - Time spent (based on process activity)
```

**Value**: End-of-day summary, time tracking, accountability.

---

## Type Design Improvements

Current weaknesses and suggested fixes:

### Session Struct
```rust
// Current: Flat data bag
pub struct Session {
    pub terminal_type: Option<TerminalType>,
    pub terminal_window_id: Option<String>,
    pub process_id: Option<u32>,
    pub process_name: Option<String>,
    pub process_start_time: Option<u64>,
    // ... many more flat fields
}

// Better: Grouped related data
pub struct Session {
    pub id: SessionId,
    pub project: ProjectRef,
    pub branch: BranchName,
    pub worktree: WorktreePath,
    pub agent: AgentConfig,
    pub status: SessionStatus,
    pub ports: PortRange,
    pub process: Option<ProcessInfo>,
    pub terminal: Option<TerminalInfo>,
    pub metadata: SessionMetadata,
}

pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub start_time: u64,
}

pub struct TerminalInfo {
    pub terminal_type: TerminalType,
    pub window_id: String,
}

pub struct SessionMetadata {
    pub created_at: DateTime,
    pub last_activity: Option<DateTime>,
    pub note: Option<String>,
    pub tags: Vec<String>,
}
```

**Benefits**:
- Invariants expressed in types (can't have window_id without terminal_type)
- Clearer data relationships
- Easier to extend

---

## CLI UX Improvements

### Better Output Formatting
```bash
# Current: JSON logs mixed with output
shards list

# Better: Clean output with optional verbosity
shards list          # Clean table
shards list -v       # With extra details
shards list --json   # Machine-readable
shards list --quiet  # Minimal (just branch names)
```

### Confirmation Prompts
```bash
shards destroy feature-auth
# "Destroy shard 'feature-auth'? This will remove the worktree. (y/N)"

shards destroy --force feature-auth  # Skip confirmation
```

### Tab Completion
```bash
# Shell completion for branch names
shards destroy fea<TAB>  # Completes to feature-auth
```

### Fuzzy Matching
```bash
shards destroy auth  # Matches "feature-auth" if unambiguous
shards destroy auth  # "Multiple matches: feature-auth, auth-bugfix. Be more specific."
```

---

## Logging & Observability

### Structured Logging Improvements
```rust
// Add correlation ID for tracking operations
info!(
    event = "session.create_completed",
    correlation_id = %correlation_id,
    session_id = %session.id,
    duration_ms = elapsed.as_millis(),
);
```

### Human-Readable Mode
```bash
SHARDS_LOG_FORMAT=pretty shards create feature-auth
# Instead of JSON, shows:
# [INFO] Creating shard 'feature-auth'...
# [INFO] Worktree created at ~/.shards/worktrees/project/feature-auth
# [INFO] Terminal opened (iTerm, window 12345)
# [SUCCESS] Shard created!
```

### Log Levels
```bash
shards -v create feature-auth   # Verbose (debug)
shards -q create feature-auth   # Quiet (errors only)
```

---

## Configuration Improvements

### Per-Project Defaults
```toml
# ./shards.toml (project root)
[defaults]
agent = "kiro"
template = "feature"

[templates.feature]
agent = "claude"
flags = "--trust-all-tools"

[templates.bugfix]
agent = "kiro"
flags = "--verbose"
```

### Environment Variable Passthrough
```toml
[agent]
env = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"]
```

---

## Quick Wins (Low Effort, High Impact)

These could be implemented quickly and would immediately improve UX:

| Feature | Effort | Impact | Notes |
|---------|--------|--------|-------|
| `--note` flag | Low | High | Just add string field to Session |
| `shards cd` | Low | High | Just print path |
| `--json` output | Low | Medium | Already have data, just format |
| `-q` quiet mode | Low | Medium | Suppress logs |
| `--force` for destroy | Low | Medium | Skip confirmation |
| Fuzzy branch matching | Medium | High | Match partial names |
| `shards open` | Low | High | Shell out to `$EDITOR` or `code` |

---

## What NOT to Build

Based on the core vision, these are out of scope:

| Idea | Why Not |
|------|---------|
| Headless agent execution (`-p` mode) | Future UI concern, not CLI |
| Agent orchestration | Humans work interactively |
| Cross-shard messaging | Agents don't coordinate |
| Automatic task distribution | Human assigns work |
| Token/cost tracking | No visibility, not our concern |
| Agent behavior control | Autonomous within session |

---

## Implementation Priorities

### Phase 1: Quick Wins (Days)
High impact, low effort improvements:
1. `--note` flag for session descriptions
2. `shards cd <branch>` - print worktree path
3. `shards open <branch>` - open in editor
4. `-q` / `--quiet` mode - suppress log noise
5. `--json` output for all list/status commands
6. `--force` flag for destroy (skip confirmation)

### Phase 2: Navigation & Visibility (Week)
1. `shards focus <branch>` - bring terminal to foreground
2. Fuzzy branch matching (partial names)
3. Quick aliases (`s1`, `s2`, etc.)
4. Enhanced `shards status` with git info
5. `shards diff` / `shards commits`

### Phase 3: Workflow Features (Weeks)
1. `shards sync` - keep branch updated with main
2. Templates (`--template`)
3. Bulk operations (`--all`)
4. Session history (archive on destroy)
5. `shards clone` - duplicate shard to new branch

### Phase 4: Advanced (Future)
1. Tab completion (shell integration)
2. `shards arrange` - terminal window positioning
3. Shard groups
4. `shards export` - generate summary markdown
5. GitHub integration (`--issue`, `shards pr`)
6. tmux mode for remote/SSH
7. Conflict detection
8. Daily digest

---

## Future: Native UI

There's a PRD for a native UI (see `.claude/PRPs/plans/`). Key points:
- Will likely use Agent SDK or custom Rust implementation
- Separate concern from CLI
- CLI remains the foundation
- UI builds on top of same session management

The CLI should be complete and solid first - it's the foundation everything else builds on.
