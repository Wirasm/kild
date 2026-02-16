# Claude Code Agent Teams — Research

Research compiled from 4 parallel agents investigating different aspects of Claude Code agent teams.

---

## 1. Team Lifecycle & Spawning

### Enabling Agent Teams

Agent teams are **experimental and disabled by default**. Enable via:
- Environment variable: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- Or in `settings.json`:
```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

### Team Creation

Teams are created via `TeamCreate`:

```
TeamCreate({ team_name: "my-project", description: "..." })
```

This creates:
- **Team config**: `~/.claude/teams/{team-name}/config.json`
- **Task list directory**: `~/.claude/tasks/{team-name}/`
- **Message directories**: `~/.claude/teams/{team-name}/messages/{session-id}/`

The `config.json` stores team name, description, creation timestamp, lead agent ID, and all members (agentId, name, type, color, backend type, pane ID).

Teams have a **1:1 correspondence with task lists** (Team = TaskList).

### Spawning Teammates via the Task Tool

Teammates are spawned using the **Task tool** with additional parameters:

```
Task({
  team_name: "my-project",
  name: "security-reviewer",
  subagent_type: "general-purpose",
  model: "sonnet",
  prompt: "You are a QA agent...",
  run_in_background: true
})
```

When `team_name` and `name` are passed to the Task tool, it spawns a **persistent team member** (not a simple subagent subprocess). The teammate gains access to `TaskList`, `TaskUpdate`, and `SendMessage`.

### Available Agent Types

| Type | Tools | Model |
|------|-------|-------|
| `general-purpose` | Full tool access | Inherits main |
| `Explore` | Read-only, codebase search | Haiku |
| `Plan` | Read-only, research/planning | Inherits main |
| `Bash` | Terminal command execution | Inherits main |
| `claude-code-guide` | Claude Code feature help | Haiku |

Custom agents can be defined in `.claude/agents/` or `~/.claude/agents/` as Markdown files with YAML frontmatter.

### Model Options
- `"sonnet"` — Claude Sonnet (fast, cheaper)
- `"opus"` — Claude Opus (most capable)
- `"haiku"` — Claude Haiku (fastest, cheapest)
- Default: inherits from main conversation

### Full Teammate Lifecycle

#### Phase 1: Spawn
1. Lead creates team via `TeamCreate`
2. Lead spawns teammates via `Task` with `team_name` parameter
3. Each teammate loads project context independently (CLAUDE.md, MCP servers, skills)
4. Teammate receives the spawn prompt but **NOT** the lead's conversation history
5. Teammates start with the lead's permission settings

#### Phase 2: Work (Active)
1. Teammates check `TaskList()` for available tasks
2. Claim tasks via `TaskUpdate({ taskId, status: "in_progress", owner: "my-name" })`
3. File locking prevents race conditions on task claiming
4. Teammates do work, communicate via `SendMessage`
5. Mark tasks complete via `TaskUpdate({ taskId, status: "completed" })`
6. Task dependencies auto-unblock when prerequisites complete

#### Phase 3: Idle
- When a teammate finishes work and has no more tasks, it goes **idle**
- An automatic `idle_notification` is sent to the team lead
- This is **normal expected behavior**, not an error
- Idle teammates can still receive messages, which "wakes them up"
- The `TeammateIdle` hook fires when a teammate is about to go idle (exit code 2 keeps them working)

#### Phase 4: Shutdown
1. Lead sends: `SendMessage({ type: "shutdown_request", recipient: "teammate-name" })`
2. Teammate can approve (terminates) or reject (continues working)
3. Teammates finish their current request/tool call before shutting down

#### Phase 5: Cleanup
- Lead deletes the team via `TeamDelete`
- Fails if any teammates are still active
- Removes team directories and shared resources
- Crashed teammates auto-timeout after **5 minutes** (heartbeat timeout)

### Display Modes (Backend Types)

| Mode | Backend | How It Works | Requirements |
|------|---------|--------------|--------------|
| `in-process` | None (same process) | Teammates in main terminal, Shift+Up/Down to navigate | Any terminal |
| `tmux` (auto) | tmux | Split panes in tmux window | `tmux` installed + `$TMUX` set |
| `tmux` (iTerm2) | iTerm2 | iTerm2 maps tmux control mode panes to native tabs/splits | iTerm2 + `it2` CLI |
| KILD daemon | kild-tmux-shim | Fake tmux that translates to daemon PTY management | `kild create --daemon` |

Configure via `"teammateMode": "auto" | "in-process" | "tmux"` in settings, or CLI: `claude --teammate-mode in-process`.

### Hooks for Quality Gates

- **`TeammateIdle`** — Fires when teammate goes idle; exit code 2 keeps them working with feedback
- **`TaskCompleted`** — Fires when task marked complete; exit code 2 prevents completion
- **`SubagentStart`** / **`SubagentStop`** — Lifecycle hooks for subagent events

### Environment Variables Set for Teammates

| Variable | Purpose |
|----------|---------|
| `CLAUDE_CODE_TEAM_NAME` | Team name |
| `CLAUDE_CODE_AGENT_ID` | Unique agent identifier |
| `CLAUDE_CODE_AGENT_NAME` | Human-readable name |
| `CLAUDE_CODE_AGENT_TYPE` | Agent type/role |
| `CLAUDE_CODE_PLAN_MODE_REQUIRED` | Whether plan approval is needed |

### Orchestration Patterns

- **Leader model**: Hierarchical task direction with centralized control
- **Swarm model**: Parallel processing, self-claiming tasks
- **Pipeline model**: Sequential stages with dependency-based auto-progression
- **Council model**: Multi-perspective decision-making
- **Watchdog model**: Quality monitoring and oversight

---

## 2. Communication Patterns

### SendMessage Types

#### `type: "message"` — Direct Messages (DMs)
- Sends to a **single specific teammate** by name
- Required fields: `recipient`, `content`, `summary`
- The `summary` field is a 5-10 word preview shown in the UI
- **Critical**: Plain text output is NOT visible to teammates. Agents MUST use SendMessage.

```json
{
  "type": "message",
  "recipient": "researcher",
  "content": "Your message here",
  "summary": "Brief status update on auth module"
}
```

#### `type: "broadcast"` — Message All Teammates
- Sends the **same message to every teammate** simultaneously
- Each broadcast = N separate message deliveries
- **Use sparingly**: only for critical blocking issues or major team-wide announcements

```json
{
  "type": "broadcast",
  "content": "Message to send to all teammates",
  "summary": "Critical blocking issue found"
}
```

#### `type: "shutdown_request"` — Request Teammate Shutdown
```json
{
  "type": "shutdown_request",
  "recipient": "researcher",
  "content": "Task complete, wrapping up the session"
}
```

#### `type: "shutdown_response"` — Respond to Shutdown Request
Must extract `requestId` from the incoming JSON message. Simply acknowledging in text is NOT enough.

```json
// Approve (terminates the agent)
{ "type": "shutdown_response", "request_id": "abc-123", "approve": true }

