---
description: Create a PR from current branch with implementation context
argument-hint: [base-branch] (default: main)
---

# Create Pull Request

**Base branch**: $ARGUMENTS (default: main)

---

## Phase 1: Gather Context

### 1.1 Check Git State

```bash
git branch --show-current
git status --short
git log origin/main..HEAD --oneline
```

### 1.2 Check for Implementation Report

Look for the most recent implementation report:

```bash
ls -t .archon/artifacts/reports/*-report.md 2>/dev/null | head -1
```

If found, read it to extract:
- Summary of what was implemented
- Files changed
- Validation results
- Any deviations from plan

### 1.3 Get Commit Summary

```bash
git log origin/main..HEAD --pretty=format:"- %s"
```

---

## Phase 2: Prepare Branch

### 2.1 Ensure All Changes Committed

If uncommitted changes exist:

```bash
git status --porcelain
```

**If dirty**:
1. Stage changes: `git add -A`
2. Commit: `git commit -m "Final changes before PR"`

### 2.2 Push Branch

```bash
git push -u origin HEAD
```

---

## Phase 3: Create PR

### 3.1 Determine PR Content

**Title**: Concise, imperative mood
- From implementation report summary, OR
- From commit messages

**Body**: Use this format:

```markdown
## Summary

[Brief description from implementation report or commits]

## Changes

[List from implementation report "Files Changed" section, or from commits]
- file1.ts - description
- file2.ts - description

## Validation

[From implementation report "Validation Results" section]
- [x] Type check passes
- [x] Lint passes
- [x] Tests pass
- [x] Build succeeds

## Testing Notes

[Any manual testing done or integration test results]

---

[If from a GitHub issue, add: Closes #XXX]
```

### 3.2 Create the PR

```bash
gh pr create \
  --title "[title]" \
  --body "[body from above]" \
  --base ${ARGUMENTS:-main}
```

Or if the content is simple:

```bash
gh pr create --fill --base ${ARGUMENTS:-main}
```

---

## Phase 4: Output

Report the result:

```markdown
## PR Created

**URL**: [PR URL]
**Branch**: [branch-name] → [base-branch]
**Title**: [PR title]

### Summary
[Brief summary of what the PR contains]

### Next Steps
1. Request review if needed
2. Address any CI failures
3. Merge when approved
```

---

## Error Handling

### No Commits to Push

```
No commits between origin/main and HEAD.
Nothing to create a PR for.
```

### Branch Already Has PR

```bash
gh pr view --web
```

Opens the existing PR instead of creating a duplicate.

### Push Fails

1. Check if branch exists remotely: `git ls-remote --heads origin [branch]`
2. If conflicts: `git pull --rebase origin main` then retry push
3. If permission issues: Check GitHub access
