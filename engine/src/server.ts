import { Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';

import { listAgents } from './kild/agents.ts';
import { addProject, loadProjects } from './kild/projects.ts';
import { SessionManager, type UiEvent } from './kild/sessions.ts';

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
app.get('/api/agents', async (c) => {
  const project = c.req.query('project');
  return c.json(await listAgents(project));
});

// ── Sessions (WebSocket) ──────────────────────────────────────────────────────
// One SessionManager per connection; messages mirror the old Tauri commands.
type ClientMessage =
  | { type: 'spawn'; id: string; model?: string; cwd?: string; agent?: string }
  | { type: 'prompt'; id: string; text: string }
  | { type: 'stop'; id: string };

app.get(
  '/ws',
  upgradeWebSocket(() => {
    const manager = new SessionManager();
    const live = new Set<string>();
    return {
      onMessage(evt, ws) {
        const send = (id: string, event: UiEvent) =>
          ws.send(JSON.stringify({ session: id, event }));
        const msg = JSON.parse(String(evt.data)) as ClientMessage;
        if (msg.type === 'spawn') {
          live.add(msg.id);
          manager.spawn(msg.id, { model: msg.model, cwd: msg.cwd, agent: msg.agent }, (e) =>
            send(msg.id, e),
          );
        } else if (msg.type === 'prompt') {
          manager.prompt(msg.id, msg.text).catch((err) => {
            console.error('prompt failed:', err);
            send(msg.id, { kind: 'agent_end' });
          });
        } else if (msg.type === 'stop') {
          void manager.stop(msg.id, (e) => send(msg.id, e));
          live.delete(msg.id);
        }
      },
      onClose() {
        for (const id of live) void manager.stop(id, () => {});
        live.clear();
      },
    };
  }),
);

console.log(`kild-engine listening on http://localhost:${PORT}`);

export default { port: PORT, fetch: app.fetch, websocket };