// Reject (continues working)
{ "type": "shutdown_response", "request_id": "abc-123", "approve": false, "content": "Still working on task #3" }
```

#### `type: "plan_approval_response"` — Approve/Reject Teammate Plans
```json
// Approve
{ "type": "plan_approval_response", "request_id": "abc-123", "recipient": "researcher", "approve": true }

// Reject with feedback
{ "type": "plan_approval_response", "request_id": "abc-123", "recipient": "researcher", "approve": false, "content": "Please add error handling" }
```

### Automatic Message Delivery

- Messages are delivered **automatically** — no polling needed
- Messages queue during active turns and deliver between API round-trips
- When an idle teammate receives a message, it **wakes up** and processes it

### Idle Notification System

- When a teammate finishes and stops, they automatically notify the lead
- **TeammateIdle hook** fires when an agent is about to go idle:
  - Exit code 0: allows idle
  - Exit code 2: sends stderr as feedback, prevents idle
- Hook input includes: `session_id`, `transcript_path`, `cwd`, `permission_mode`, `teammate_name`, `team_name`

### Peer DM Summaries (Team Lead Visibility)

When teammates message each other directly:
- A **brief summary** is included in their idle notification to the lead
- Gives the lead visibility into peer collaboration without full message content
- The `summary` field in SendMessage serves this purpose

### Plan Approval Flow

1. Teammate spawned with `plan_mode_required: true` (read-only tools)
2. Teammate creates plan using Read, Grep, Glob tools
3. Teammate calls `ExitPlanMode` → sends `plan_approval_request` to lead
4. Lead reviews and sends `plan_approval_response` (approve or reject with feedback)
5. If approved: teammate exits plan mode, gains full tool access
6. If rejected: teammate revises and resubmits

### Best Practices

1. **Default to DMs over broadcast** — broadcasts are expensive
2. **Always use SendMessage** — plain text output is NOT visible to teammates
3. **Include meaningful summaries** — used for UI previews and peer DM summaries
4. **Refer to teammates by name**, never by UUID
5. **Don't send structured JSON status messages** — use `TaskUpdate` instead
6. **Give teammates enough context** in spawn prompts — they don't inherit conversation history
7. **Avoid file conflicts** — break work so each teammate owns different files
8. **Use delegate mode** (Shift+Tab) to restrict the lead to coordination-only tools

---

## 3. Task Coordination

### Task Management Tools

**TaskCreate** — Creates a new task in the shared task list.
- Fields: `subject` (imperative title), `description` (detailed requirements), `activeForm` (present continuous form for spinner), optional `metadata`
- All tasks created with status `pending` and no owner

**TaskList** — Lists all tasks in summary form.
- Returns: `id`, `subject`, `status`, `owner`, `blockedBy`
- Omits `description`, `activeForm`, `metadata` — use TaskGet for full details

**TaskGet** — Retrieves full details for a single task by ID.
- Returns: `subject`, `description`, `status`, `blocks`, `blockedBy`, `owner`, `metadata`

**TaskUpdate** — Updates task status, ownership, dependencies, and details.
- Fields: `taskId` (required), `status`, `owner`, `addBlocks`, `addBlockedBy`, `subject`, `description`, `activeForm`, `metadata`

### Task Dependencies (blocks/blockedBy)

- **addBlockedBy**: Declares which tasks must complete first. Example: `TaskUpdate({ taskId: "3", addBlockedBy: ["1", "2"] })` means task 3 waits for tasks 1 and 2.
- **addBlocks**: The inverse — mark that the current task blocks others.
- **Auto-unblocking**: When a blocking task completes, dependent tasks automatically become available.
- **Pipeline pattern**: Create sequential workflows: Task 1 (implement) blocks Task 2 (test) blocks Task 3 (document).

### Agent Claiming via Owner Field

- Tasks start with no owner (available to any agent)
- Agents claim via: `TaskUpdate({ taskId: "X", owner: "my-agent-name" })`
- Lead can assign: `TaskUpdate({ taskId: "X", owner: "researcher" })`
- File locking prevents race conditions
- Only claim tasks that are `pending`, no owner, empty `blockedBy`

### Recommended Workflow

1. **Lead creates tasks** with TaskCreate, sets dependencies
2. **Agents discover work** via TaskList (pending, no owner, no blockers)
3. **Agents claim** via TaskUpdate (set owner + in_progress)
4. **Agents work** — read full requirements via TaskGet, implement
5. **Agents mark complete** via TaskUpdate (status: completed)
6. **Dependencies auto-unblock**
7. **Agents loop** — TaskList again for next available task
8. **Lead synthesizes** results

Prefer tasks in **ID order** (lowest first) when multiple are available.

### Task Status Progression

```
pending → in_progress → completed
                        (or deleted)
