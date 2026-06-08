export type Project = { name: string; path: string };
export type Agent = { name: string; description?: string; systemPrompt: string };

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

/** A post on a room's shared log — the conversation unit. */
export type Message = {
  id: string;
  from: string; // participant @handle, or "human"
  to: string[]; // resolved addressees ([] = broadcast)
  text: string;
  ts: number;
  implicit?: boolean;
  system?: boolean;
};

/** One agent instance in a room. `items` is its UiEvent transcript (the working detail). */
export type Participant = {
  name: string; // @handle
  agent?: string;
  model: string;
  items: Item[];
  running: boolean;
  modelLabel: string | null;
  stats: { tokens: number; cost: number; context_pct: number | null } | null;
};

/** A room — a set of participants + the human, with a shared message log. A single
 *  agent is just a 1-participant room. */
export type Room = {
  id: string;
  name: string;
  participants: Participant[];
  log: Message[];
  status: "running" | "stopped";
  origin: "ui" | "cli";
  /** Shared worktree branch (`kild/<name>`), when the room runs in one. */
  branch?: string;
  worktreePath?: string;
};

/** Room descriptor broadcast by the engine — the room-list source. */
export type RoomSummary = {
  id: string;
  name: string;
  worktree?: string;
  participants: { name: string; agent?: string }[];
};

/** Spec to open a room. */
export type RoomSpec = {
  name: string;
  cwd: string;
  participants: { name: string; agent?: string; model?: string }[];
  worktree?: string;
};

/** A kild worktree (`kild/<name>` branch) as listed by the engine. `name` is the
 *  branch minus the `kild/` prefix — the selectable id the engine expects. */
export type Worktree = { branch: string; path: string; name?: string };
