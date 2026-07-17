// One binary, two roles: `KILD_ROLE=worker` runs a single agent session (one per
// process — the coding-agent SDK requires it); otherwise this is the engine.
if (process.env.KILD_ROLE === 'worker') {
  await (await import('./worker.ts')).runWorker();
}

import { execFile as execFileCb } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import { promisify } from 'node:util';

import { Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';

import { listAgents } from './kild/agents.ts';
import { addProject, findProject, loadProjects } from './kild/projects.ts';
import { parseMentions } from './kild/room/parse-mentions.ts';
import { roomManager } from './kild/room/room-manager.ts';
import { type CommandResult, HUMAN, type ParticipantSpec } from './kild/room/room-types.ts';
import { sessionManager } from './kild/sessions.ts';
import {
  assertSafeBranch,
  forceRemoveWorktree,
  listWorktrees,
  pruneMergedWorktrees,
  removeWorktree,
  worktreePath,
  worktreesRoot,
} from './kild/worktree.ts';

const execFile = promisify(execFileCb);
const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

const PORT = Number(process.env.KILD_PORT ?? 4517);
// Bind to loopback only: the engine holds the user's OAuth and runs bash in their
// repos, so it must never be reachable from the LAN.
const HOST = process.env.KILD_HOST ?? '127.0.0.1';

// Only the cockpit's own origins may drive the engine from a browser context.
// A request with no Origin is a non-browser client (the CLI / curl); browsers
// always send one, so an unexpected Origin is a hostile web page — rejected.
const ALLOWED_ORIGINS = new Set([
  'http://localhost:1420',
  'http://127.0.0.1:1420',
  'tauri://localhost',
  'https://tauri.localhost',
]);
const originAllowed = (origin: string | undefined): boolean =>
  origin == null || ALLOWED_ORIGINS.has(origin);

const { upgradeWebSocket, websocket } = createBunWebSocket();

const app = new Hono();
app.use('/*', cors({ origin: (origin) => (origin && ALLOWED_ORIGINS.has(origin) ? origin : '') }));

app.get('/api/health', (c) => c.json({ ok: true, name: 'kild-engine' }));

// ── Projects ────────────────────────────────────────────────────────────────
app.get('/api/projects', async (c) => c.json(await loadProjects()));
app.post('/api/projects', async (c) => {
  const { name, path } = await c.req.json<{ name: string; path: string }>();
  try {
    return c.json(await addProject(name, path));
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Agents ────────────────────────────────────────────────────────────────────
app.get('/api/agents', async (c) => c.json(await listAgents(c.req.query('project'))));

// ── Worktrees ─────────────────────────────────────────────────────────────────
// A project query may be a registered name or a raw path (the cockpit passes the
// path; the CLI a name). Worktree names a live session is using are never pruned.
async function resolveProjectPath(q: string | undefined): Promise<string | null> {
  if (!q) return null;
  return (await findProject(q))?.path ?? q;
}

function participantSpecs(input: unknown): ParticipantSpec[] | null {
  if (!Array.isArray(input)) return null;
  const participants: ParticipantSpec[] = [];
  for (const item of input) {
    if (typeof item !== 'object' || item === null) return null;
    const participant = item as Record<string, unknown>;
    if (typeof participant.name !== 'string') return null;
    if (participant.agent !== undefined && typeof participant.agent !== 'string') return null;
    if (participant.model !== undefined && typeof participant.model !== 'string') return null;
    participants.push({
      name: participant.name,
      agent: typeof participant.agent === 'string' ? participant.agent : undefined,
      model: typeof participant.model === 'string' ? participant.model : undefined,
    });
  }
  return participants;
}

function envRecord(input: unknown): Record<string, string> | undefined {
  if (input === undefined) return undefined;
  if (typeof input !== 'object' || input === null) return undefined;
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(input)) {
    if (typeof value !== 'string') return undefined;
    env[key] = value;
  }
  return env;
}

function addressKickoff(kickoff: string, participants: ParticipantSpec[]): string {
  const lead = participants[0]?.name;
  if (!lead) return kickoff;
  const addressed = parseMentions(kickoff).some((handle) =>
    participants.some((p) => p.name === handle),
  );
  return addressed ? kickoff : `@${lead} ${kickoff}`;
}

function roomResultStatus(result: Extract<CommandResult<unknown>, { ok: false }>): 404 | 409 {
  return result.code === 'not_found' ? 404 : 409;
}

const worktreesInUse = (): Set<string> =>
  new Set(
    sessionManager
      .list()
      .map((s) => s.worktree)
      .filter((w): w is string => typeof w === 'string'),
  );

app.get('/api/worktrees', async (c) => {
  const repo = await resolveProjectPath(c.req.query('project'));
  if (!repo) return c.json({ error: 'project required' }, 400);
  try {
    await pruneMergedWorktrees(repo, worktreesInUse()); // prune-merged on every list
    const trees = (await listWorktrees(repo)).filter((t) => t.branch.startsWith('kild/'));
    return c.json(trees);
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

app.delete('/api/worktrees', async (c) => {
  const { project, name, force } = await c.req.json<{
    project: string;
    name: string;
    force?: boolean;
  }>();
  const repo = await resolveProjectPath(project);
  if (!repo) return c.json({ error: 'project required' }, 400);
  if (force !== undefined && typeof force !== 'boolean') {
    return c.json({ error: 'force must be a boolean' }, 400);
  }
  try {
    assertSafeBranch(name); // allowlist before building a path under worktreesRoot()
    const wtPath = worktreePath(name);
    const result = worktreesInUse().has(name)
      ? { ok: false as const, code: 'in_use' as const }
      : force
        ? await forceRemoveWorktree(repo, wtPath)
        : await removeWorktree(repo, wtPath);
    if (!result.ok) {
      const error =
        result.code === 'dirty'
          ? `worktree '${name}' has uncommitted or untracked files; retry with force: true to discard them`
          : result.code === 'in_use'
            ? `worktree '${name}' is in use by a live session`
            : `worktree '${name}' was not found`;
      return c.json(
        { error, code: result.code, ...(result.files ? { files: result.files } : {}) },
        result.code === 'not_found' ? 404 : 409,
      );
    }
    return c.json({ ok: true, name });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

app.post('/api/worktrees/prune', async (c) => {
  const { project } = await c.req.json<{ project: string }>();
  const repo = await resolveProjectPath(project);
  if (!repo) return c.json({ error: 'project required' }, 400);
  try {
    const pruned = await pruneMergedWorktrees(repo, worktreesInUse());
    return c.json({ pruned });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Open in OS ────────────────────────────────────────────────────────────────
// Reveal a worktree path in the OS file browser. Only paths under the worktree root
// are allowed — the engine is loopback-only but must never shell `open` on an
// arbitrary path. Keeps the cockpit pure-web (no Tauri opener API needed).
app.post('/api/open', async (c) => {
  const { path: target } = await c.req.json<{ path: string }>();
  const root = worktreesRoot();
  const resolved = path.resolve(target ?? '');
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    return c.json({ error: 'path is not under the worktree root' }, 403);
  }
  try {
    const opener = process.platform === 'darwin' ? 'open' : 'xdg-open';
    await execFile(opener, [resolved]);
    return c.json({ ok: true });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// Open an external http(s) URL in the OS browser. The cockpit routes rendered links
// here so a click never navigates the Tauri webview away from the app. Restricted to
// http/https — never file://, app schemes, etc. execFile (no shell) → no injection.
app.post('/api/open-url', async (c) => {
  const { url } = await c.req.json<{ url: string }>();
  let parsed: URL;
  try {
    parsed = new URL(url ?? '');
  } catch {
    return c.json({ error: 'invalid url' }, 400);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    return c.json({ error: 'only http(s) urls may be opened' }, 403);
  }
  try {
    const opener = process.platform === 'darwin' ? 'open' : 'xdg-open';
    await execFile(opener, [parsed.toString()]);
    return c.json({ ok: true });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Sessions ──────────────────────────────────────────────────────────────────
app.get('/api/sessions', (c) => c.json(sessionManager.list()));

// ── Rooms ─────────────────────────────────────────────────────────────────────
// Past rooms recovered from disk (read-only history). Live rooms flow over the WS
// (`{rooms}` summaries + `{roomMessage}` posts); this is the conversation record of
// rooms from previous engine runs — their participant subprocesses are long gone.
app.get('/api/rooms/archive', (c) => c.json(roomManager.archived()));
// Live rooms WITH their logs — so a cockpit joining a room it didn't open (or after a
// refresh) can load the conversation so far. The WS only streams *new* messages.
app.get('/api/rooms/live', (c) => c.json(roomManager.liveRooms()));
app.post('/api/rooms', async (c) => {
  const body = await c.req.json<{
    name?: unknown;
    cwd?: unknown;
    project?: unknown;
    worktree?: unknown;
    participants?: unknown;
    kickoff?: unknown;
    from?: unknown;
  }>();
  if (typeof body.name !== 'string') return c.json({ error: 'name required' }, 400);
  if (typeof body.kickoff !== 'string' || !body.kickoff.trim()) {
    return c.json({ error: 'kickoff required' }, 400);
  }
  if (body.cwd !== undefined && typeof body.cwd !== 'string')
    return c.json({ error: 'cwd must be a string' }, 400);
  if (body.project !== undefined && typeof body.project !== 'string') {
    return c.json({ error: 'project must be a string' }, 400);
  }
  if (body.worktree !== undefined && typeof body.worktree !== 'string') {
    return c.json({ error: 'worktree must be a string' }, 400);
  }
  const participants = participantSpecs(body.participants);
  if (!participants || participants.length === 0) {
    return c.json({ error: 'participants must name at least one agent' }, 400);
  }

  const cwd =
    typeof body.project === 'string' ? await resolveProjectPath(body.project) : (body.cwd ?? null);
  if (!cwd) return c.json({ error: 'cwd or project required' }, 400);

  const id = randomUUID();
  const opened = await roomManager.open(id, {
    name: body.name,
    cwd,
    participants,
    worktree: body.worktree,
  });
  if (!opened.ok) return c.json({ error: opened.message }, roomResultStatus(opened));
  // Honest attribution: a kickoff posted by an agent operator (e.g. the fleet
  // brain) carries its name; the transcript must never claim the human spoke.
  const kickoff = addressKickoff(body.kickoff, participants);
  const posted =
    typeof body.from === 'string' && body.from.trim()
      ? await roomManager.postAs(id, body.from.trim(), kickoff)
      : await roomManager.postFromHuman(id, kickoff);
  if (!posted.ok) {
    await roomManager.close(id);
    return c.json({ error: posted.message }, roomResultStatus(posted));
  }
  return c.json({ ok: true, id: opened.value.roomId, message: opened.value.message });
});
app.post('/api/rooms/:id/post', async (c) => {
  const { text, from } = await c.req.json<{ text?: unknown; from?: unknown }>();
  if (typeof text !== 'string') return c.json({ error: 'text required' }, 400);
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  const id = c.req.param('id');
  const result =
    (from ?? HUMAN) === HUMAN
      ? await roomManager.postFromHuman(id, text)
      : await roomManager.postAs(id, from as string, text);
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
app.post('/api/rooms/:id/close', async (c) => {
  const result = await roomManager.close(c.req.param('id'));
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});

// ── Live stream (WebSocket) ─────────────────────────────────────────────────
// Every connection subscribes to the room broadcast — so rooms opened by any client
// (cockpit or CLI) are visible to all. Rooms are engine-owned and survive a drop.
// Frames carry the room id as `id`; sessions are the internal substrate, not on the wire.
type ClientMessage =
  | {
      type: 'spawn';
      id: string;
      cwd?: string;
      agent?: string;
      model?: string;
      worktree?: string;
      projectName?: string;
      env?: Record<string, string>;
    }
  | { type: 'prompt'; id: string; text: string; from?: string }
  | { type: 'stop'; id: string }
  | {
      type: 'room_open';
      id: string;
      name: string;
      cwd: string;
      participants: Array<{ name: string; agent?: string; model?: string }>;
      worktree?: string;
    }
  | { type: 'room_post'; id: string; text: string }
  | { type: 'room_add'; id: string; participant: { name: string; agent?: string; model?: string } }
  | { type: 'room_halt'; id: string }
  | { type: 'room_close'; id: string };

function parseClientMessage(data: string): ClientMessage | null {
  let msg: unknown;
  try {
    msg = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof msg !== 'object' || msg === null) return null;
  const m = msg as Record<string, unknown>;
  if (typeof m.id !== 'string') return null;
  if (m.type === 'spawn') {
    if (m.cwd !== undefined && typeof m.cwd !== 'string') return null;
    if (m.agent !== undefined && typeof m.agent !== 'string') return null;
    if (m.model !== undefined && typeof m.model !== 'string') return null;
    if (m.worktree !== undefined && typeof m.worktree !== 'string') return null;
    if (m.projectName !== undefined && typeof m.projectName !== 'string') return null;
    const env = envRecord(m.env);
    if (m.env !== undefined && env === undefined) return null;
    return { ...(m as Omit<Extract<ClientMessage, { type: 'spawn' }>, 'env'>), env };
  }
  if (m.type === 'prompt') {
    if (typeof m.text !== 'string') return null;
    if (m.from !== undefined && typeof m.from !== 'string') return null;
    return m as ClientMessage;
  }
  if (m.type === 'stop') return m as ClientMessage;
  if (m.type === 'room_open') {
    if (typeof m.name !== 'string' || typeof m.cwd !== 'string' || !Array.isArray(m.participants)) {
      return null;
    }
    if (m.worktree !== undefined && typeof m.worktree !== 'string') return null;
    return m as ClientMessage;
  }
  if (m.type === 'room_post' && typeof m.text === 'string') return m as ClientMessage;
  if (m.type === 'room_add' && typeof m.participant === 'object' && m.participant !== null) {
    return m as ClientMessage;
  }
  if (m.type === 'room_halt') return m as ClientMessage;
  if (m.type === 'room_close') return m as ClientMessage;
  return null;
}

app.get(
  '/ws',
  async (c, next) => {
    if (!originAllowed(c.req.header('origin'))) return c.text('forbidden', 403);
    return next();
  },
  upgradeWebSocket(() => {
    let unsubscribeRooms: (() => void) | undefined;
    let unsubscribeSessions: (() => void) | undefined;
    // Room commands became async when open() gained validation, which broke the
    // implicit frame ordering clients rely on (open immediately followed by the
    // kickoff post raced, and the post was rejected with "no such room"). Frames
    // on one connection execute strictly in arrival order.
    let queue: Promise<void> = Promise.resolve();
    const enqueue = (label: string, task: () => Promise<{ ok: boolean; message?: string }>) => {
      queue = queue.then(async () => {
        const result = await task();
        if (!result.ok) console.warn(`kild: ${label} rejected: ${result.message}`);
      });
      queue = queue.catch((err) => console.warn(`kild: ${label} failed: ${errText(err)}`));
    };
    return {
      onOpen(_evt, ws) {
        unsubscribeRooms = roomManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
        unsubscribeSessions = sessionManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = parseClientMessage(String(evt.data));
        if (!msg) return; // ignore malformed / unknown frames
        if (msg.type === 'spawn') {
          sessionManager.spawn(
            msg.id,
            {
              cwd: msg.cwd,
              agent: msg.agent,
              model: msg.model,
              worktree: msg.worktree,
              projectName: msg.projectName,
              env: msg.env,
            },
            'cli',
          );
        } else if (msg.type === 'prompt') {
          sessionManager.prompt(msg.id, msg.text, msg.from);
        } else if (msg.type === 'stop') {
          sessionManager.stop(msg.id);
        } else if (msg.type === 'room_open') {
          enqueue(`room_open ${msg.id}`, () =>
            roomManager.open(msg.id, {
              name: msg.name,
              cwd: msg.cwd,
              participants: msg.participants,
              worktree: msg.worktree,
            }),
          );
        } else if (msg.type === 'room_post') {
          enqueue(`room_post ${msg.id}`, () => roomManager.postFromHuman(msg.id, msg.text));
        } else if (msg.type === 'room_add') {
          enqueue(`room_add ${msg.id}`, () => roomManager.addParticipant(msg.id, msg.participant));
        } else if (msg.type === 'room_halt') {
          enqueue(`room_halt ${msg.id}`, () => roomManager.halt(msg.id));
        } else if (msg.type === 'room_close') {
          enqueue(`room_close ${msg.id}`, () => roomManager.close(msg.id));
        }
      },
      onClose() {
        unsubscribeRooms?.();
        unsubscribeSessions?.();
      },
    };
  }),
);

console.log(`kild-engine listening on http://${HOST}:${PORT}`);

// One-shot merge-prune on start: clean up worktrees whose kild/* branch already
// landed in the default branch. Fire-and-forget per registered project; no timer.
void loadProjects()
  .then((projects) =>
    Promise.all(
      projects.map((p) =>
        pruneMergedWorktrees(p.path, worktreesInUse()).catch((err) => {
          // A non-git/unreadable project dir is expected (skip quietly-ish); anything
          // else is logged rather than hidden.
          console.warn(`kild: startup prune skipped ${p.name}: ${errText(err)}`);
        }),
      ),
    ),
  )
  .catch((err) => console.warn(`kild: startup prune failed: ${errText(err)}`));

// Kill child workers on shutdown so a `--watch` reload or Ctrl-C never orphans
// them (otherwise they reparent to init and linger as zombie sessions).
for (const sig of ['SIGINT', 'SIGTERM'] as const) {
  process.on(sig, () => {
    sessionManager.shutdown();
    process.exit(0);
  });
}

export default { port: PORT, hostname: HOST, fetch: app.fetch, websocket };
