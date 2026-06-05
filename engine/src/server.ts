// One binary, two roles: `KILD_ROLE=worker` runs a single agent session (one per
// process — the coding-agent SDK requires it); otherwise this is the engine.
if (process.env.KILD_ROLE === 'worker') {
  await (await import('./worker.ts')).runWorker();
}

import { Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';

import { listAgents } from './kild/agents.ts';
import { addProject, loadProjects } from './kild/projects.ts';
import { type Outbound, sessionManager } from './kild/sessions.ts';

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

// ── Sessions ──────────────────────────────────────────────────────────────────
app.get('/api/sessions', (c) => c.json(sessionManager.list()));

// ── Live stream (WebSocket) ─────────────────────────────────────────────────
// Every connection subscribes to the same broadcast — so sessions started by any
// client (UI or CLI) are visible to all. Sessions are engine-owned and survive a
// connection drop.
type ClientMessage =
  | {
      type: 'spawn';
      id: string;
      model?: string;
      cwd?: string;
      agent?: string;
      projectName?: string;
      origin?: 'ui' | 'cli';
    }
  | { type: 'prompt'; id: string; text: string }
  | { type: 'stop'; id: string };

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
  if (m.type === 'spawn' || m.type === 'stop') return m as ClientMessage;
  if (m.type === 'prompt' && typeof m.text === 'string') return m as ClientMessage;
  return null;
}

app.get(
  '/ws',
  async (c, next) => {
    if (!originAllowed(c.req.header('origin'))) return c.text('forbidden', 403);
    return next();
  },
  upgradeWebSocket(() => {
    let unsubscribe: (() => void) | undefined;
    return {
      onOpen(_evt, ws) {
        unsubscribe = sessionManager.subscribe((msg: Outbound) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = parseClientMessage(String(evt.data));
        if (!msg) return; // ignore malformed / unknown frames
        if (msg.type === 'spawn') {
          sessionManager.spawn(msg.id, msg, msg.origin ?? 'ui');
        } else if (msg.type === 'prompt') {
          sessionManager.prompt(msg.id, msg.text);
        } else if (msg.type === 'stop') {
          sessionManager.stop(msg.id);
        }
      },
      onClose() {
        unsubscribe?.();
      },
    };
  }),
);

console.log(`kild-engine listening on http://${HOST}:${PORT}`);

export default { port: PORT, hostname: HOST, fetch: app.fetch, websocket };
