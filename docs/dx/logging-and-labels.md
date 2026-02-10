# Logging and Labels: A Unified System

This project uses a unified naming convention across structured logs and GitHub labels. The same grep patterns work on both.

## The Convention

Both systems use: `{layer}.{domain}`

**Layers:**
- `core` - Library logic (`crates/kild-core/`)
- `cli` - User-facing commands (`crates/kild/`)
- `ui` - GPUI native GUI (`crates/kild-ui/`)
- `peek.core` - kild-peek core library (`crates/kild-peek-core/`)
- `peek.cli` - kild-peek CLI (`crates/kild-peek/`)

**Domains:**
- `session` - Session lifecycle
- `terminal` - Terminal backends (core), terminal rendering (ui)
- `git` - Worktree operations
- `cleanup` - Orphaned resource cleanup
- `health` - Health monitoring
- `process` - PID tracking
- `config` - Configuration system
- `agents` - Agent backends
- `files` - File operations
- `input` - Keystroke translation (UI)
- `projects` - Project management (UI)
- `watcher` - File system watcher (UI)
- `window` - Window enumeration (kild-peek)
- `screenshot` - Screenshot capture (kild-peek)
- `diff` - Image comparison (kild-peek)
- `assert` - UI state assertions (kild-peek)
- `interact` - UI interaction: clicks, typing, key combos (kild-peek)
- `element` - Accessibility element enumeration and finding (kild-peek)

## In Practice

**Logs** use `{layer}.{domain}.{action}_{state}`:
```
core.session.create_started
core.session.create_completed
core.terminal.spawn_failed
cli.list_started
ui.watcher.started
ui.projects.add_completed
peek.core.screenshot.capture_completed
```

**Labels** use `{layer}.{domain}`:
```
core.session
core.terminal
cli
ui.watcher
peek.core.screenshot
```

## Why This Matters

Same grep pattern, both systems:

```bash
# Find all session-related log entries
grep 'core\.session' logs.json

# Find all watcher-related log entries
grep 'ui\.watcher' logs.json

# Find all session-related issues
gh issue list --label "core.session"

# Export issues and grep the same way
gh issue list --json number,title,labels | grep 'core\.session'
```

## Label Categories

| Category | Pattern | Examples |
|----------|---------|----------|
| Type | bare name | `bug`, `feature`, `chore` |
| Effort | `effort/{level}` | `effort/low`, `effort/high` |
| Priority | `P{n}` | `P0`, `P1`, `P2`, `P3` |
| Area | `{layer}.{domain}` | `core.session`, `cli` |

## Triage Command

Use `/triage` to automatically label issues following this convention. It reads issue content, applies appropriate labels, and detects duplicates/relationships.

```bash
/triage           # Unlabeled issues only
/triage all       # All open issues
/triage 67        # Specific issue
/triage 60-67     # Range
```
