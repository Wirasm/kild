/**
 * kild pi extension — drive kild rooms from the pi CLI.
 *
 * The pi session is the OPERATOR/driver: it opens rooms (concurrent multi-agent
 * workstreams), delegates by posting, observes, and closes only on the human's explicit
 * order. This is a THIN client over the kild engine's REST API — all orchestration
 * (concurrent workers, routing, idle failsafe, observability) lives in the engine.
 *
 * Install: symlink this directory into pi's discovery path and install deps:
 *   ln -s <kild>/pi-extension ~/.pi/agent/extensions/kild
 *   cd <kild>/pi-extension && bun install
 * Env: KILD_ENGINE (default http://localhost:4517), KILD_ENGINE_DIR (enables auto-start).
 *
 * Duck-typed against pi 0.81.1 (the extension API is pre-1.0; we deliberately avoid a
 * type dependency on the churning SDK and declare the minimal surface we use).
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Type } from 'typebox';

// ── minimal structural types for the pi ExtensionAPI surface we use ──────────────────
interface ToolResult {
  content: Array<{ type: 'text'; text: string }>;
  details?: unknown;
}
interface PiToolDefinition {
  name: string;
  label: string;
  description: string;
  parameters: unknown;
  execute(toolCallId: string, params: unknown): Promise<ToolResult>;
}
interface PiExtensionAPI {
  registerTool(tool: PiToolDefinition): void;
  on(event: string, handler: (event: never, ctx: never) => unknown): void;
}

const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';
const MAX_TEXT = 48_000; // pi convention: tools self-truncate ~50KB

// ── engine client ─────────────────────────────────────────────────────────────────────
async function engineFetch<T>(p: string, init?: RequestInit): Promise<T> {
  await ensureEngine();
  const r = await fetch(`${ENGINE}${p}`, init);
  if (!r.ok) {
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `${p} failed (${r.status})`);
  }
  return r.json() as Promise<T>;
}

async function engineUp(): Promise<boolean> {
  try {
    const r = await fetch(`${ENGINE}/api/health`, { signal: AbortSignal.timeout(1500) });
    return r.ok;
  } catch {
    return false;
  }
}

/** Preflight: the engine is the product — make sure it's running. Auto-start when
 *  KILD_ENGINE_DIR names the engine dir (spawned detached, logs to /tmp/kild-engine.log);
 *  otherwise fail with the exact command so the agent can start it over bash. */
async function ensureEngine(): Promise<void> {
  if (await engineUp()) return;
  const dir = process.env.KILD_ENGINE_DIR;
  if (dir && fs.existsSync(path.join(dir, 'package.json'))) {
    const out = fs.openSync('/tmp/kild-engine.log', 'a');
    spawn('bun', ['run', 'serve'], { cwd: dir, detached: true, stdio: ['ignore', out, out] }).unref();
    for (let i = 0; i < 20; i++) {
      await new Promise((res) => setTimeout(res, 500));
      if (await engineUp()) return;
    }
  }
  throw new Error(
    `kild engine is not running at ${ENGINE}. Start it with: cd <kild>/engine && bun run serve ` +
      `(or set KILD_ENGINE_DIR to the engine directory to let this extension auto-start it).`,
  );
}

// ── engine payload shapes (the parts we render) ──────────────────────────────────────
interface GitStatus {
  branch: string | null;
  base: string;
  ahead: number;
  behind: number;
  dirty: boolean;
  changedFiles: string[];
  conflictsWithBase: boolean | null;
  error?: string;
}
interface LiveRoom {
  id: string;
  name: string;
  worktree?: string;
  state?: string;
  participants: Array<{ name: string; agent?: string; model?: string }>;
  log: Array<{ from: string; to: string[]; text: string; system?: boolean; implicit?: boolean }>;
  git?: GitStatus;
}

function gitLine(g?: GitStatus): string {
  if (!g) return '';
  const flags = `${g.dirty ? ' dirty' : ''}${g.conflictsWithBase ? ' CONFLICTS-WITH-BASE' : ''}`;
  return ` · ${g.branch ?? '?'} (base ${g.base}) +${g.ahead}/-${g.behind}${flags} · ${g.changedFiles.length} files changed`;
}

