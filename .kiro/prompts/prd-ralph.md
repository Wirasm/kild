---
description: Create a PRD for Ralph autonomous coding loop
argument-hint: 
---

# Create PRD for Ralph Loop

---

## Your Mission

First, ask the user what feature they want to build. Once established, create a comprehensive PRD that Ralph can use for autonomous development. Store all files in `.kiro/artifacts/prds/{feature-name}/`.

---

## Phase 1: DISCOVER - What to Build

Ask the user:
- What feature do you want to build?
- What's the main goal or problem this solves?
- Any specific requirements or constraints?

Wait for their response before proceeding.

---

## Phase 2: SETUP - Create Directory Structure

```bash
# Create PRD directory structure
mkdir -p .kiro/artifacts/prds/{feature-name}
```

---

## Phase 3: CREATE - Generate PRD Files

### 3.1 Create prd.json

Create `.kiro/artifacts/prds/{feature-name}/prd.json`:

```json
{
  "branchName": "ralph/{feature-name}",
  "feature": "{feature-name}",
  "description": "Brief description of the feature",
  "userStories": [
    {
      "id": "US-001",
      "title": "Story title",
      "acceptanceCriteria": [
        "Specific, testable criteria",
        "typecheck passes",
        "tests pass"
      ],
      "priority": 1,
      "passes": false,
      "notes": ""
    }
  ]
}
```

### 3.2 Create prompt.md

Create `.kiro/artifacts/prds/{feature-name}/prompt.md`:

```markdown
# Ralph Agent Instructions

## Your Task

1. Read `.kiro/artifacts/prds/{feature-name}/prd.json`
2. Read `.kiro/artifacts/prds/{feature-name}/progress.txt`
3. Check you're on the correct branch
4. Pick highest priority story where `passes: false`
5. Implement that ONE story
6. Run typecheck and tests
7. Commit: `feat: [ID] - [Title]`
8. Update prd.json: `passes: true`
9. Append learnings to progress.txt

## Progress Format

APPEND to progress.txt:

## [Date] - [Story ID]
- What was implemented
- Files changed
- **Learnings:**
  - Patterns discovered
  - Gotchas encountered
---

## Stop Condition

If ALL stories pass, reply:
<promise>COMPLETE</promise>

Otherwise end normally.
```

### 3.3 Create progress.txt

Create `.kiro/artifacts/prds/{feature-name}/progress.txt`:

```markdown
# Ralph Progress Log
Started: {current-date}
Feature: {feature-name}

## Codebase Patterns
- Add patterns as discovered

## Key Files
- List important files as discovered
---
```

### 3.4 Create ralph.sh

Create `.kiro/artifacts/prds/{feature-name}/ralph.sh`:

```bash
#!/bin/bash
set -e

MAX_ITERATIONS=${1:-10}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🚀 Starting Ralph for {feature-name}"

for i in $(seq 1 $MAX_ITERATIONS); do
  echo "═══ Iteration $i ═══"
  
  OUTPUT=$(cat "$SCRIPT_DIR/prompt.md" \
    | kiro-cli chat --no-interactive --trust-all-tools 2>&1 \
    | tee /dev/stderr) || true
  
  if echo "$OUTPUT" | grep -q "<promise>COMPLETE</promise>"; then
    echo "✅ Done!"
    exit 0
  fi
  
  sleep 2
done

echo "⚠️ Max iterations reached"
exit 1
```

Make executable:
```bash
chmod +x .kiro/artifacts/prds/{feature-name}/ralph.sh
```

---

## Phase 4: POPULATE - Create User Stories

Break down the feature into small, implementable stories:

1. **Each story must fit in one context window**
2. **Add specific acceptance criteria** including:
   - Functional requirements
   - Technical requirements (typecheck, tests)
   - Verification steps

### Story Size Guidelines

❌ Too big: "Build entire auth system"
✅ Right size: 
- "Add login form component"
- "Add email validation"
- "Create auth server action"

---

## Phase 5: OUTPUT - Report Created PRD

Show the user:
- Location of created files
- Number of user stories
- Next steps to start Ralph

---

## Success Criteria

- **USER_INPUT**: Asked user what to build and got response
- **PRD_CREATED**: All Ralph files created in `.kiro/artifacts/prds/{feature-name}/`
- **STORIES_DEFINED**: At least one user story with clear criteria
- **READY_TO_RUN**: Ralph can start immediately
