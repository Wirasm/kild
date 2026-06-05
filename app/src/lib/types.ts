export type Project = { name: string; path: string };
export type Agent = { name: string; systemPrompt: string };

export type Item =
  | { type: "user"; text: string }
  | { type: "assistant"; text: string }
  | { type: "tool"; id: string; name: string; args: string; status: "running" | "ok" | "error" };

export type UiEvent =
  | { kind: "model"; provider: string; id: string }
  | { kind: "text"; delta: string }
  | { kind: "tool_start"; id: string; name: string; args: string }
  | { kind: "tool_end"; id: string; name: string; ok: boolean }
  | { kind: "retry"; attempt: number; max: number }
  | { kind: "agent_end" }
  | { kind: "stats"; tokens: number; cost: number; context_pct: number | null }
  | { kind: "error"; message: string }
  | { kind: "session_end" };

export type Session = {
  id: string;
  projectName: string;
  agent: string;
  model: string;
  items: Item[];
  running: boolean;
  status: "running" | "stopped";
  modelLabel: string | null;
  stats: { tokens: number; cost: number; context_pct: number | null } | null;
  origin: "ui" | "cli";
  /** `kild/<name>` branch, when the session runs in an isolated worktree. */
  branch?: string;
  /** On-disk worktree path, when the session runs in an isolated worktree. */
  worktreePath?: string;
};

/** Session metadata broadcast by the engine — including sessions other clients
 *  (e.g. the CLI) started. */
export type SessionInfo = {
  id: string;
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
  origin: "ui" | "cli";
  worktree?: string;
  branch?: string;
  worktreePath?: string;
};

/** A kild worktree (`kild/<name>` branch) as listed by the engine. `name` is the
 *  branch minus the `kild/` prefix — the selectable id the engine expects. */
export type Worktree = { branch: string; path: string; name?: string };
