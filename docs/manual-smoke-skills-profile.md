# Skills-profile smoke test

This verifies that a capability profile applies to the agents in a kild.

1. Make an absolute profile directory containing exactly these seven skill directories:
   `prp-plan`, `prp-prd`, `prp-codebase-question`, `prp-commit`, `prp-debug`,
   `prp-review`, and `prp-implement`. Each directory must contain its `SKILL.md`.
2. Start the engine with the profile: `KILD_SKILLS_PROFILE=/absolute/path/to/profile bun run serve`
   (from `engine/`).
3. Open a kild (`kild new "…" --detach`) and inspect an agent's startup resource/skills list.
   It must list exactly the seven skills above, and must not list `prp-worktree`, `prp-loop`
   or `prp-pr`, even if those are globally or project-discoverable.
4. Run a one-shot agent outside a kild (`kild run "…"`): it is **not** profile-scoped, and its
   normally discovered skills remain available.

The engine rejects a relative `KILD_SKILLS_PROFILE` value at startup.

**Caveat worth knowing before you trust this.** `KILD_SKILLS_PROFILE` scopes *skills* and
nothing else. A profiled agent still inherits the operator's `~/.pi/agent` extensions,
`SYSTEM.md`/`AGENTS.md` context files and prompt templates, plus the project's own
`.pi/extensions` — pi's resource loader takes `noSkills` alone, and that is the only door
this closes. Real isolation would need `noExtensions` + `noContextFiles` +
`noPromptTemplates`, or an `agentDir` pointed at a scratch directory.
