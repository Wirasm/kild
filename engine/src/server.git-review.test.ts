import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * Error-path tests for the review-intelligence routes, against the real Hono app
 * (`server.ts`'s default export) via `fetch` — no port is bound (Bun only serves the
 * default export for the entry module). KILD_HOME points at a temp dir BEFORE the
 * dynamic import so the module's load-time side effects (project prune, room archive
 * load) see an empty state, not the user's.
 */
let fetchApp: (req: Request) => Response | Promise<Response>;

beforeAll(async () => {
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-review-'));
  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
});

const get = (url: string) => fetchApp(new Request(`http://localhost${url}`));

test.each(['commits', 'files'])('GET git/%s on an unknown room is a clean 404', async (leaf) => {
  const res = await get(`/api/rooms/no-such-room/git/${leaf}`);
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({
    error: 'no such live room: no-such-room',
    code: 'not_found',
  });
});

test('GET git/diff on an unknown room is a clean 404', async () => {
  const res = await get('/api/rooms/no-such-room/git/diff?path=README.md');
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({
    error: 'no such live room: no-such-room',
    code: 'not_found',
  });
});

test('GET git/diff without a path query is a 400', async () => {
  const res = await get('/api/rooms/no-such-room/git/diff');
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'path query parameter required' });
});
