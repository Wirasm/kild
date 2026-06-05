# kild-flue-spike

A spike that rebuilds kild's current capabilities **and** the three batteries kild
doesn't have yet, on [Flue](https://github.com/withastro/flue) (the TS agent
framework built on the same `pi-agent-core` kernel kild drives). Purpose: a
grounded build-vs-borrow decision for kild's orchestration engine.

**Read [`COMPARISON.md`](./COMPARISON.md) for the findings.**

## Layout

```
src/
  kild/                 # "kild-on-Flue" library — all the logic
    config.ts           #   default model + state dir
    projects.ts         #   project registry        (rebuild of kild-core::project)
    agents.ts           #   agents from .kild/.claude/.pi  (rebuild of kild-core::agent)
    run.ts              #   one-shot run → RunOutcome (rebuild of kild-core::rpc::run_to_completion)
    worktree.ts         #   BATTERY 1 — git worktree as a Flue Sandbox
    rooms.ts            #   BATTERY 2 — agent-to-agent rooms (peer comms bus + tools)
    observability.ts    #   BATTERY 3a — observe() → cockpit event log
    brain.ts            #   BATTERY 3b — operator-mirror agent (kild capabilities as tools)
  workflows/            # runnable demos (`flue run <name>`)
    hello.ts            #   auth/toolchain smoke test
    run.ts              #   kild run rebuilt
    worktree-demo.ts    #   proves worktree-as-Sandbox
    rooms-demo.ts       #   two agents converse through a kild room
    brain-demo.ts       #   brain orchestrates via kild tools
    merge-team-demo.ts  #   N reviewers in parallel + conflict-aware merge order
```

## Setup

```bash
npm install
# minimax has a raw API key in pi's auth.json and an Anthropic-compatible endpoint,
# so it works without provider OAuth. (Flue does NOT read ~/.pi/agent/auth.json —
# see COMPARISON.md "auth regression".) The .env below is written for you by the
# project-registration step and is gitignored.
```

Register the demo project (a throwaway git repo) and the model key:

```bash
# .env  →  MINIMAX_API_KEY=...   (pulled from ~/.pi/agent/auth.json)
# .kild-spike/projects.json  →  {flue-spike, kild}
# Both are created by the setup used in the spike session; recreate with:
mkdir -p .kild-spike/sample-repo && (cd .kild-spike/sample-repo && git init -q \
  && git commit -q --allow-empty -m init)
```

## Run the demos

```bash
npx flue run hello          --target node --payload '{"text":"hi","model":"minimax/MiniMax-M3"}'
npx flue run worktree-demo  --target node --payload '{"project":"flue-spike","branch":"spike-demo"}'
npx flue run rooms-demo     --target node --payload '{}'
npx flue run brain-demo     --target node --payload '{}'
npx flue run run            --target node --payload '{"prompt":"...","project":"flue-spike","agent":"reviewer"}'
npx flue run merge-team-demo --target node --payload '{}'
npm run typecheck
```

All six ran green in the spike session.
