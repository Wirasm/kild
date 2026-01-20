# Investigation: Fix .kiro prompts to check current branch before creating new ones

**Issue**: #29 (https://github.com/Wirasm/shards/issues/29)
**Type**: BUG
**Investigated**: 2026-01-20T15:13:17.286+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | HIGH | Causes workflow confusion and duplicate branches during PR merges, affecting all agent workflows |
| Complexity | LOW | Only 2 files need updating with simple branch checking logic, isolated change |
| Confidence | HIGH | Clear root cause identified with specific file:line evidence and concrete examples |

---

## Problem Statement

The .kiro prompts instruct agents to create new branches even when they're already on the correct branch for their work, causing duplicate branches like `worktree-worktree-issue-X` during PR merges.

---

## Analysis

### Root Cause / Change Rationale

WHY: Agents create duplicate branches like `worktree-worktree-issue-16-empty-terminal`
↓ BECAUSE: Prompts always create new branches without checking current branch name
  Evidence: `.kiro/prompts/implement-issue.md:146-147` - `├─ YES → Create branch: fix/issue-{number}-{slug}`

↓ BECAUSE: Decision tree only checks if on main/master, not if already on appropriate branch
  Evidence: `.kiro/prompts/implement-issue.md:134-160` - Missing current branch name validation

↓ ROOT CAUSE: No logic to detect if current branch already matches the intended work
  Evidence: Current branch `worktree-issue-29-fix-kiro-prompts-branch-check` demonstrates the pattern

### Evidence Chain

WHY: Duplicate branches created during PR merges
↓ BECAUSE: Agents create new branches without checking current branch appropriateness
  Evidence: `.kiro/prompts/implement-issue.md:146-147` - `git checkout -b fix/issue-{number}-{slug}`

↓ BECAUSE: Decision tree lacks branch name pattern matching
  Evidence: `.kiro/prompts/implement-plan.md:76` - `Create branch: git checkout -b feature/{plan-slug}`

↓ ROOT CAUSE: Missing current branch validation logic in prompt decision trees
  Evidence: Both files check main/master but not branch name patterns

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `.kiro/prompts/implement-issue.md` | 134-160 | UPDATE | Add branch name checking to decision tree |
| `.kiro/prompts/implement-plan.md` | 71-78 | UPDATE | Add branch name checking to decision table |

### Integration Points

- All AI agents use these prompts for implementation workflows
- Affects branch creation during issue implementation and plan execution
- Impacts PR merge workflows and branch naming consistency

### Git History

- **Introduced**: a46cf56b - 2026-01-12 - "Update .kiro prompts: add create-plan, implement-plan, implement-issue, investigate-issue; remove implement-fix"
- **Last modified**: f69f8b0 - Recent fix for $ARGUMENTS pattern
- **Implication**: Recent addition to prompt system, not a regression but missing feature

---

## Implementation Plan

### Step 1: Update implement-issue.md decision tree

**File**: `.kiro/prompts/implement-issue.md`
**Lines**: 134-160
**Action**: UPDATE

**Current code:**
```markdown
┌─ IN WORKTREE?
│  └─ YES → Use it (assume it's for this work)
├─ ON MAIN/MASTER?
│  └─ Q: Working directory clean?
│     ├─ YES → Create branch: fix/issue-{number}-{slug}
│     │        git checkout -b fix/issue-{number}-{slug}
│     └─ NO  → Warn user
├─ ON FEATURE/FIX BRANCH?
│  └─ Use it (assume it's for this work)
└─ DIRTY STATE?
   └─ Warn and suggest: git stash or git commit
```

**Required change:**
```markdown
┌─ IN WORKTREE?
│  └─ YES → Use it (assume it's for this work)
├─ CHECK CURRENT BRANCH:
│  └─ current_branch=$(git branch --show-current)
├─ ON MAIN/MASTER?
│  └─ Q: Working directory clean?
│     ├─ YES → Create branch: fix/issue-{number}-{slug}
│     │        git checkout -b fix/issue-{number}-{slug}
│     └─ NO  → Warn user
├─ BRANCH CONTAINS ISSUE NUMBER?
│  └─ if [[ "$current_branch" =~ issue-{number} ]]; then
│     └─ Use existing branch (already appropriate for this work)
├─ ON FEATURE/FIX BRANCH?
│  └─ Use it (assume it's for this work)
└─ DIRTY STATE?
   └─ Warn and suggest: git stash or git commit
```

**Why**: Adds branch name pattern matching to prevent duplicate branch creation

---

### Step 2: Update implement-plan.md decision table

**File**: `.kiro/prompts/implement-plan.md`
**Lines**: 71-78
**Action**: UPDATE

**Current code:**
```markdown
| Current State | Action |
|---------------|--------|
| In worktree | Use it |
| On main, clean | Create branch: `git checkout -b feature/{plan-slug}` |
| On main, dirty | STOP |
| On feature branch | Use it |
```

**Required change:**
```markdown
| Current State | Action |
|---------------|--------|
| In worktree | Use it |
| On main, clean | Create branch: `git checkout -b feature/{plan-slug}` |
| On main, dirty | STOP |
| Branch matches plan | Use existing branch (check if current branch contains plan keywords) |
| On feature branch | Use it |
```

**Why**: Adds branch name matching for plan-based workflows

---

### Step 3: Add branch checking helper

**File**: `.kiro/prompts/implement-issue.md`
**Lines**: Before Phase 3
**Action**: UPDATE

**Add branch checking logic:**
```bash
# Check if current branch is appropriate for this issue
current_branch=$(git branch --show-current)
if [[ "$current_branch" =~ issue-{number} ]] || [[ "$current_branch" =~ fix.*{number} ]]; then
  echo "✅ Current branch '$current_branch' is appropriate for issue #{number}"
  # Use existing branch
else
  # Proceed with normal decision tree
fi
```

**Why**: Provides reusable branch validation logic

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```markdown
# SOURCE: .kiro/prompts/implement-issue.md:134-160
# Pattern for decision tree structure
┌─ CONDITION?
│  └─ ACTION
├─ NEXT CONDITION?
│  └─ Q: Sub-question?
│     ├─ YES → Action with command
│     └─ NO  → Alternative action
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Branch name doesn't match pattern but is correct | Add manual override option in prompts |
| Multiple issue numbers in branch name | Use most specific match or ask user |
| Branch name contains partial issue number | Use exact pattern matching with word boundaries |

---

## Validation

### Automated Checks

```bash
# Test branch detection logic
current_branch=$(git branch --show-current)
echo "Current branch: $current_branch"

# Test pattern matching
if [[ "$current_branch" =~ issue-29 ]]; then
  echo "✅ Branch matches issue pattern"
else
  echo "❌ Branch doesn't match"
fi
```

### Manual Verification

1. Test on branch with issue number - should not create new branch
2. Test on main branch - should create new branch
3. Test on unrelated branch - should warn or ask user

---

## Scope Boundaries

**IN SCOPE:**
- Update implement-issue.md decision tree
- Update implement-plan.md decision table
- Add branch name pattern matching logic

**OUT OF SCOPE (do not touch):**
- Other prompt files that only read branch info
- Actual git branch creation commands (only decision logic)
- Complex branch naming conventions beyond current patterns

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:13:17.286+02:00
- **Artifact**: `.archon/artifacts/issues/issue-29.md`
