# PR Merge Order Analysis

**Repository**: Wirasm/shards
**Date**: 2026-01-20
**Base Branch**: main
**Open PRs Analyzed**: 9
**Ready to Merge**: 6
**Conflict-Prone**: 3

---

## Executive Summary

**Key Findings**:
- 6 PRs ready for immediate merge with careful sequencing
- 3 PRs have potential conflicts requiring specific merge order
- 0 PRs are blocked by dependencies
- 3 PRs can be merged in parallel after conflict resolution

**Recommended Strategy**: CONFLICT_MINIMIZING with priority weighting

**Estimated Merge Time**: 2-3 hours with 3 potential conflicts requiring careful sequencing

---

## Recommended Merge Order

### Phase 1: Independent Merges (No Conflicts)
1. **PR #24** - Add detailed implementation plan for prompt piping feature
   - **Priority**: 4/10
   - **Rationale**: Documentation only, no code conflicts
   - **Files**: docs/*, .archon/artifacts/*
   - **Status**: ✅ Ready

2. **PR #34** - Fix: Add branch checking logic to .kiro prompts (#29)
   - **Priority**: 5/10
   - **Rationale**: Kiro prompts only, no code conflicts
   - **Files**: .kiro/prompts/*
   - **Status**: ✅ Ready

3. **PR #40** - Fix: Implement last_activity tracking for health monitoring (#26)
   - **Priority**: 7/10
   - **Rationale**: Health monitoring improvement, isolated to sessions/health modules
   - **Files**: src/health/*, src/sessions/*
   - **Status**: ✅ Ready

4. **PR #39** - Fix: Add branch validation to shards create command (#33)
   - **Priority**: 8/10
   - **Rationale**: Important validation feature, isolated to git module
   - **Files**: src/git/*
   - **Status**: ✅ Ready

### Phase 2: Conflict Resolution Sequence
5. **PR #35** - Fix: Remove dead code warnings in cleanup module
   - **Priority**: 6/10
   - **Rationale**: Merge before PR #41 to avoid cleanup module conflicts
   - **Files**: src/cleanup/operations.rs
   - **Status**: ✅ Ready - MERGE FIRST in cleanup sequence

6. **PR #41** - Fix: Wire cleanup strategy functions to CLI flags (#27)
   - **Priority**: 8/10
   - **Rationale**: Depends on cleanup module, merge after #35, before #38
   - **Conflicts**: Potential with PR #38 (cli/commands.rs)
   - **Status**: ⚠️ Merge after #35, before #38

7. **PR #38** - Fix: expand session name display column from 16 to 32 characters (#6)
   - **Priority**: 6/10
   - **Rationale**: UI improvement, merge after #41 to resolve cli/commands.rs conflict
   - **Conflicts**: With PR #41 (cli/commands.rs)
   - **Status**: ⚠️ Merge after #41

### Phase 3: Terminal Module Sequence
8. **PR #37** - Fix: Improve agent process detection reliability (#28)
   - **Priority**: 7/10
   - **Rationale**: Process detection improvement, merge before #36
   - **Conflicts**: With PR #36 (terminal/handler.rs)
   - **Status**: ⚠️ Merge before #36

9. **PR #36** - Fix: Implement terminal type selection and cross-platform terminal support (#7)
   - **Priority**: 8/10
   - **Rationale**: Major terminal enhancement, merge after #37
   - **Conflicts**: With PR #37 (terminal/handler.rs)
   - **Status**: ⚠️ Merge after #37

---

## Parallel Merge Opportunities

### Batch A: Documentation & Configuration (Can merge simultaneously)
- **PR #24** - Documentation changes (docs/*)
- **PR #34** - Kiro prompts (.kiro/prompts/*)

### Batch B: Independent Modules (After Phase 1)
- **PR #40** - Health/sessions modules
- **PR #39** - Git module validation

**Merge Commands for Batch A**:
```bash
# Can be merged simultaneously
gh pr merge 24 --squash --delete-branch
gh pr merge 34 --squash --delete-branch
```

---

## Conflict Analysis Matrix

| PR Pair | Conflict Risk | Files Overlap | Recommendation |
|---------|---------------|---------------|----------------|
| #41 + #38 | HIGH | cli/commands.rs | Merge #41 first, rebase #38 |
| #37 + #36 | HIGH | terminal/handler.rs | Merge #37 first, rebase #36 |
| #41 + #35 | MEDIUM | cleanup module | Merge #35 first, then #41 |
| #40 + #39 | NONE | Different modules | Can merge in parallel |
| #24 + #34 | NONE | Different areas | Can merge in parallel |

---

## Dependency Graph

```
Phase 1 (Parallel):
#24 (docs) ──┐
#34 (prompts) ┼─→ Phase 2
#40 (health) ─┤
#39 (git) ────┘

Phase 2 (Sequential):
#35 (cleanup/ops) → #41 (cleanup/cli) → #38 (cli/table)

Phase 3 (Sequential):
#37 (process) → #36 (terminal)
```

**Critical Path**: #35 → #41 → #38 (cleanup/CLI sequence)
**Estimated Time**: 1.5 hours if merged sequentially

---

## PRs Not Ready for Merge

All PRs are ready for merge with proper sequencing. No blocking issues detected.

---

## Risk Assessment

### High-Risk Merges
- **PR #41 + #38**: Both modify cli/commands.rs
  - **Mitigation**: Strict sequential order (#41 first, then rebase #38)
  - **Rollback Plan**: Individual revert commits ready

- **PR #37 + #36**: Both modify terminal/handler.rs
  - **Mitigation**: Merge #37 first, coordinate with #36 rebase
  - **Rollback Plan**: Terminal module rollback strategy

### Medium-Risk Merges  
- **PR #35 + #41**: Cleanup module changes
  - **Mitigation**: Merge #35 first to establish clean base

---

## Execution Plan

### Immediate Actions (Next 30 minutes)
- [ ] Merge Phase 1 PRs: #24, #34, #40, #39
- [ ] Notify about upcoming conflict resolution sequence
- [ ] Prepare rebase instructions for conflict-prone PRs

### Short Term (Next 2 hours)
- [ ] Execute Phase 2 sequence: #35 → #41 → #38
- [ ] Execute Phase 3 sequence: #37 → #36
- [ ] Monitor for merge conflicts during execution

### Medium Term (Today)
- [ ] Verify all merges completed successfully
- [ ] Update any affected documentation
- [ ] Monitor for any post-merge issues

---

## GitHub CLI Commands

### Phase 1: Safe Parallel Merges
```bash
# Documentation and configuration (parallel)
gh pr merge 24 --squash --delete-branch
gh pr merge 34 --squash --delete-branch

# Independent modules (parallel)
gh pr merge 40 --squash --delete-branch
gh pr merge 39 --squash --delete-branch
```

### Phase 2: Cleanup/CLI Sequence (Sequential)
```bash
# Step 1: Cleanup operations first
gh pr merge 35 --squash --delete-branch

# Step 2: Cleanup CLI integration
gh pr merge 41 --squash --delete-branch

# Step 3: CLI table improvements (may need rebase)
gh pr checkout 38
git rebase origin/main
git push --force-with-lease
gh pr merge 38 --squash --delete-branch
```

### Phase 3: Terminal Module Sequence (Sequential)
```bash
# Step 1: Process detection improvements
gh pr merge 37 --squash --delete-branch

# Step 2: Terminal enhancements (may need rebase)
gh pr checkout 36
git rebase origin/main
git push --force-with-lease
gh pr merge 36 --squash --delete-branch
```

### Conflict Resolution Commands
```bash
# If conflicts occur during rebase:
git rebase origin/main
# Resolve conflicts in editor, then:
git add .
git rebase --continue
git push --force-with-lease
```

---

## Next Analysis Date

**Recommended**: 2026-01-21

**Triggers for Re-analysis**:
- New PRs opened
- Existing PRs updated significantly  
- Merge conflicts detected during execution
- Priority changes from stakeholders

---

## Success Metrics

**Target Completion**: All 9 PRs merged within 3 hours
**Conflict Resolution**: Maximum 2 rebase operations needed
**Zero Rollbacks**: No merge reversals required
**Clean History**: Squash merges maintain clean commit history
