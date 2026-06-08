// One binary, two roles: `KILD_ROLE=worker` runs a single agent session (one per
// process — the coding-agent SDK requires it); otherwise this is the engine.
if (process.env.KILD_ROLE === 'worker') {
  await (await import('./worker.ts')).runWorker();
}

import { execFile as execFileCb } from 'node:child_process';
import path from 'node:path';
import { promisify } from 'node:util';

import { Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';

import { listAgents } from './kild/agents.ts';
import { addProject, findProject, loadProjects } from './kild/projects.ts';
import { roomManager } from './kild/room/room-manager.ts';
import { sessionManager } from './kild/sessions.ts';
import {
  assertSafeBranch,
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
  const { project, name } = await c.req.json<{ project: string; name: string }>();
  const repo = await resolveProjectPath(project);
  if (!repo) return c.json({ error: 'project required' }, 400);
  try {
    assertSafeBranch(name); // allowlist before building a path under worktreesRoot()
    if (worktreesInUse().has(name)) {
      return c.json({ error: `worktree '${name}' is in use by a live session` }, 409);
    }
    await removeWorktree(repo, worktreePath(name));
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

// ── Sessions ──────────────────────────────────────────────────────────────────
app.get('/api/sessions', (c) => c.json(sessionManager.list()));

// ── Rooms ─────────────────────────────────────────────────────────────────────
// Past rooms recovered from disk (read-only history). Live rooms flow over the WS
// (`{rooms}` summaries + `{roomMessage}` posts); this is the conversation record of
// rooms from previous engine runs — their participant subprocesses are long gone.
app.get('/api/rooms/archive', (c) => c.json(roomManager.archived()));

// ── Live stream (WebSocket) ─────────────────────────────────────────────────
// Every connection subscribes to the room broadcast — so rooms opened by any client
// (cockpit or CLI) are visible to all. Rooms are engine-owned and survive a drop.
// Frames carry the room id as `id`; sessions are the internal substrate, not on the wire.
type ClientMessage =
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
    return {
      onOpen(_evt, ws) {
        unsubscribeRooms = roomManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = parseClientMessage(String(evt.data));
        if (!msg) return; // ignore malformed / unknown frames
        if (msg.type === 'room_open') {
          roomManager.open(msg.id, {
            name: msg.name,
            cwd: msg.cwd,
            participants: msg.participants,
            worktree: msg.worktree,
          });
        } else if (msg.type === 'room_post') {
          roomManager.postFromHuman(msg.id, msg.text);
        } else if (msg.type === 'room_add') {
          roomManager.addParticipant(msg.id, msg.participant);
        } else if (msg.type === 'room_halt') {
          roomManager.halt(msg.id);
        } else if (msg.type === 'room_close') {
          roomManager.close(msg.id);
        }
      },
      onClose() {
        unsubscribeRooms?.();
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
