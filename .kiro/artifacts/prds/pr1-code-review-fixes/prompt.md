# Ralph Agent Instructions

## Your Task

1. Read `.kiro/artifacts/prds/pr1-code-review-fixes/prd.json`
2. Read `.kiro/artifacts/prds/pr1-code-review-fixes/progress.txt`
3. Check you're on the correct branch (`ralph/file-based-persistence`)
4. Pick highest priority story where `passes: false`
5. Implement that ONE story
6. Run typecheck and tests: `cargo check && cargo test`
7. Commit: `feat: [ID] - [Title]`
8. Update prd.json: `passes: true` for completed story
9. Append learnings to progress.txt

## Context

You are fixing critical and important issues found in PR #1 code review for the file-based persistence system. Focus on:

- **Silent failure patterns** - Add proper logging and user feedback
- **Error handling** - Preserve error context and provide helpful messages  
- **Test coverage** - Add missing tests for critical functionality
- **Race conditions** - Fix TOCTOU issues and improve atomicity

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
