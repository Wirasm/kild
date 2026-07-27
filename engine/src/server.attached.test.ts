import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * The attached-agent routes, against the real Hono app (`server.ts`'s default
 * export) via `fetch` — no port is bound (Bun only serves the default export for the
 * entry module), so the operator's engine on 4517 is never touched. KILD_HOME points at a
 * temp dir BEFORE the dynamic import so the module's load-time side effects see an empty
 * state, not the user's.
 *
 * Only the paths that need no live kild are covered here; attach/drain against a real kild
 * would mean spawning pi sessions. The kild manager's own tests cover that half.
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

test.each(['agents/attach', 'inbox/drain'])('POST %s without a handle is a 400', async (verb) => {
  const res = await post(`/api/kilds/kild-1/${verb}`, {});
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'handle required' });
});

test.each([
  'agents/attach',
  'inbox/drain',
])('POST %s with a blank handle is a 400', async (verb) => {
  const res = await post(`/api/kilds/kild-1/${verb}`, { handle: '   ' });
  expect(res.status).toBe(400);
});

test.each([
  'agents/attach',
  'inbox/drain',
])('POST %s on an unknown kild is a clean 404', async (verb) => {
  const res = await post(`/api/kilds/no-such-kild/${verb}`, { handle: 'claude' });
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: no-such-kild' });
});

test('drain is not exposed as a GET — a destructive read must not be retryable', async () => {
  const res = await fetchApp(new Request('http://localhost/api/kilds/kild-1/inbox/drain'));
  expect(res.status).toBe(404); // no route matches the GET
});
