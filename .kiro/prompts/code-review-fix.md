---
description: Process to fix bugs found in manual/AI code review
---

# Fix Code Review Issues

## First Step: Get Review Information

Ask the user these questions:
1. "What code review file or description of issues should I address?"
2. "What is the scope of the fixes?"

## Process

I ran/performed a code review and found these issues:

Code-review (file or description of issues): [User's response to question 1]

Please fix these issues one by one. If the Code-review is a file read the entire file first to understand all of the issue(s) presented there.

Scope: [User's response to question 2]

For each fix:
1. Explain what was wrong
2. Show the fix
3. Create and run relevant tests to verify

After all fixes, run the validate command (see commands/validate.md) to finalize your fixes.