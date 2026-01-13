---
description: Run comprehensive code review using specialized agents in parallel
---

# Comprehensive Code Review

## First Step: Get Review Scope

Ask the user: "What would you like to review? Options: PR number (e.g., '123'), 'diff' for current unstaged changes, 'staged' for staged changes, or 'branch' for current branch vs main"

---

## Your Mission

Orchestrate a comprehensive code review by spawning specialized agents in parallel, then synthesizing their findings into a detailed, actionable report that enables informed decision-making.

---

## Phase 1: DETERMINE SCOPE

Based on user's response, determine the review scope:

| User Input | Scope Type | Command to Get Changes |
|------------|------------|------------------------|
| Number (e.g., `123`) | PR Review | `gh pr diff 123` and `gh pr view 123` |
| `diff` | Unstaged Changes | `git diff` |
| `staged` | Staged Changes | `git diff --staged` |
| `branch` | Branch vs Main | `git diff main...HEAD` |

**First, verify the scope exists:**

```bash
# For PR: Check PR exists and get context
gh pr view [number] --json title,state,body,author

# For diff/staged/branch: Check there are changes
git diff [options] --stat
```

**If no changes found, inform user and stop.**

---

## Phase 2: SPAWN REVIEW AGENTS IN PARALLEL

**CRITICAL: Spawn ALL applicable agents simultaneously using subagents.**

For each agent, provide clear instructions including:
1. The exact scope (PR number or diff command)
2. What to analyze
3. Expected output format with fix options

### Agents to Spawn

**Always spawn these 4 core agents in parallel:**

1. **code-reviewer** - General code quality and guidelines
   ```
   Review scope: [PR #{number} | current diff | staged changes | branch diff]

   Get changes with: [gh pr diff {number} | git diff | git diff --staged | git diff main...HEAD]

   Analyze the actual code changes for:
   - Project guidelines compliance
   - Bug detection (logic errors, null handling)
   - Code quality issues

   For each issue, provide:
   - Detailed explanation of WHY it's a problem
   - The IMPACT if left unfixed
   - 2-3 FIX OPTIONS with trade-offs for each

   Return findings with file:line references and confidence scores.
   ```

2. **comment-analyzer** - Documentation and comment quality
   ```
   Review scope: [PR #{number} | current diff | staged changes | branch diff]

   Get changes with: [command]

   Analyze comments and documentation in the actual changes for:
   - Factual accuracy vs code
   - Completeness and value
   - Misleading or outdated content

   For each issue, provide:
   - What's wrong and WHY it matters
   - 2-3 FIX OPTIONS (rewrite, remove, expand)

   Return findings with specific locations and suggestions.
   ```

3. **error-hunter** - Silent failures and error handling
   ```
   Review scope: [PR #{number} | current diff | staged changes | branch diff]

   Get changes with: [command]

   Hunt for error handling issues in the actual changes:
   - Silent failures and empty catch blocks
   - Inadequate error messages
   - Missing error logging
   - Overly broad exception catching

   For each issue, provide:
   - What errors could be hidden and their IMPACT
   - User experience consequences
   - 2-3 FIX OPTIONS with code examples

   Return findings with severity levels and fix suggestions.
   ```

4. **type-analyzer** - Type design and safety
   ```
   Review scope: [PR #{number} | current diff | staged changes | branch diff]

   Get changes with: [command]

   Analyze type definitions and usage in the actual changes:
   - Invariant strength and enforcement
   - Encapsulation quality
   - Type safety issues

   For each issue, provide:
   - WHY the current design is problematic
   - What bugs it could allow
   - 2-3 FIX OPTIONS with trade-offs

   Return ratings and specific improvement suggestions.
   ```

**Optionally spawn if relevant:**

5. **test-analyzer** - Test coverage (if test files are changed or new functionality added)
   ```
   Review scope: [PR #{number} | current diff | staged changes | branch diff]

   Get changes with: [command]

   Analyze test coverage for the actual changes:
   - Critical gaps in coverage
   - Test quality and maintainability
   - Missing edge cases

   For each gap, provide:
   - What could break without this test
   - Specific test cases to add
   - Priority (critical vs nice-to-have)

   Return prioritized test recommendations.
   ```

---

## Phase 3: WAIT AND COLLECT

Wait for all spawned agents to complete and collect their reports.

**Track progress:**
- [ ] code-reviewer complete
- [ ] comment-analyzer complete
- [ ] error-hunter complete
- [ ] type-analyzer complete
- [ ] test-analyzer complete (if spawned)

---

## Phase 4: SYNTHESIZE DETAILED REPORT

Combine all agent findings into a comprehensive report. This SAME report will be:
1. Saved as an artifact
2. Posted to GitHub (for PR reviews)
3. Shown to the user

**IMPORTANT**: The report must be detailed enough for informed decision-making. Include full context, reasoning, and fix options for every issue.

### Report Structure