function participantLine(room: LiveRoom): string {
  return room.participants
    .map((p) => (p.model ? `${p.name}:${p.model}` : p.name))
    .join(', ');
}

function truncate(text: string): string {
  return text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}\n… (truncated)` : text;
}

// ── the driver guide injected into the pi session's system prompt ────────────────────
function modelCatalog(): string {
  const read = (f: string): Record<string, string> => {
    try {
      const cfg = JSON.parse(fs.readFileSync(f, 'utf8')) as { models?: Record<string, string> };
      return cfg.models ?? {};
    } catch {
      return {};
    }
  };
  const home = process.env.KILD_HOME ?? path.join(os.homedir(), '.config', 'kild');
  const merged = {
    ...read(path.join(home, 'config.json')),
    ...read(path.join(process.cwd(), '.kild', 'config.json')),
  };
  const lines = Object.entries(merged).map(([ref, desc]) => `- ${ref} — ${desc}`);
  return lines.length ? `\nParticipant models (pick per task — fit over cost):\n${lines.join('\n')}` : '';
}

function driverGuide(): string {
  return `<kild-fleet-driver>
You can orchestrate CONCURRENT multi-agent workstreams ("rooms") via the kild_* tools. You
are the operator/driver: rooms do the work; you open, delegate, observe, and land.

- One room = one workstream = one isolated git worktree. Name the worktree for the task
  (e.g. fix-2247). Participants run concurrently, each a real agent session.
- Open with kild_open_room (participants: name + agent persona + model; kickoff = the goal,
  delivered to the room lead). Steer or delegate more work with kild_room_post.
