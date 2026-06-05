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

const { upgradeWebSocket, websocket } = createBunWebSocket();

const app = new Hono();
app.use('/*', cors());

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

/** Spawn a session over HTTP — used by the CLI so its session shows up in the UI. */
app.post('/api/sessions', async (c) => {
  const body = await c.req.json<{
    id: string;
    model?: string;
    cwd?: string;
    agent?: string;
    projectName?: string;
  }>();
  sessionManager.spawn(body.id, body, 'cli');
  return c.json({ id: body.id });
});
app.post('/api/sessions/:id/prompt', async (c) => {
  const { text } = await c.req.json<{ text: string }>();
  sessionManager.prompt(c.req.param('id'), text);
  return c.json({ ok: true });
});

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

app.get(
  '/ws',
  upgradeWebSocket(() => {
    let unsubscribe: (() => void) | undefined;
    return {
      onOpen(_evt, ws) {
        unsubscribe = sessionManager.subscribe((msg: Outbound) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = JSON.parse(String(evt.data)) as ClientMessage;
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

console.log(`kild-engine listening on http://localhost:${PORT}`);

export default { port: PORT, fetch: app.fetch, websocket };
