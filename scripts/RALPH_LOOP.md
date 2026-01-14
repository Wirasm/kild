# Ralph Loop - Autonomous Implementation Agent

Ralph is an autonomous agent loop that implements features by iteratively working through user stories defined in a PRD.

## Quick Start

```bash
# Run Ralph on a specific PRD
./scripts/ralph-loop.sh .kiro/artifacts/prds/my-feature

# With custom max iterations
./scripts/ralph-loop.sh .kiro/artifacts/prds/my-feature 30
```

## How It Works

### Context Management

Ralph provides two types of context to each iteration:

1. **Meta Context** (constant across iterations):
   - Implementation rules and patterns
   - Architecture guidelines
   - Code style requirements
   - From `prompt.md` in the PRD directory

2. **Iteration Context** (changes each iteration):
   - Current iteration number
   - Previous progress from `progress.txt`
   - Current PRD state from `prd.json`
   - Working directory location

### Iteration Flow

Each iteration:

1. **Reads** `prd.json` to find incomplete stories
2. **Reads** `progress.txt` to understand previous work
3. **Picks** highest priority story where `passes: false`
4. **Implements** that ONE story completely
5. **Updates** `prd.json` to mark `passes: true`
6. **Appends** learnings to `progress.txt`

### Stop Conditions

Ralph stops when:
- All user stories have `passes: true` (success)
- Max iterations reached (timeout)
- Agent outputs `<promise>COMPLETE</promise>` (explicit completion)

## PRD Directory Structure

```
.kiro/artifacts/prds/my-feature/
├── prd.json          # User stories with pass/fail status
├── prompt.md         # Meta context and implementation rules
└── progress.txt      # Iteration history and learnings
```

### prd.json Format

```json
{
  "name": "Feature Name",
  "userStories": [
    {
      "id": "US001",
      "title": "Story title",
      "description": "What to implement",
      "acceptanceCriteria": ["Criterion 1", "Criterion 2"],
      "passes": false,
      "priority": 1
    }
  ]
}
```

### prompt.md Format

Contains the meta context that applies to all iterations:

```markdown
# Implementation Rules

1. Work ONE user story at a time
2. Follow existing patterns
3. Use vertical slice architecture
4. Maintain compatibility
5. Validate immediately

# Key Dependencies

- dependency1 = "version"
- dependency2 = "version"

# Patterns to Mirror

[Code examples from existing codebase]
```

### progress.txt Format

Append-only log of what each iteration accomplished:

```markdown
# Progress Log - Feature Name

Started: 2026-01-14T10:00:00Z

## 2026-01-14T10:15:00Z - US001
- Implemented feature X
- Files changed: src/module/file.rs
- **Learnings:**
  - Pattern discovered: Use Arc<Mutex<T>> for shared state
  - Gotcha: GPUI requires git dependency

---

## 2026-01-14T10:30:00Z - US002
...
```

## Key Improvements

### 1. Combined Context Delivery

- **Before**: Prompt sent in fragments, causing agent confusion
- **After**: Complete prompt with meta + iteration context in one message

### 2. Relative Working Directory

- **Before**: Hardcoded paths, didn't work in worktrees
- **After**: Works from any directory, uses relative paths

### 3. Trust All Tools

- **Before**: Required manual approval for every tool
- **After**: Uses `--trust-all-tools` flag for autonomous operation

### 4. Progress Tracking

- **Before**: No visibility into what previous iterations did
- **After**: Each iteration sees complete progress history

### 5. Clear Task Definition

- **Before**: Agent had to figure out what to do
- **After**: Explicit task for current iteration with context

## Troubleshooting

### Agent Can't Find Files

Check that you're running Ralph from the correct directory:

```bash
# Should be in project root or worktree root
pwd
ls -la .kiro/artifacts/prds/
```

### Agent Stuck in Loop

Check `progress.txt` to see if agent is making progress:

```bash
tail -20 .kiro/artifacts/prds/my-feature/progress.txt
```

If stuck on same story, the acceptance criteria may be unclear.

### Max Iterations Reached

Increase the iteration limit:

```bash
./scripts/ralph-loop.sh .kiro/artifacts/prds/my-feature 50
```

Or break the PRD into smaller chunks with fewer user stories.

## Best Practices

### 1. Clear Acceptance Criteria

Each user story should have specific, testable acceptance criteria:

```json
{
  "acceptanceCriteria": [
    "cargo check passes with new dependencies",
    "GPUI git dependency resolves correctly"
  ]
}
```

### 2. Atomic User Stories

Each story should be independently implementable:

- ✅ "Add GPUI dependency to Cargo.toml"
- ❌ "Implement entire GUI system"

### 3. Provide Examples

Include code examples in `prompt.md` for patterns to follow:

```markdown
## Error Handling Pattern

\`\`\`rust
// SOURCE: src/sessions/errors.rs:4-15
#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    #[error("PTY process failed: {message}")]
    PtyError { message: String },
}
\`\`\`
```

### 4. Monitor Progress

Watch `progress.txt` to ensure agent is learning and making progress:

```bash
watch -n 5 tail -20 .kiro/artifacts/prds/my-feature/progress.txt
```

## Examples

### Running Ralph on GUI Feature

```bash
./scripts/ralph-loop.sh .kiro/artifacts/prds/shards-gui-gpui-pty
```

### Running Ralph in a Worktree

```bash
cd ~/tmp/worktrees/shards/feature-gui-gpui-pty
./scripts/ralph-loop.sh .kiro/artifacts/prds/shards-gui-gpui-pty
```

### Custom Iteration Limit

```bash
./scripts/ralph-loop.sh .kiro/artifacts/prds/my-feature 100
```
