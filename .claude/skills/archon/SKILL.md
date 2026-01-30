---
name: archon
description: |
  Use when: User wants to run Archon CLI workflows OR set up Archon for the first time.
  Triggers (workflows): "use archon to", "run archon", "archon workflow", "use archon for",
            "have archon", "let archon", "ask archon to".
  Triggers (setup): "set up archon", "install archon", "how to use archon",
            "configure archon", "archon setup", "get started with archon".
  Triggers (config): "change my archon config", "modify archon config", "archon config",
            "change archon settings", "update my config", "help me change my config",
            "edit archon config", "archon configuration".
  Capability: Runs AI workflows in isolated git worktrees for parallel development.
  NOT for: Direct Claude Code work - only for delegating to Archon CLI.
argument-hint: "[workflow] [message or issue number]"
---

# Archon CLI Skill

Archon is a remote agentic coding platform that runs AI workflows in isolated git worktrees. This skill teaches you how to invoke Archon workflows from the command line, and guides first-time setup.

## Available Workflows (live)

!`archon workflow list 2>&1 || echo "Archon CLI not installed. Run /archon to set it up."`

## Routing

**Determine the user's intent:**

- **Setup / install / "how to use" intent** → Read `guides/setup.md` and follow the interactive wizard using AskUserQuestion.
- **Config / settings intent** → Read `guides/config.md` and follow the interactive config editor. This covers viewing, modifying, and understanding both global (`~/.archon/config.yaml`) and repo-level (`.archon/config.yaml`) configuration. Available at any time, not just during setup.
- **Workflow intent (default)** → Continue with workflow invocation below.

---

## Core Command

```bash
archon workflow run <workflow-name> --branch <branch-name> "<message>"
```

**CRITICAL RULES**:

1. **Always run in background** - Archon workflows are long-running. Always invoke the Bash tool with `run_in_background: true`. This allows you to continue working while Archon runs. Use `/tasks` or the TaskOutput tool to check on progress.

2. **Always use worktree isolation** - Use the `--branch` flag unless the user explicitly requests otherwise. This creates an isolated environment so Archon can work without affecting the main branch.

## Isolation Modes

| Mode | Flag | When to Use |
|------|------|-------------|
| **Worktree (Default)** | `--branch <name>` | Always use this unless told otherwise |
| **Direct checkout** | `--no-worktree` | Only if user explicitly requests no isolation |
| **No isolation** | (no flag) | Only if user explicitly says "in current directory" |

## Available Workflows

### `archon-fix-github-issue`
**Use when**: User wants to fix, resolve, or implement a solution for a GitHub issue.
**Triggers**: "fix issue #123", "resolve this bug", "implement issue", "fix it"
**Does**: Investigates root cause → creates implementation plan → makes code changes → creates PR

```bash
archon workflow run archon-fix-github-issue --branch fix/issue-123 "Fix issue #123"
```

### `archon-feature-development`
**Use when**: Implementing a feature from an existing plan.
**Triggers**: "implement the plan", "execute plan", "implement feature from plan"
**Does**: Implements the plan with validation loops → creates pull request
**Requires**: Path to a plan file or GitHub issue containing a plan

```bash
archon workflow run archon-feature-development --branch feat/my-feature "Implement plan from .archon/artifacts/plans/my-feature.plan.md"
```

### `archon-comprehensive-pr-review`
**Use when**: User wants a thorough code review of a pull request.
**Triggers**: "review PR #123", "comprehensive review", "full PR review", "review and fix"
**Does**: Syncs with main → runs 5 specialized review agents → auto-fixes critical issues → reports findings

```bash
archon workflow run archon-comprehensive-pr-review --branch review/pr-123 "Review PR #123"
```

### `archon-resolve-conflicts`
**Use when**: PR has merge conflicts that need resolution.
**Triggers**: "resolve conflicts", "fix merge conflicts", "rebase this PR"
**Does**: Fetches latest base → analyzes conflicts → auto-resolves where possible → presents options for complex ones

```bash
archon workflow run archon-resolve-conflicts --branch resolve/pr-123 "Resolve conflicts in PR #123"
```

### `archon-ralph-fresh`
**Use when**: Implementing a PRD with many stories (7+) that are independent.
**Triggers**: "ralph-fresh", "run ralph-fresh", "fresh ralph", "stateless ralph"
**Does**: Iterates through stories with fresh context each iteration (no memory between iterations)
**Requires**: `.archon/ralph/{feature}/prd.md` and `prd.json`

```bash
archon workflow run archon-ralph-fresh --branch feat/my-prd "Run ralph on .archon/ralph/my-feature"
```

### `archon-ralph-stateful`
**Use when**: Implementing a PRD with few tightly-coupled stories (under 5-7).
**Triggers**: "ralph-stateful", "run ralph-stateful", "stateful ralph"
**Does**: Iterates through stories with persistent memory across iterations
**Requires**: `.archon/ralph/{feature}/prd.md` and `prd.json`