```markdown
# Code Review Report

**Scope**: [PR #X with title | Current Diff | Staged Changes | Branch Diff]
**Date**: [YYYY-MM-DD HH:MM]
**Reviewers**: code-reviewer, comment-analyzer, error-hunter, type-analyzer[, test-analyzer]

---

## Executive Summary

**Overall Assessment**: [APPROVED / NEEDS CHANGES / BLOCKED]
**Risk Level**: [LOW / MEDIUM / HIGH / CRITICAL]

| Metric | Count |
|--------|-------|
| Critical Issues | X |
| Important Issues | X |
| Suggestions | X |

**Recommendation**: [Detailed recommendation explaining the assessment]

---

## Critical Issues (Must Fix Before Merge)

### Issue 1: [Descriptive Title]

**Location**: `file/path.ts:42-50`
**Source**: [agent-name]
**Confidence**: X%

**Problem**:
[Detailed explanation of what's wrong]

**Why This Matters**:
[Impact on users, system stability, security, maintainability]

**Risk If Unfixed**:
[Specific consequences - bugs, security issues, tech debt]

**Fix Options**:

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A (Recommended) | [Description] | [Benefits] | [Drawbacks] |
| B | [Description] | [Benefits] | [Drawbacks] |
| C | [Description] | [Benefits] | [Drawbacks] |

**Recommended Fix**:
```[language]
// Before
[current code]

// After (Option A)
[fixed code]
```

---

### Issue 2: [Descriptive Title]
[Same structure as above]

---

## Important Issues (Should Fix)

### Issue 1: [Descriptive Title]

**Location**: `file/path.ts:100`
**Source**: [agent-name]

**Problem**:
[Explanation]

**Impact**:
[Why this matters, even if not critical]

**Fix Options**:

| Option | Approach | Trade-off |
|--------|----------|-----------|
| A | [Description] | [Trade-off] |
| B | [Description] | [Trade-off] |

**Suggested Fix**:
[Code example or description]

---

## Suggestions (Nice to Have)

### Suggestion 1: [Title]

**Location**: `file/path.ts:200`
**Source**: [agent-name]

**Current State**: [What exists now]
**Improvement**: [What could be better]
**Benefit**: [Why bother]

---

## Detailed Agent Reports

### Code Quality Analysis (code-reviewer)

**Files Reviewed**: [list]

**Findings Summary**:
[Detailed summary of code quality observations]

**Patterns Observed**:
- [Good patterns found]
- [Anti-patterns found]

---

### Documentation Analysis (comment-analyzer)

**Comments Reviewed**: [count and types]

**Findings Summary**:
[Detailed summary of documentation quality]

**Comment Quality Score**: X/10

---

### Error Handling Analysis (error-hunter)

**Error Handlers Reviewed**: [count]

**Findings Summary**:
[Detailed summary of error handling quality]

**Silent Failure Risk**: [LOW/MEDIUM/HIGH]

---

### Type Design Analysis (type-analyzer)

**Types Reviewed**: [list]

**Findings Summary**:
[Detailed summary of type safety]

**Overall Type Safety Score**: X/10

---

### Test Coverage Analysis (test-analyzer)
*(If applicable)*

**Test Files Reviewed**: [list]

**Coverage Assessment**:
[Detailed summary of test coverage]

**Critical Gaps**: [list]

---

## What's Done Well

- [Specific positive observation with location]
- [Another positive observation]
- [Good patterns that should be continued]

---

## Action Items (Prioritized)

### Must Do (Blocking)
1. [ ] [Specific action with file:line] - [brief reason]
2. [ ] [Specific action with file:line] - [brief reason]

### Should Do (Before Merge)
1. [ ] [Specific action with file:line] - [brief reason]
2. [ ] [Specific action with file:line] - [brief reason]

### Consider (Optional)
1. [ ] [Specific action with file:line] - [brief reason]

---

## Decision Guide

**If you have limited time**, focus on:
1. [Most critical item]
2. [Second most critical]

**If you want thorough improvement**, also address:
1. [Important items]

**Quick wins** (easy fixes with good impact):
1. [Easy fix with high value]

---

*Review generated by Kiro AI agents*
```

---

## Phase 5: SAVE AND POST

### 5.1 Save Report

```bash
mkdir -p .kiro/artifacts/code-review-reports
```

Save the FULL report to: `.kiro/artifacts/code-review-reports/review-[scope]-[date].md`

### 5.2 Post to GitHub (PR reviews only)

**CRITICAL: Post the SAME full report to GitHub, not a summary.**

For PR reviews, post the complete report as a PR comment:

```bash
gh pr comment [number] --body "$(cat .kiro/artifacts/code-review-reports/review-PR-[number]-[date].md)"
```

This ensures the GitHub comment is identical to the saved artifact.

---

## Phase 6: OUTPUT TO USER

Show the user the same complete report (not a summary).

Then add:

```markdown
---

## Files

**Report saved**: `.kiro/artifacts/code-review-reports/review-[scope]-[date].md`
**GitHub**: [Posted full report to PR #X / N/A - not a PR review]

## Quick Reference

| Priority | Count | Action |
|----------|-------|--------|
| Critical | X | Must fix before merge |
| Important | X | Should fix |
| Suggestions | X | Optional |

**Top 3 Actions**:
1. [Most important with location]
2. [Second with location]
3. [Third with location]
```

---

## Tips

- **Run early**: Review before creating PR, not after
- **Focus on critical**: Fix blocking issues first
- **Use fix options**: Each issue has multiple approaches - pick what fits your situation
- **Re-run after fixes**: Verify issues are resolved
- **Use for self-review**: Great for checking your own code before committing
