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
};