```

- **pending**: Exists but no one working on it. May be blocked.
- **in_progress**: Agent claimed and actively working. `activeForm` shown in spinner.
- **completed**: Done. Auto-unblocks dependent tasks.
- **deleted**: Permanently removed.

**Critical**: Only mark `completed` when FULLY accomplished. If blocked or errored, keep as `in_progress`.

### Task Creation by Agents

Any agent can create new tasks during work:
- Discovering additional work during implementation
- Breaking large tasks into sub-tasks
- Creating follow-up tasks for issues found
- Adding tasks for blockers that need resolution

### Shared Task List Storage

- Task data: `~/.claude/tasks/{team-name}/`
- Each task is a JSON file on disk
- File locking provides concurrency control
- All agents share the same namespace

### Work Breakdown Patterns

**Sizing**: 5-6 tasks per teammate is the sweet spot.

**Effective patterns**:
- **Parallel specialists**: Each agent handles different aspects (security, performance, tests) — no file overlap
- **Sequential pipelines**: Dependencies create stages (implement → test → document)
- **Self-organizing swarms**: Pool of independent tasks, agents claim at own pace
- **File ownership boundaries**: Each teammate owns different files to prevent conflicts

**Anti-patterns**:
- Tasks too small (coordination overhead exceeds benefit)
- Tasks too large (risk of wasted effort)
- Multiple agents editing the same file (leads to overwrites)
- Not enough tasks (agents idle while others overloaded)

---

## 4. Architecture & tmux Backend

### Why tmux?

Claude Code's agent teams need a mechanism to spawn and manage multiple independent Claude Code instances. tmux was chosen because:
- **Isolated PTY environments**: Each teammate runs in its own pseudo-terminal
- **Pane management built-in**: Splitting, listing, sending keystrokes, destroying panes
- **Developer-familiar**: Power users already use tmux
- **Process isolation**: Each pane is a separate process
- **Persistence**: tmux sessions survive terminal disconnects

### tmux's Role

tmux serves as the **process orchestration layer**, not the communication layer:

**tmux handles**: spawning instances (`split-window`), sending commands (`send-keys`), listing panes (`list-panes`), destroying panes (`kill-pane`), querying info (`display-message`), layout management.

**Communication uses**: file-based inbox system (`~/.claude/teams/{name}/inboxes/`) and shared task list (`~/.claude/tasks/{name}/`).

### How Claude Code Detects tmux

1. Checks `$TMUX` environment variable
2. `teammateMode: "auto"` triggers split-pane mode when `$TMUX` is present
3. `$TMUX_PANE` identifies current pane (e.g., `%0`, `%1`)
4. Also checks for iTerm2's `it2` CLI as an alternative

### tmux Commands Used for Teams

| Command | Purpose |
|---------|---------|
| `split-window -h/-v` | Create new pane for teammate |
| `split-window -P -F "#{pane_id}"` | Create pane and print its ID |
| `send-keys -t %N` | Send keystrokes to a pane |
| `list-panes -F "#{pane_id}"` | Enumerate active panes |
| `kill-pane -t %N` | Destroy a teammate's pane |
| `display-message -p "#{pane_id}"` | Query current pane ID |
| `select-pane -t %N -P style -T title` | Set pane styling and title |
| `set-option -p/-w key value` | Configure pane/window options |
| `select-layout` | Arrange pane layout |
| `resize-pane -x -y` | Resize panes |
| `has-session -t name` | Check if session exists |
| `new-session -d -s name` | Create detached session |
| `new-window -n name` | Create new window |
| `list-windows` | List windows in session |
| `break-pane` | Move pane to its own window |
| `join-pane` | Move pane back into a window |
| `capture-pane -p -S N` | Read scrollback buffer content |

**Known issue**: Rapid parallel `send-keys` to panes in the same window can cause command corruption (keystrokes interleave).

### KILD's tmux Shim Architecture

KILD needs agent teams to work inside daemon-managed PTY sessions without real tmux. The solution: **intercept tmux calls with a shim binary**.

#### Setup Flow (`daemon_helpers.rs`)

1. `kild create --daemon` triggers `ensure_shim_binary()`
2. `kild-tmux-shim` binary is symlinked as `~/.kild/bin/tmux`
3. `~/.kild/bin` is prepended to `$PATH` in the daemon PTY environment
4. `$TMUX` set to `<daemon.sock>,<pid>,0` (looks like real tmux)
5. `$TMUX_PANE` set to `%0` (leader pane)
6. `$KILD_SHIM_SESSION` set to session ID for state lookup
7. A `ZDOTDIR` wrapper prevents macOS `path_helper` from reordering `$PATH`

#### How the Shim Translates Commands

**`split-window`** (creating a teammate):
- Parses tmux args (`-h`, `-t %N`, `-P`, `-F format`)
- Creates a new daemon PTY via IPC (`ClientMessage::CreateSession`)
- Registers pane in `~/.kild/shim/<session>/panes.json`
- Allocates pane ID (`%1`, `%2`, etc.)
- Returns pane ID via stdout if `-P` flag present

**`send-keys`** (sending input):
- Resolves target pane from `-t %N` or `$TMUX_PANE`
- Looks up pane's `daemon_session_id` in registry
- Translates key names: `Enter` → `\n`, `Space` → ` `, `Tab` → `\t`, `C-c` → `0x03`, etc.
- Sends bytes to daemon PTY stdin via IPC (`ClientMessage::WriteStdin`)

**`kill-pane`** (destroying a teammate):
- Looks up pane in registry
- Sends `ClientMessage::DestroySession` to daemon
- Removes pane from registry

**`capture-pane`** (reading scrollback):
- Sends `ClientMessage::ReadScrollback` to daemon
- Receives base64-encoded content
- Supports `-S -N` for "last N lines"
- Prints to stdout with `-p` flag

**`display-message`** (querying info):
- Expands format strings: `#{pane_id}`, `#{session_name}`, `#{window_name}`, `#{pane_title}`
- Simple queries return env vars directly

