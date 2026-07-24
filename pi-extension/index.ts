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
  /** Inject a user-role message; `followUp` queues it as the next turn (immediate when idle). */
  sendUserMessage(content: string, options?: { deliverAs?: 'steer' | 'followUp' }): void;
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
interface RoomDecision {
  key: string;
  summary: string;
  openedBy: string;
  resolvedAt?: number;
}
interface LiveRoom {
  id: string;
  name: string;
  worktree?: string;
  state?: string;
  participants: Array<{
    name: string;
    agent?: string;
    model?: string;
    piSessionId?: string;
    piSessionFile?: string;
  }>;
  log: Array<{
    id?: string;
    from: string;
    to: string[];
    text: string;
    system?: boolean;
    implicit?: boolean;
  }>;
  git?: GitStatus;
  decisions?: RoomDecision[];
}

function openDecisionsLine(room: LiveRoom): string {
  const open = (room.decisions ?? []).filter((d) => d.resolvedAt === undefined);
  if (open.length === 0) return '';
  const rendered = open.map((d) => `${d.key} (${d.summary}, raised by @${d.openedBy})`).join('; ');
  return `\n    OPEN DECISIONS: ${rendered}`;
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

/** Terminal-resume handles for a room's agents — any agent session can be reopened in a
 *  normal pi CLI with `pi --session <file>` (full context, works from any cwd). */
function resumeLines(room: LiveRoom): string {
  const withHandles = room.participants.filter((p) => p.piSessionFile ?? p.piSessionId);
  if (withHandles.length === 0) return '';
  return withHandles
    .map((p) => `\n    resume @${p.name}: pi --session ${p.piSessionFile ?? p.piSessionId}`)
    .join('');
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

/** The operator's cross-project memory ($KILD_HOME/MAIN_MEMORY.md), capped — the pi
 *  driver is a fleet operator, so it gets the same fleet memory an engine-spawned driver
 *  gets. Empty string when absent. */
function fleetMemory(): string {
  const home = process.env.KILD_HOME ?? path.join(os.homedir(), '.config', 'kild');
  try {
    const content = fs.readFileSync(path.join(home, 'MAIN_MEMORY.md'), 'utf8').trim();
    if (!content) return '';
    const capped = content.length > 6000 ? `${content.slice(0, 6000)}…` : content;
    return `\n\n<fleet-memory>\nYour cross-project fleet memory ($KILD_HOME/MAIN_MEMORY.md):\n${capped}\n</fleet-memory>`;
  } catch {
    return '';
  }
}

function driverGuide(): string {
  return `<kild-fleet-driver>
You can orchestrate CONCURRENT multi-agent workstreams ("rooms") via the kild_* tools. You
are the operator/driver: rooms do the work; you open, delegate, observe, and land.

- One room = one workstream = one isolated git worktree. Name the worktree for the task
  (e.g. fix-2247). Participants run concurrently, each a real agent session.
- Open with kild_open_room (participants: name + agent persona + model; kickoff = the goal,
  delivered to the room lead). Steer or delegate more work with kild_room_post.
- Delegation is asynchronous and you are NOT blocked. When a room reports to @human, kild
  pushes that report to you automatically as a new message — you do not poll for completion.
  React when it arrives; otherwise keep working. Use kild_rooms / kild_room_log to pull
  status on demand (per-agent models, git state, collisions, full thread).
- Rooms you open or post to sit IDLE with full context after finishing — follow up anytime
  with kild_room_post.
- NEVER call kild_room_close unless the human explicitly tells you to close — closing kills
  every agent's context irrecoverably. Finished rooms stay open for follow-up.
- Rooms carry keyed decisions: a participant's \`needs-decision[<key>]: <question>\` post
  opens one; it stays visible in kild_rooms and BLOCKS close until someone posts
  \`resolved[<key>]: <how>\`. When you make the call, post the resolved line into the room.
  Never force-close past open decisions without the human's explicit say-so.
- For a long campaign you won't drive yourself, hand off to a detached driver with
  kild_fleet_start (steer with kild_fleet_post, list with kild_sessions); its rooms outlive it.
- kild_agents lists the personas valid for participants; "default" always works.
${modelCatalog()}
</kild-fleet-driver>${fleetMemory()}`;
}

// ── completion push: engine WS → injected turns ─────────────────────────────────────
// The pi driver is an EXTERNAL client — kild's in-engine wake machinery (post routing,
// idle nudges) never reaches it. This bridge subscribes to the engine's WS and injects a
// room's report-to-human as a follow-up user message, so the driver is woken instead of
// having to poll. Scoped to rooms this session engaged (opened or posted to) so an
// unrelated fleet's traffic never spams the session. Started lazily on first engagement.
//
// Robustness: pi's extension handle goes STALE on session replacement/reload (the factory
// re-runs with a fresh `pi`). So we (1) keep a mutable ref to the latest `pi`, refreshed
// each factory run, (2) queue reports and drain them whenever a live handle is available,
// and (3) NEVER let a delivery error escape — a stale/failed push must not crash the host.
const engagedRooms = new Set<string>();
const roomNames = new Map<string, string>();
const seenReports = new Set<string>();
const pendingReports: string[] = [];
let currentPi: PiExtensionAPI | undefined;
let watching = false;

// Optional diagnostics: set KILD_EXT_DEBUG=1 to trace the bridge to a log file (default
// /tmp/kild-ext.log). The bridge runs inside the pi process with no visible output, so this
// is the only window into why a completion push did or didn't fire.
const DEBUG = process.env.KILD_EXT_DEBUG === '1';
const DEBUG_LOG = process.env.KILD_EXT_LOG ?? '/tmp/kild-ext.log';
function dbg(msg: string): void {
  if (!DEBUG) return;
  try {
    fs.appendFileSync(DEBUG_LOG, `${new Date().toISOString()} ${msg}\n`);
  } catch {
    /* diagnostics must never affect behavior */
  }
}

/** Deliver queued reports via the latest live handle; requeue (stop) on the stale boundary. */
function drainReports(): void {
  if (pendingReports.length && !currentPi) dbg(`drain: ${pendingReports.length} queued but no live handle`);
  while (pendingReports.length > 0 && currentPi) {
    const text = pendingReports[0];
    try {
      currentPi.sendUserMessage(text, { deliverAs: 'followUp' });
      dbg(`drain: delivered (${text.slice(0, 40).replace(/\n/g, ' ')}…)`);
    } catch (e) {
      dbg(`drain: send threw (stale?) — requeued; ${(e as Error).message}`);
      return; // handle went stale — leave it queued; the next factory run drains it
    }
    pendingReports.shift();
  }
}

function pushReport(text: string): void {
  pendingReports.push(text);
  drainReports();
}

let ws: WebSocket | undefined;
let lastBootId: string | undefined;
let everConnected = false;
const notifiedGone = new Set<string>();

/** Catch up after a WS gap: events pushed while the socket was down are gone, so on every
 *  RE-connect pull `/api/rooms/live` once and reconcile the engaged rooms — push any
 *  missed report-to-human (deduped via seenReports) and say so once for an engaged room
 *  that is no longer live (closed, or died with an engine restart). Pull-based on
 *  purpose: no engine-side outbox/ack machinery for a gap this rare. */
async function reconcileAfterReconnect(): Promise<void> {
  if (engagedRooms.size === 0) return;
  let rooms: LiveRoom[];
  try {
    const r = await fetch(`${ENGINE}/api/rooms/live`, { signal: AbortSignal.timeout(5000) });
    if (!r.ok) throw new Error(String(r.status));
    rooms = (await r.json()) as LiveRoom[];
  } catch (e) {
    dbg(`reconcile: fetch failed — ${(e as Error).message}`);
    return;
  }
  const liveById = new Map(rooms.map((r) => [r.id, r]));
  for (const roomId of engagedRooms) {
    const room = liveById.get(roomId);
    if (!room) {
      if (notifiedGone.has(roomId)) continue;
      notifiedGone.add(roomId);
      const name = roomNames.get(roomId) ?? roomId.slice(0, 8);
      dbg(`reconcile: engaged room "${name}" no longer live — notifying`);
      pushReport(
        `[kild] room "${name}" (${roomId}) is no longer live — it was closed, or its agents ` +
          `died with an engine restart, while the report bridge was down. Use kild_rooms for ` +
          `current state and re-open a room if the workstream is unfinished.`,
      );
      continue;
    }
    roomNames.set(room.id, room.name);
    for (const m of room.log) {
      if (!m.id || m.system || m.implicit || !m.to?.includes('human')) continue;
      if (seenReports.has(m.id)) continue;
      seenReports.add(m.id);
      dbg(`reconcile: missed report in "${room.name}" from ${m.from} — pushing`);
      pushReport(
        `[kild] room "${room.name}" (${room.id}) — @${m.from} reports (delivered late; the ` +
          `report bridge was down):\n${m.text}\n\n` +
          `(React if action is needed; the room stays open and idle for follow-up.)`,
      );
    }
  }
}

/** Force a fresh WS, dropping any existing one (used on restart detection / reconnect). */
function reconnect(): void {
  if (typeof WebSocket === 'undefined') return;
  const old = ws;
  ws = undefined;
  try {
    old?.close();
  } catch {
    /* already closing */
  }
  connectWs();
}

function connectWs(): void {
  if (typeof WebSocket === 'undefined') return;
  let sock: WebSocket;
  try {
    sock = new WebSocket(`${ENGINE.replace(/^http/, 'ws')}/ws`);
  } catch (e) {
    dbg(`ws: construct threw, retry 5s — ${(e as Error).message}`);
    setTimeout(connectWs, 5000);
    return;
  }
  ws = sock;
  sock.onopen = () => {
    dbg(`ws: open (${engagedRooms.size} engaged rooms)`);
    if (everConnected) void reconcileAfterReconnect();
    everConnected = true;
  };
  sock.onmessage = (ev) => {
      try {
        const msg = JSON.parse(String(ev.data)) as {
          rooms?: Array<{ id: string; name: string }>;
          roomMessage?: {
            id: string;
            roomId: string;
            from: string;
            to: string[];
            text: string;
            system?: boolean;
            implicit?: boolean;
          };
        };
        if (msg.rooms) for (const r of msg.rooms) roomNames.set(r.id, r.name);
        const m = msg.roomMessage;
        if (!m) return;
        if (m.system || m.implicit) return;
        if (!engagedRooms.has(m.roomId)) {
          dbg(`msg: ${m.from}→${JSON.stringify(m.to)} in ${m.roomId.slice(0, 8)} — NOT engaged, skip`);
          return;
        }
        if (!m.to?.includes('human')) {
          dbg(`msg: ${m.from}→${JSON.stringify(m.to)} in ${m.roomId.slice(0, 8)} — not to @human, skip`);
          return;
        }
        if (seenReports.has(m.id)) return;
        seenReports.add(m.id);
        const name = roomNames.get(m.roomId) ?? m.roomId.slice(0, 8);
        dbg(`msg: ${m.from}→@human in "${name}" — PUSHING`);
        pushReport(
          `[kild] room "${name}" (${m.roomId}) — @${m.from} reports:\n${m.text}\n\n` +
            `(React if action is needed; the room stays open and idle for follow-up.)`,
        );
      } catch (e) {
        dbg(`ws: onmessage threw — ${(e as Error).message}`);
      }
    };
  sock.onclose = () => {
    if (ws !== sock) return; // superseded by a newer socket (restart reconnect)
    ws = undefined;
    dbg('ws: closed, reconnect in 3s');
    setTimeout(connectWs, 3000);
  };
  sock.onerror = () => {
    dbg('ws: error');
    try {
      sock.close();
    } catch {
      /* already closing */
    }
  };
}

/** Heartbeat: a read-only WS client doesn't reliably see the engine die, so poll health and
 *  force-reconnect when the engine's bootId changes (a restart) or the socket isn't OPEN. */
function heartbeat(): void {
  void engineUp().catch(() => false);
  fetch(`${ENGINE}/api/health`, { signal: AbortSignal.timeout(2000) })
    .then((r) => (r.ok ? (r.json() as Promise<{ bootId?: string }>) : Promise.reject()))
    .then((h) => {
      if (h.bootId && lastBootId && h.bootId !== lastBootId) {
        dbg(`heartbeat: engine restarted (${lastBootId.slice(0, 8)}→${h.bootId.slice(0, 8)}) — reconnecting`);
        lastBootId = h.bootId;
        reconnect();
      } else if (h.bootId && !lastBootId) {
        lastBootId = h.bootId;
        if (ws?.readyState !== 1) reconnect(); // OPEN === 1
      } else if (ws?.readyState !== 1) {
        dbg('heartbeat: socket not open — reconnecting');
        reconnect();
      }
    })
    .catch(() => {
      /* engine unreachable — the next successful beat with a new bootId reconnects */
    });
}

/** One WS subscription to the engine, reading the latest `currentPi` at delivery time. */
function watchEngine(): void {
  if (watching || typeof WebSocket === 'undefined') return;
  watching = true;
  connectWs();
  setInterval(heartbeat, 8000);
}

/** Mark a room's reports-to-human as something this session wants pushed to it. */
function engage(roomId: string): void {
  engagedRooms.add(roomId);
  dbg(`engage ${roomId.slice(0, 8)} (${engagedRooms.size} total)`);
  watchEngine();
}

// ── extension entrypoint ──────────────────────────────────────────────────────────────
export default function (pi: PiExtensionAPI) {
  // Refresh the live handle (the factory re-runs on session replacement) and flush any
  // reports that queued while the previous handle was stale.
  currentPi = pi;
  drainReports();

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
      engage(res.id); // push this room's reports-to-human into the session
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
        return `${r.id}\n    ${r.name} [${participantLine(r)}]${gitLine(r.git)}${openDecisionsLine(r)}${resumeLines(r)}${lastLine}`;
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
      engage(p.id); // posting into a room engages it: its reports now push here
      return { content: [{ type: 'text', text: res.message }] };
    },
  });

  pi.registerTool({
    name: 'kild_room_close',
    label: 'kild: close room',
    description:
      'Close a kild room: stops every agent and archives the transcript. DESTRUCTIVE — all ' +
      "agent context is lost forever. Call ONLY when the human explicitly says to close. " +
      'Finished rooms should stay open (idle) for follow-up. A room with open decisions ' +
      'refuses to close: get each resolved (a "resolved[<key>]: <how>" post), or pass ' +
      'force ONLY when the human explicitly says to abandon them.',
    parameters: Type.Object({
      id: Type.String({ description: 'Room id.' }),
      force: Type.Optional(
        Type.Boolean({
          description:
            'Close past open decisions. Only on an explicit human instruction — this buries ' +
            'unresolved decisions.',
        }),
      ),
    }),
    async execute(_id, params) {
      const p = params as { id: string; force?: boolean };
      const res = await engineFetch<{ message: string }>(`/api/rooms/${encodeURIComponent(p.id)}/close`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(p.force ? { force: true } : {}),
      });
      return { content: [{ type: 'text', text: res.message }] };
    },
  });

  pi.registerTool({
    name: 'kild_agents',
    label: 'kild: list agent personas',
    description:
      'List the agent personas available in a project (its .claude/agents, .pi/agents, and ' +
      'config-plugged packs like prp-core) — the valid `agent` values for kild_open_room ' +
      'participants. "default" is always available (general-purpose, no persona).',
    parameters: Type.Object({
      project: Type.Optional(Type.String({ description: 'Absolute project path. Omit for global-only personas.' })),
    }),
    async execute(_id, params) {
      const p = params as { project?: string };
      const q = p.project ? `?project=${encodeURIComponent(p.project)}` : '';
      const agents = await engineFetch<Array<{ name: string; description: string }>>(`/api/agents${q}`);
      const lines = agents.map((a) => (a.description ? `${a.name} — ${a.description}` : a.name));
      return { content: [{ type: 'text', text: truncate(lines.join('\n')) }], details: { count: agents.length } };
    },
  });

  pi.registerTool({
    name: 'kild_fleet_start',
    label: 'kild: start fleet driver',
    description:
      'Spawn a DETACHED, persistent fleet-driver session that keeps orchestrating rooms after ' +
      'you disconnect: it gets the room-control tools and your goal as its first prompt. ' +
      'Returns its session id. Use for long campaigns you want to hand off; for work you are ' +
      'driving yourself, just use the kild_room_* tools directly.',
    parameters: Type.Object({
      goal: Type.String({ description: 'The campaign goal, delivered as the driver’s first prompt.' }),
      project: Type.Optional(Type.String({ description: 'Absolute project path the driver works in. Defaults to the current directory.' })),
      agent: Type.Optional(Type.String({ description: 'Persona for the driver (default: the general-purpose "default").' })),
      model: Type.Optional(Type.String({ description: 'provider/model ref for the driver (pick a strong orchestration model).' })),
    }),
    async execute(_id, params) {
      const p = params as { goal: string; project?: string; agent?: string; model?: string };
      const res = await engineFetch<{ id: string }>('/api/sessions', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          cwd: p.project ?? process.cwd(),
          agent: p.agent ?? 'default',
          model: p.model,
          projectName: 'fleet',
          fleet: true,
          prompt: p.goal,
        }),
      });
      return { content: [{ type: 'text', text: `fleet driver started: ${res.id}` }], details: { sessionId: res.id } };
    },
  });

  pi.registerTool({
    name: 'kild_fleet_post',
    label: 'kild: post to fleet driver',
    description: 'Send a steering message to a running fleet-driver session (by session id).',
    parameters: Type.Object({
      id: Type.String({ description: 'Fleet-driver session id.' }),
      text: Type.String({ description: 'The steering message.' }),
    }),
    async execute(_id, params) {
      const p = params as { id: string; text: string };
      await engineFetch(`/api/sessions/${encodeURIComponent(p.id)}/prompt`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text: p.text }),
      });
      return { content: [{ type: 'text', text: 'posted' }] };
    },
  });

  pi.registerTool({
    name: 'kild_fleet_stop',
    label: 'kild: stop fleet driver',
    description:
      'Stop a fleet-driver session by id. Its rooms keep running (close those separately, ' +
      'and only on the human’s explicit order).',
    parameters: Type.Object({
      id: Type.String({ description: 'Fleet-driver session id.' }),
    }),
    async execute(_id, params) {
      const p = params as { id: string };
      await engineFetch(`/api/sessions/${encodeURIComponent(p.id)}/stop`, { method: 'POST' });
      return { content: [{ type: 'text', text: 'stopped' }] };
    },
  });

  pi.registerTool({
    name: 'kild_sessions',
    label: 'kild: list sessions',
    description: 'List live kild sessions (fleet drivers and one-shot runs) with their models.',
    parameters: Type.Object({}),
    async execute() {
      const sessions = await engineFetch<Array<{ id: string; agent?: string; model?: string; projectName?: string }>>('/api/sessions');
      if (sessions.length === 0) return { content: [{ type: 'text', text: 'no live sessions' }] };
      const lines = sessions.map(
        (s) => `${s.id}\t${s.agent ?? 'default'}${s.model ? ` (${s.model})` : ''}${s.projectName ? ` · ${s.projectName}` : ''}`,
      );
      return { content: [{ type: 'text', text: truncate(lines.join('\n')) }], details: { count: sessions.length } };
    },
  });

  // The driver guide rides the system prompt every turn — chained after other extensions.
  pi.on('before_agent_start', ((event: { systemPrompt: string }) => ({
    systemPrompt: `${event.systemPrompt}\n\n${driverGuide()}`,
  })) as never);
}
