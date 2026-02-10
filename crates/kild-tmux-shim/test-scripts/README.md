# tmux Shim Test Scripts

Scripts for testing the KILD tmux shim at different levels.

## Test Levels

| Level | Script | Automated? | What it tests |
|-------|--------|------------|---------------|
| 3 | `run-simulation.sh` | Yes | Replay Claude Code's exact 14-step spawn sequence against real daemon |
| 4 | `run-interactive-e2e.sh` | Semi-auto | Sets up daemon + kild, you run `claude` interactively inside |

## Level 3: Simulation (automated)

```bash
./crates/kild-tmux-shim/test-scripts/run-simulation.sh
```

Starts daemon, creates a test kild, runs the spawn simulation, checks results, cleans up.
Exit 0 = all passed. Exit 1 = failures.

## Level 4: Interactive E2E (manual)

```bash
./crates/kild-tmux-shim/test-scripts/run-interactive-e2e.sh
```

Sets up daemon + kild, then tells you what to do. You run `claude` inside the daemon
PTY and ask it to create a team. The script monitors shim logs in the background.

## Results

Test results and shim logs are saved to `/tmp/kild-shim-test/` for post-mortem analysis.

## Key Finding

`claude -p` (non-interactive/SDK mode) forces in-process teammates regardless of `$TMUX`
or `--teammate-mode tmux`. The tmux shim is only exercised when Claude Code runs
**interactively** inside a daemon PTY (which is the real production path).