**No-op commands**: `select-layout`, `resize-pane` (meaningless without visual multiplexer)

#### Architecture Diagram

```
┌─────────────────────────────────────────────┐
│ Claude Code (team lead)                     │
│  - Detects $TMUX → uses tmux pane backend   │
│  - Spawns teammates via tmux commands        │
│  - Communication via inbox files + tasks     │
└──────────────────┬──────────────────────────┘
                   │ tmux split-window / send-keys / etc.
                   ▼
┌─────────────────────────────────────────────┐
│ tmux (real) OR kild-tmux-shim (fake)        │
│  Real: actual tmux multiplexer              │
│  Shim: translates to daemon IPC             │
│    - split-window → CreateSession           │
│    - send-keys → WriteStdin                 │
│    - kill-pane → DestroySession             │
│    - capture-pane → ReadScrollback          │
│    - State in ~/.kild/shim/<sid>/panes.json │
└──────────────────┬──────────────────────────┘
                   │ (shim path only)
                   ▼
┌─────────────────────────────────────────────┐
│ kild-daemon (PTY manager)                   │
│  - Unix socket IPC (JSONL protocol)         │
│  - Creates/destroys PTY sessions            │
│  - Manages stdin/stdout for each PTY        │
│  - Stores scrollback buffers                │
│  - Session state machine                    │
└─────────────────────────────────────────────┘
```

### Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `$TMUX` | Triggers tmux pane backend | `/tmp/tmux-501/default,12345,0` (real) or `~/.kild/daemon.sock,67890,0` (shim) |
| `$TMUX_PANE` | Current pane ID | `%0` (leader), `%1` (teammate) |
| `$KILD_SHIM_SESSION` | Links shim to state directory | `myproject_feature-branch` |
| `$KILD_SHIM_LOG` | Enables shim debug logging | `1` or file path |
| `$KILD_SESSION_BRANCH` | Branch name for Codex integration | `feature-branch` |
| `$CLAUDE_CODE_TASK_LIST_ID` | Shared task list ID | `kild-myproject_feature-branch` |
| `$CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` | Feature flag | `1` |

---

## Key Limitations

- **Experimental**: gated behind environment variable
- **Token cost**: ~800k tokens for 3-agent team vs ~200k solo
- **No nested teams**: teammates cannot spawn their own teams
- **One team per session**: clean up before starting new
- **No session resumption** for in-process teammates
- **Task status lag**: teammates sometimes forget to mark tasks complete
- **Shutdown can be slow**: waits for current request to finish
- **Lead is fixed** for team lifetime
- **File conflicts**: multiple agents editing the same file leads to overwrites

---

## Sources

- [Official Claude Code Agent Teams Docs](https://code.claude.com/docs/en/agent-teams)
- [Official Claude Code Sub-Agents Docs](https://code.claude.com/docs/en/sub-agents)
- [Official Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks)
- [From Tasks to Swarms: Agent Teams in Claude Code](https://alexop.dev/posts/from-tasks-to-swarms-agent-teams-in-claude-code/)
- [Claude Code's Hidden Swarm](https://paddo.dev/blog/claude-code-hidden-swarm/)
- [Claude Code Swarm Orchestration Skill (GitHub Gist)](https://gist.github.com/kieranklaassen/4f2aba89594a4aea4ad64d753984b2ea)
- [Addy Osmani: Claude Code Agent Teams](https://addyosmani.com/blog/claude-code-agent-teams/)
- [Claude Code Task Management (claudefast)](https://claudefa.st/blog/guide/development/task-management)
- [Claude Code Agent Teams Guide (claudefast)](https://claudefa.st/blog/guide/agents/agent-teams)
- [Claude Code Multi-Agent tmux Setup](https://www.dariuszparys.com/claude-code-multi-agent-tmux-setup/)
- [Task tool system prompts (GitHub)](https://github.com/Piebald-AI/claude-code-system-prompts/blob/main/system-prompts/tool-description-taskcreate.md)
