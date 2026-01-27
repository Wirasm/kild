# KILD CLI E2E Testing Guide

Run this to verify the CLI works correctly. Can be run from any location: main branch, feature branch, or inside a worktree.

## Pre-flight: Locate Yourself

Before running tests, determine your current context:

```bash
# Where am I?
pwd

# What branch am I on?
git branch --show-current

# Am I in a worktree?
git rev-parse --show-toplevel
```

**Context determines the test target:**

| Location | What you're testing |
|----------|---------------------|
| Main branch in main repo | The merged/released code |
| Feature branch in main repo | Your changes before merging |
| Inside a worktree (`~/.kild/worktrees/...`) | Changes in that isolated workspace |

## Build from Current Location

Build the release binary from wherever you are:

```bash
cargo build --release --bin kild
```

This builds from your current branch/worktree code. The binary will be at `./target/release/kild`.

**Verify the build is from your code:**
```bash
git log -1 --oneline  # Note the commit
./target/release/kild --version  # Should match
```

## Test Sequence

Execute these tests in order. Use `./target/release/kild` for all commands.

### Phase 1: Clean State Check
```bash
./target/release/kild list
```
**Expected**: Shows existing kilds or "No active kilds found." Note any existing kilds - they should not be affected.

### Phase 2: Create Test Kild
```bash
./target/release/kild create e2e-test-kild --agent claude
```
**Expected**:
- Success message
- Branch: `e2e-test-kild`
- Worktree path shown
- Port range allocated
- Terminal window opens with Claude

**If it fails**:
- Branch exists? `git branch -a | grep e2e-test`
- In a git repo? `git status`
- Disk space? `df -h`

### Phase 3: List Shows the Kild
```bash
./target/release/kild list
```
**Expected**: Table shows `e2e-test-kild` with status active, process running.

### Phase 4: Status Details
```bash
./target/release/kild status e2e-test-kild
```
**Expected**: Detailed info box with process running, PID shown.

### Phase 5: Health (All)
```bash
./target/release/kild health
```
**Expected**: Dashboard table with Working status, CPU/memory metrics.

### Phase 6: Health (Single)
```bash
./target/release/kild health e2e-test-kild
```
**Expected**: Detailed health for just this kild.

### Phase 7: Cleanup --orphans
```bash
./target/release/kild cleanup --orphans
```
**Expected**: "No orphaned resources found" (kild has valid session).

### Phase 8: Restart
```bash
./target/release/kild restart e2e-test-kild
```
**Expected**: Success, agent restarted.

### Phase 9: Destroy
```bash
./target/release/kild destroy e2e-test-kild
```
**Expected**: Success, terminal closes, worktree removed.

### Phase 10: Verify Clean
```bash
./target/release/kild list
```
**Expected**: `e2e-test-kild` gone, only pre-existing kilds remain.

## Edge Cases

Test error handling after the main sequence:

### Destroy Non-existent
```bash
./target/release/kild destroy fake-kild-xyz
```
**Expected**: Error "not found"

### Status Non-existent
```bash
./target/release/kild status fake-kild-xyz
```
**Expected**: Error "not found"

### Cleanup Empty
```bash
./target/release/kild cleanup --stopped
```
**Expected**: "No orphaned resources found"

### Health JSON
```bash
./target/release/kild health --json
```
**Expected**: Valid JSON output

## Test Report

Summarize results:

| Test | Status | Notes |
|------|--------|-------|
| Location | | (branch/worktree name) |
| Build | | |
| Create | | |
| List | | |
| Status | | |
| Health (all) | | |
| Health (single) | | |
| Cleanup --orphans | | |
| Restart | | |
| Destroy | | |
| Clean state | | |
| Edge cases | | |

**All tests must pass.**

## Special Considerations

### Testing from a Worktree

If you're inside a kild worktree (e.g., `~/.kild/worktrees/kild/feature-x/`):
- You're testing the code from that worktree's branch
- The test kild will be created as a nested worktree (this is fine)
- Make sure to destroy test kilds before destroying the parent worktree

### Testing from a Feature Branch

If you're on a feature branch in the main repo:
- You're testing your uncommitted/committed changes
- Good for verifying changes before creating a PR
- The binary reflects your branch's code, not main

### Comparing Against Main

To compare behavior between your changes and main:
```bash
# Build your branch
cargo build --release --bin kild
cp ./target/release/kild /tmp/kild-feature

# Switch to main and build
git checkout main
cargo build --release --bin kild
cp ./target/release/kild /tmp/kild-main

# Now you can compare
/tmp/kild-main list
/tmp/kild-feature list
```

## Troubleshooting

**Terminal doesn't open**: Try `--terminal iterm` or `--terminal terminal`

**Process not tracked**: Check `~/.kild/pids/`

**Worktree exists**: Run `git worktree list` and `git worktree prune`

**Port conflict**: Run `kild list` to check existing kilds

**JSON log noise**: Normal - look for human-readable success messages and tables
