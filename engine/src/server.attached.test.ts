import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * The attached-participant routes, against the real Hono app (`server.ts`'s default
 * export) via `fetch` — no port is bound (Bun only serves the default export for the
 * entry module), so the operator's engine on 4517 is never touched. KILD_HOME points at a
 * temp dir BEFORE the dynamic import so the module's load-time side effects see an empty
 * state, not the user's.
 *
 * Only the paths that need no live room are covered here; join/drain against a real room
 * would mean spawning pi sessions. The room manager's own tests cover that half.
 */
let fetchApp: (req: Request) => Response | Promise<Response>;

beforeAll(async () => {
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-attached-'));
  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
});

const post = (url: string, body: unknown) =>
  fetchApp(
    new Request(`http://localhost${url}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    }),
  );

test.each(['join', 'drain'])('POST %s without a name is a 400', async (verb) => {
  const res = await post(`/api/rooms/room-1/${verb}`, {});
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'name required' });
});

test.each(['join', 'drain'])('POST %s with a blank name is a 400', async (verb) => {
  const res = await post(`/api/rooms/room-1/${verb}`, { name: '   ' });
  expect(res.status).toBe(400);
});

test.each(['join', 'drain'])('POST %s on an unknown room is a clean 404', async (verb) => {
  const res = await post(`/api/rooms/no-such-room/${verb}`, { name: 'claude' });
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such room: no-such-room' });
});

test('drain is not exposed as a GET — a destructive read must not be retryable', async () => {
  const res = await fetchApp(new Request('http://localhost/api/rooms/room-1/drain'));
  expect(res.status).toBe(404); // no route matches the GET
});