- Observe with kild_rooms (compact status: per-agent models, git state, collisions) and
  kild_room_log (one room's full thread). Pull when you need state — don't poll hot.
- Delegation inside rooms is asynchronous; leads report when done and rooms then sit IDLE
  with full context — follow up anytime with kild_room_post.
- NEVER call kild_room_close unless the human explicitly tells you to close — closing kills
  every agent's context irrecoverably. Finished rooms stay open for follow-up.
${modelCatalog()}
</kild-fleet-driver>`;
}

// ── extension entrypoint ──────────────────────────────────────────────────────────────
export default function (pi: PiExtensionAPI) {
  pi.registerTool({
    name: 'kild_open_room',
    label: 'kild: open room',
    description:
      'Open a kild room — a concurrent multi-agent workstream in an isolated git worktree. ' +
      'Returns the room id. The kickoff goal is delivered to the room lead (first participant).',
    parameters: Type.Object({
      name: Type.String({ description: 'Short room name, e.g. "fix-2247".' }),
      project: Type.Optional(Type.String({ description: 'Registered kild project name or absolute path. Defaults to the current directory.' })),
      participants: Type.Optional(
        Type.Array(
          Type.Object({
            name: Type.String({ description: 'Participant @handle.' }),
            agent: Type.Optional(Type.String({ description: 'Agent persona to run (default: the general-purpose "default").' })),
            model: Type.Optional(Type.String({ description: 'provider/model ref, e.g. openai-codex/gpt-5.6-sol.' })),
          }),
        ),
      ),
      worktree: Type.Optional(Type.String({ description: 'Isolated worktree name (branch kild/<name>). Strongly recommended: one per workstream.' })),
      base: Type.Optional(Type.String({ description: 'Base branch to fork from / measure against (default: config baseBranch, else current branch).' })),
      kickoff: Type.String({ description: 'The goal/steering message delivered to the room lead.' }),
    }),
    async execute(_id, params) {
      const p = params as {
        name: string;
        project?: string;
        participants?: Array<{ name: string; agent?: string; model?: string }>;
        worktree?: string;
        base?: string;
        kickoff: string;
      };
      const body = {
        name: p.name,
        ...(p.project?.startsWith('/') ? { cwd: p.project } : p.project ? { project: p.project } : { cwd: process.cwd() }),
        participants: p.participants?.length ? p.participants : [{ name: 'agent', agent: 'default' }],
        worktree: p.worktree,
        base: p.base,
        kickoff: p.kickoff,
      };
      const res = await engineFetch<{ id: string; message: string }>('/api/rooms', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      });
      return {
        content: [{ type: 'text', text: `${res.message} id=${res.id}` }],
        details: { roomId: res.id },
      };
    },
  });

  pi.registerTool({
    name: 'kild_rooms',
    label: 'kild: list rooms',
    description:
      'List live kild rooms: participants (with the model each agent runs), git/worktree state ' +
      '(branch, ahead/behind base, dirty, conflicts, changed-file count), and the last post.',
    parameters: Type.Object({}),
    async execute() {
      const rooms = await engineFetch<LiveRoom[]>('/api/rooms/live');
      if (rooms.length === 0) return { content: [{ type: 'text', text: 'no live rooms' }] };
      const lines = rooms.map((r) => {
        const last = r.log.filter((m) => !m.system).at(-1);
        const lastLine = last ? `\n    last: ${last.from} → [${last.to.join(', ')}]: ${last.text.replace(/\s+/g, ' ').slice(0, 120)}` : '';
        return `${r.id}\n    ${r.name} [${participantLine(r)}]${gitLine(r.git)}${lastLine}`;
      });
      return { content: [{ type: 'text', text: truncate(lines.join('\n')) }], details: { count: rooms.length } };
    },
  });

  pi.registerTool({
    name: 'kild_room_log',
    label: 'kild: room log',
    description: "Read one live room's full message thread (pull view). Use tail to limit.",
    parameters: Type.Object({
      id: Type.String({ description: 'Room id.' }),
      tail: Type.Optional(Type.Number({ description: 'Only the last N messages (default 30).' })),
    }),
    async execute(_id, params) {
      const p = params as { id: string; tail?: number };
      const rooms = await engineFetch<LiveRoom[]>('/api/rooms/live');
      const room = rooms.find((r) => r.id === p.id);
      if (!room) throw new Error(`no such live room: ${p.id}`);
      const tail = p.tail ?? 30;
      const msgs = room.log.slice(-tail).map((m) => {
        const tag = m.system ? ' [sys]' : m.implicit ? ' [narration]' : '';
        return `${m.from} → [${m.to.join(', ')}]${tag}: ${m.text}`;
      });
      return {
        content: [{ type: 'text', text: truncate(msgs.join('\n') || '(no messages)') }],
        details: { total: room.log.length, shown: msgs.length },
      };
    },
  });

  pi.registerTool({
    name: 'kild_room_post',
    label: 'kild: post to room',
    description:
      'Post a message into a live kild room — kick off, steer, delegate more work, or follow ' +
      'up with an idle room. Untargeted posts go to the room lead.',
    parameters: Type.Object({
      id: Type.String({ description: 'Room id.' }),
      text: Type.String({ description: 'The message.' }),
    }),
    async execute(_id, params) {
      const p = params as { id: string; text: string };
      const res = await engineFetch<{ message: string }>(`/api/rooms/${encodeURIComponent(p.id)}/post`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text: p.text }),
      });
      return { content: [{ type: 'text', text: res.message }] };
    },
  });

  pi.registerTool({
    name: 'kild_room_close',
    label: 'kild: close room',
    description:
      'Close a kild room: stops every agent and archives the transcript. DESTRUCTIVE — all ' +
      "agent context is lost forever. Call ONLY when the human explicitly says to close. " +
      'Finished rooms should stay open (idle) for follow-up.',
    parameters: Type.Object({
      id: Type.String({ description: 'Room id.' }),
    }),
    async execute(_id, params) {
      const p = params as { id: string };
      const res = await engineFetch<{ message: string }>(`/api/rooms/${encodeURIComponent(p.id)}/close`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({}),
      });
      return { content: [{ type: 'text', text: res.message }] };
    },
  });

  // The driver guide rides the system prompt every turn — chained after other extensions.
  pi.on('before_agent_start', ((event: { systemPrompt: string }) => ({
    systemPrompt: `${event.systemPrompt}\n\n${driverGuide()}`,
  })) as never);
}