```bash
archon workflow run archon-ralph-stateful --branch feat/my-prd "Run ralph on .archon/ralph/my-feature"
```

### `archon-assist`
**Use when**: No other workflow matches - general questions, debugging, exploration, one-off tasks.
**Triggers**: "help with", "explain", "debug", "explore", general questions
**Does**: Full Claude Code agent with all tools available

```bash
archon workflow run archon-assist --branch assist/task-name "What does the orchestrator do?"
```

## Branch Naming Conventions

Use descriptive branch names that match the task:

| Task Type | Branch Pattern | Example |
|-----------|---------------|---------|
| Fix issue | `fix/issue-{number}` | `fix/issue-123` |
| Feature | `feat/{name}` | `feat/dark-mode` |
| PR review | `review/pr-{number}` | `review/pr-456` |
| Resolve conflicts | `resolve/pr-{number}` | `resolve/pr-456` |
| General assist | `assist/{description}` | `assist/debug-auth` |

## Workflow Selection Guide

When the user says... → Use this workflow:

| User Intent | Workflow |
|-------------|----------|
| "Fix issue #X" / "Resolve bug #X" | `archon-fix-github-issue` |
| "Implement the plan" / "Execute plan" | `archon-feature-development` |
| "Review PR #X" / "Code review" | `archon-comprehensive-pr-review` |
| "Resolve conflicts" / "Fix merge conflicts" | `archon-resolve-conflicts` |
| "Run ralph" (many independent stories) | `archon-ralph-fresh` |
| "Run ralph" (few coupled stories) | `archon-ralph-stateful` |
| General questions / debugging / exploration | `archon-assist` |

## Other CLI Commands

### List available workflows
```bash
archon workflow list
```

### List active worktrees
```bash
archon isolation list
```

### Clean up stale worktrees (default: 7 days)
```bash
archon isolation cleanup
archon isolation cleanup 14  # Custom: 14 days
```

### Show version
```bash
archon version
```

## Important Notes

1. **Always run from the repository root** - The CLI needs to be in a git repo
2. **Worktree isolation is the default** - Archon works best in isolated environments
3. **One workflow per shell** - Each workflow blocks its shell. Use `run_in_background: true` to run multiple workflows in parallel via separate background tasks.
4. **Check isolation list** - Use `archon isolation list` to see active environments
5. **Clean up periodically** - Use `archon isolation cleanup` to remove stale worktrees

## Multi-Issue Invocation

**CRITICAL**: When the user mentions multiple issues (e.g., "fix issues #1, #2, and #3"), you must run the workflow **separately for each issue**. Do NOT combine them into a single command.

Each issue needs its own:
- Separate workflow invocation
- Separate branch name
- Separate worktree

**Correct approach** - Run each as a separate background task (they can run in parallel):
```bash
# Issue #1 (background)
archon workflow run archon-fix-github-issue --branch fix/issue-1 "Fix issue #1"

# Issue #2 (background)
archon workflow run archon-fix-github-issue --branch fix/issue-2 "Fix issue #2"

# Issue #3 (background)
archon workflow run archon-fix-github-issue --branch fix/issue-3 "Fix issue #3"
```

Each gets its own worktree, so they won't conflict. Use `run_in_background: true` on each Bash invocation.

**WRONG** - Never combine multiple issues into one command:
```bash
# DON'T DO THIS - it won't work correctly
archon workflow run archon-fix-github-issue --branch fix/issues "Fix issues #1, #2, and #3"
```

This same pattern applies to multiple PRs, multiple plans, or any batch of similar tasks.

## Example Interactions

**User**: "Use Archon to fix issue #42"
```bash
archon workflow run archon-fix-github-issue --branch fix/issue-42 "Fix issue #42"
```

**User**: "Have Archon review PR #15"
```bash
archon workflow run archon-comprehensive-pr-review --branch review/pr-15 "Review PR #15"
```

**User**: "Use Archon to implement the dark mode feature plan"
```bash
archon workflow run archon-feature-development --branch feat/dark-mode "Implement plan from .archon/artifacts/plans/dark-mode.plan.md"
```

**User**: "Ask Archon to help debug the authentication flow"
```bash
archon workflow run archon-assist --branch assist/debug-auth "Debug the authentication flow"
```

**User**: "Use Archon to fix issue #99 without isolation" (explicit no isolation)
```bash
archon workflow run archon-fix-github-issue "Fix issue #99"
```

**User**: "Use Archon to fix issues #10, #11, and #12" (multiple issues — run in parallel as background tasks)
```bash
archon workflow run archon-fix-github-issue --branch fix/issue-10 "Fix issue #10"
archon workflow run archon-fix-github-issue --branch fix/issue-11 "Fix issue #11"
archon workflow run archon-fix-github-issue --branch fix/issue-12 "Fix issue #12"
```
