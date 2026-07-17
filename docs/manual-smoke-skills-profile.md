# Skills-profile room smoke test

This verifies that a capability profile applies to room participants only.

1. Make an absolute profile directory containing exactly these seven skill directories:
   `prp-plan`, `prp-prd`, `prp-codebase-question`, `prp-commit`, `prp-debug`,
   `prp-review`, and `prp-implement`. Each directory must contain its `SKILL.md`.
2. Start the engine with the profile: `KILD_SKILLS_PROFILE=/absolute/path/to/profile bun run serve`
   (from `engine/`).
3. Open a room and inspect a participant's startup resource/skills list. It must list
   exactly the seven skills above, and must not list `prp-worktree`, `prp-loop`, or
   `prp-pr`, even if those are globally or project-discoverable.
4. Spawn an ordinary session and run `kild fleet ...`; neither is profile-scoped.
   Their normal discovered skills remain available.

The engine rejects a relative `KILD_SKILLS_PROFILE` value at startup.
