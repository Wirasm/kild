---
description: Systematically triage GitHub issues by analyzing PRs, dependencies, labels, and impact prioritization
---

## First Step: Repository Detection

Detect the current GitHub repository using GitHub CLI:
- Use `gh repo view` to get repository information
- Extract repository name and owner from current directory
- Confirm we're in a valid Git repository with GitHub remote

---

<objective>
Perform comprehensive GitHub issue triage through systematic analysis of open issues, recent commits, recent PRs, and impact assessment. Generate actionable recommendations for issue closure, labeling, and prioritization.

**Core Principle**: Signal over noise - be critical about what truly matters for project success.

**Execution Approach**: Be flexible and adaptive - use your judgment to determine what data to gather and how deeply to analyze.

**Output**: Structured triage report with specific actions for each issue.
</objective>

<context>
Repository: [Auto-detected from current directory]
Current date: [Current timestamp for recency analysis]
</context>

<process>

## Phase 1: DISCOVER - Gather Repository Data

### 1.1 Detect Base Branch and Recent Activity

**Determine base branch** (usually main/master):
```bash
git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@'
```

**Analyze recent commits on base branch** (last 2-4 weeks):
```bash
git log --oneline --since="2 weeks ago" origin/{base-branch}
```
Look for commit messages that reference issues (#123, "fixes #456", "closes #789")

### 1.2 Collect Available Labels

Use GitHub CLI to get all repository labels:
- Fetch all labels with descriptions and colors
- Categorize labels by type (priority, status, area, type)
- Document label taxonomy for consistent application

### 1.3 List Open Issues

Use GitHub CLI to get comprehensive issue data:
- All open issues with metadata (title, body, labels, assignees, created date)
- Issue comments and recent activity
- Cross-reference mentions and linked PRs
- Extract issue dependencies from descriptions/comments

### 1.4 Analyze Recent PRs (Merged and Open)

Use GitHub CLI to examine recent pull requests:
- **Merged PRs**: Last 30 days with merge status and linked issues
- **Open PRs**: Current active development
- Extract PR titles, descriptions, and linked issues
- Identify issue references (#123, "fixes #456", "closes #789")
- Check PR commit messages for additional issue references

**DISCOVERY_CHECKPOINT:**
- [ ] Base branch identified and recent commits analyzed
- [ ] Repository labels cataloged with categories
- [ ] All open issues collected with full metadata
- [ ] Recent PRs analyzed with issue references
- [ ] Issue dependency graph identified

---

## Phase 2: ANALYZE - Issue Assessment

### 2.1 Issue-Commit-PR Correlation Analysis

For each open issue, determine resolution status by checking:

**Recent Commits**: Search commit messages for issue references
**Merged PRs**: Check if any merged PRs reference the issue
**Code Changes**: Look for file changes that might address the issue
**Current State**: Verify if the problem described still exists

**Resolution Categories**:
- **RESOLVED**: Issue already fixed by merged PR or commit
- **IN_PROGRESS**: Issue being addressed by open PR
- **PARTIALLY_RESOLVED**: Some aspects fixed, others remain
- **DUPLICATE**: Multiple issues describing same problem
- **SUPERSEDED**: Newer issue/approach makes this obsolete
- **STALE**: No activity for 60+ days, unclear relevance
- **STILL_VALID**: Issue remains relevant and unaddressed

**Use your judgment**: If you suspect an issue might be resolved, investigate the codebase or recent changes to verify.

### 2.2 Dependency Mapping (Flexible)

Create dependency relationships where they exist:
- **BLOCKS**: Issues that prevent other work
- **BLOCKED_BY**: Issues waiting on dependencies  
- **RELATED**: Issues in same feature area
- **CONFLICTS**: Issues with contradictory requirements

*Note: Don't force dependencies where none exist naturally.*

### 2.3 Impact Assessment

Evaluate each issue on multiple dimensions:

**IMPACT_FACTORS:**
- **User Experience**: Does this affect end users directly?
- **Developer Experience**: Does this block/slow development?
- **Security**: Does this create vulnerabilities?
- **Performance**: Does this affect system performance?
- **Reliability**: Does this cause crashes/errors?
- **Technical Debt**: Does this accumulate maintenance burden?

**IMPACT_SCORING** (1-10 scale):
- **10**: Critical system failure, security vulnerability
- **8-9**: Major user-facing issues, significant dev blockers
- **6-7**: Important improvements, moderate pain points
- **4-5**: Nice-to-have features, minor issues
- **1-3**: Cosmetic changes, edge cases

**ANALYSIS_CHECKPOINT:**
- [ ] Issue resolution status determined through commit/PR analysis
- [ ] Dependency relationships mapped (where they exist)
- [ ] Impact scores calculated for all issues
- [ ] Stale/duplicate/resolved issues flagged

---

## Phase 3: CORRELATE - Cross-Reference Analysis

### 3.1 Resolution Verification

**Be thorough in checking if issues are resolved**:
- Check recent commits for fixes that might not be explicitly linked
- Look at file changes in areas mentioned by the issue
- Verify if the described problem still exists in current codebase
- Cross-reference with merged PRs and their descriptions

### 3.2 Label Gap Analysis

Compare current labels vs. recommended labels:
- **MISSING_PRIORITY**: Issues without priority labels
- **MISSING_AREA**: Issues without component/area labels
- **MISSING_TYPE**: Issues without type classification
- **INCORRECT_LABELS**: Labels that don't match issue content

### 3.3 Dependency Chain Analysis (If Applicable)

Identify critical paths where dependencies exist:
- **ROOT_BLOCKERS**: Issues blocking multiple others
- **LEAF_DEPENDENCIES**: Issues with no dependents
- **CIRCULAR_DEPENDENCIES**: Issues blocking each other
- **ORPHANED_ISSUES**: Issues with broken dependency links

**CORRELATION_CHECKPOINT:**
- [ ] Resolution status verified through multiple sources
- [ ] Label gaps identified with recommendations
- [ ] Critical dependency paths mapped (if any)
- [ ] Circular dependencies detected (if any)

---

## Phase 4: PRIORITIZE - Impact-Based Ranking

### 4.1 Priority Matrix

Create 2x2 matrix based on:
- **X-Axis**: Implementation Effort (Low/High)
- **Y-Axis**: Impact Score (Low/High)

**PRIORITY_CATEGORIES:**
- **P0 - CRITICAL**: High Impact + Any Effort (security, crashes)
- **P1 - HIGH**: High Impact + Low Effort (quick wins)
- **P2 - MEDIUM**: High Impact + High Effort OR Low Impact + Low Effort
- **P3 - LOW**: Low Impact + High Effort (defer/close)

### 4.2 Dependency-Adjusted Ranking

Adjust priorities based on dependencies:
- **BOOST**: Issues that unblock multiple others
- **DEFER**: Issues blocked by high-priority dependencies
- **BATCH**: Related issues that should be tackled together

### 4.3 Signal vs Noise Filter

Apply critical assessment:
- **SIGNAL**: Issues that meaningfully advance project goals
- **NOISE**: Issues that distract from core objectives
- **QUESTIONABLE**: Issues needing stakeholder input

**PHASE_4_CHECKPOINT:**
- [ ] All issues assigned priority levels (P0-P3)
- [ ] Dependency adjustments applied
- [ ] Signal vs noise assessment completed
- [ ] Final ranking established

---

## Phase 5: RECOMMEND - Generate Action Plan

### 5.1 Immediate Actions

**CLOSE_CANDIDATES** (issues to close immediately):
- Issues resolved by merged PRs
- Duplicate issues (keep the better one)
- Stale issues with no clear value
- Issues superseded by newer approaches

**LABEL_UPDATES** (apply missing/correct labels):
- Add priority labels based on impact assessment
- Add area/component labels for organization
- Add type labels (bug, feature, enhancement, etc.)
- Remove incorrect or outdated labels

### 5.2 Priority Recommendations

**P0 - Address Immediately:**
- Critical bugs affecting users
- Security vulnerabilities
- System reliability issues

**P1 - Next Sprint:**
- High-impact, low-effort improvements
- Issues blocking other development
- User experience pain points

**P2 - Backlog:**
- Important but complex features
- Technical debt with clear ROI
- Performance optimizations

**P3 - Consider Closing:**
- Low-impact, high-effort requests
- Edge cases affecting few users
- Features misaligned with project goals

### 5.3 Dependency Action Plan

**UNBLOCK_SEQUENCE:**
- Order for tackling dependency chains
- Issues to batch together
- Dependencies to break/simplify

**PHASE_5_CHECKPOINT:**
- [ ] Close candidates identified with rationale
- [ ] Label updates specified for each issue
- [ ] Priority-based action plan created
- [ ] Dependency resolution sequence defined

---

## Phase 6: GENERATE - Triage Report

### 6.1 Create Report Directory

```bash
mkdir -p .github/triage-reports
```

### 6.2 Generate Comprehensive Report

**Path**: `.github/triage-reports/triage-{YYYY-MM-DD}.md`

```markdown
# GitHub Issue Triage Report

**Repository**: {repo-name}
**Date**: {YYYY-MM-DD}
**Total Issues Analyzed**: {N}
**Issues Recommended for Closure**: {M}

---

## Executive Summary

**Key Findings:**
- {N} issues can be closed (already resolved/duplicate/stale)
- {M} issues missing critical labels
- {K} high-impact issues need immediate attention
- {L} dependency chains identified

**Immediate Actions Required:**
- Close {N} resolved/duplicate issues
- Apply priority labels to {M} unlabeled issues
- Address {K} P0 critical issues

---

## Issues Recommended for Closure

| Issue | Reason | Evidence |
|-------|--------|----------|
| #{number} - {title} | Already resolved | Closed by PR #{pr-number} |
| #{number} - {title} | Duplicate | Same as #{other-issue} |
| #{number} - {title} | Stale | No activity for {days} days, unclear value |

**GitHub CLI Commands:**
```bash
# Close resolved issues
gh issue close {issue-number} --comment "Resolved by PR #{pr-number}"

# Close duplicates
gh issue close {issue-number} --comment "Duplicate of #{main-issue}"

# Close stale issues
gh issue close {issue-number} --comment "Closing due to inactivity and unclear current relevance"
```

---

## Priority Rankings

### P0 - Critical (Address Immediately)

| Issue | Impact Score | Reason | Dependencies |
|-------|--------------|--------|--------------|
| #{number} - {title} | {score}/10 | {impact-reason} | {blocking-issues} |

### P1 - High Priority (Next Sprint)

| Issue | Impact Score | Effort | Quick Win |
|-------|--------------|--------|-----------|
| #{number} - {title} | {score}/10 | {Low/High} | {Yes/No} |

### P2 - Medium Priority (Backlog)

| Issue | Impact Score | Effort | Notes |
|-------|--------------|--------|-------|
| #{number} - {title} | {score}/10 | {Low/High} | {context} |

### P3 - Low Priority (Consider Closing)

| Issue | Impact Score | Reason for Low Priority |
|-------|--------------|-------------------------|
| #{number} - {title} | {score}/10 | {why-low-priority} |

---

## Label Recommendations

### Missing Priority Labels

```bash
# Apply priority labels
gh issue edit {issue-number} --add-label "priority/P0"
gh issue edit {issue-number} --add-label "priority/P1"
gh issue edit {issue-number} --add-label "priority/P2"
```

### Missing Area Labels

```bash
# Apply area labels
gh issue edit {issue-number} --add-label "area/frontend"
gh issue edit {issue-number} --add-label "area/backend"
gh issue edit {issue-number} --add-label "area/infrastructure"
```

### Missing Type Labels

```bash
# Apply type labels
gh issue edit {issue-number} --add-label "type/bug"
gh issue edit {issue-number} --add-label "type/feature"
gh issue edit {issue-number} --add-label "type/enhancement"
```

---

## Dependency Analysis

### Critical Dependency Chains

**Chain 1: {Feature Area}**
```
#{root-issue} → #{dependent-1} → #{dependent-2}
```
**Recommendation**: Address #{root-issue} first to unblock chain

**Chain 2: {Feature Area}**
```
#{blocker} ← #{blocked-issue-1}
           ← #{blocked-issue-2}
```
**Recommendation**: Prioritize #{blocker} to enable parallel work

### Circular Dependencies

| Issue A | Issue B | Resolution |
|---------|---------|------------|
| #{issue-1} | #{issue-2} | {how-to-break-cycle} |

---

## Issues Already in Progress

| Issue | PR | Status | ETA |
|-------|----|---------|----|
| #{number} - {title} | #{pr-number} | {status} | {estimate} |

---

## Signal vs Noise Assessment

### High Signal (Aligned with Project Goals)

- #{issue}: {why-important}
- #{issue}: {why-important}

### Low Signal (Consider Closing)

- #{issue}: {why-not-important}
- #{issue}: {why-not-important}

### Needs Stakeholder Input

- #{issue}: {what-needs-clarification}

---

## Recommended Actions

### Immediate (This Week)

- [ ] Close {N} resolved/duplicate/stale issues
- [ ] Apply priority labels to all unlabeled issues
- [ ] Address P0 critical issues: #{list}

### Short Term (Next Sprint)

- [ ] Tackle P1 high-priority issues in dependency order
- [ ] Review and update issue templates
- [ ] Set up automated stale issue detection

### Long Term (Next Month)

- [ ] Establish regular triage cadence
- [ ] Create issue lifecycle documentation
- [ ] Implement dependency tracking system

---

## Repository Health Metrics

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Issues with priority labels | {N}% | 90% | {status} |
| Issues with area labels | {N}% | 85% | {status} |
| Stale issues (60+ days) | {N} | <10 | {status} |
| Average issue age | {N} days | <30 days | {status} |

---

## Next Triage Date

**Recommended**: {date-in-2-weeks}

**Preparation**:
- Run this triage prompt again
- Review progress on current recommendations
- Update priority assessments based on new developments
```

**PHASE_6_CHECKPOINT:**
- [ ] Comprehensive report generated
- [ ] Specific GitHub CLI commands provided
- [ ] Action items prioritized and dated
- [ ] Next triage scheduled

---

</process>

<output>
**OUTPUT_FILE**: `.github/triage-reports/triage-{YYYY-MM-DD}.md`

**REPORT_TO_USER** (display after creating report):

```markdown
## Issue Triage Complete

**Repository**: {repo-name}
**Issues Analyzed**: {total-count}

**Key Findings**:
- 🔴 **{N} Critical (P0)**: Immediate attention required
- 🟡 **{M} High Priority (P1)**: Next sprint candidates  
- 🟢 **{K} Can Close**: Already resolved/duplicate/stale
- 🏷️ **{L} Need Labels**: Missing priority/area/type labels

**Immediate Actions**:
1. Close {K} resolved issues using provided CLI commands
2. Apply priority labels to {L} unlabeled issues
3. Address {N} P0 critical issues first

**Report Location**: `.github/triage-reports/triage-{YYYY-MM-DD}.md`

**Next Steps**:
- Review the detailed report
- Execute the provided GitHub CLI commands
- Schedule next triage for {date-in-2-weeks}
```
</output>

<verification>
**FINAL_VALIDATION before completing triage:**

**DATA_COMPLETENESS:**
- [ ] All open issues analyzed with impact scores
- [ ] Recent commits and PRs thoroughly checked for issue resolutions
- [ ] Dependencies mapped and validated (where they exist)
- [ ] Labels assessed against repository taxonomy

**PRIORITIZATION_ACCURACY:**
- [ ] P0 issues are genuinely critical (security, crashes, blockers)
- [ ] P1 issues have clear high impact and reasonable effort
- [ ] P3 issues have justified low priority reasoning
- [ ] Signal vs noise filter applied critically

**RESOLUTION_VERIFICATION:**
- [ ] Suspected resolved issues verified through code/commit analysis
- [ ] No false positives in "already resolved" category
- [ ] Duplicate detection is accurate
- [ ] Recent development activity properly considered

**ACTIONABILITY:**
- [ ] Close recommendations have clear evidence
- [ ] Label recommendations are specific and correct
- [ ] GitHub CLI commands are ready to execute
- [ ] Dependency resolution order is logical (if dependencies exist)

**QUALITY_ASSURANCE:**
- [ ] Impact assessments consider user and business value
- [ ] Recommendations are realistic and achievable
- [ ] Process adapted to repository's specific context
</verification>

<success_criteria>
**COMPREHENSIVE_ANALYSIS**: All open issues evaluated against commits, PRs, dependencies, and impact
**CRITICAL_ASSESSMENT**: Signal vs noise filter applied - only important issues prioritized
**ACTIONABLE_OUTPUT**: Specific GitHub CLI commands ready for execution
**RESOLUTION_AWARE**: Recent development activity properly considered to identify resolved issues
**IMPACT_DRIVEN**: Priority based on genuine business and user value, not just activity
**ADAPTIVE_PROCESS**: Flexible approach that adapts to repository context and recent activity
</success_criteria>
