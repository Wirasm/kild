import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * `POST /api/kilds/:id/messages`'s `to` field, against the real Hono app via `fetch` — no
 * port is bound (Bun only serves the default export for the entry module), so the
 * operator's engine on 4517 is never touched. KILD_HOME points at a temp dir BEFORE the
 * dynamic import so load-time side effects see empty state, not the user's.
 *
 * Only validation is covered here: it runs before the kild lookup, so these need no live
 * kild. Delivery with `to` is the kild manager's own test ('a message with no addressee
 * defaults to the lead and never fans out to inboxes' and its addressed counterpart).
 */
let fetchApp: (req: Request) => Response | Promise<Response>;

beforeAll(async () => {
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-send-to-'));
  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
});

function send(body: unknown): Promise<Response> {
  return Promise.resolve(
    fetchApp(
      new Request('http://localhost/api/kilds/any-kild/messages', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      }),
    ),
  );
}

test('a non-array `to` is rejected', async () => {
  const res = await send({ text: 'hi', to: 'claude' });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'to must be an array of agent handles' });
});

test('a `to` holding non-strings is rejected', async () => {
  const res = await send({ text: 'hi', to: ['claude', 7] });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'to must be an array of agent handles' });
});

test('an empty `to` is rejected rather than silently meaning "the lead"', async () => {
  // `[]` reads as "addressed to nobody" but would behave as "addressed to the lead".
  // Rejecting it keeps the caller's intent and the delivery in agreement.
  const res = await send({ text: 'hi', to: [] });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'to must name at least one agent' });
});

test('a well-formed `to` passes validation and reaches the kild lookup', async () => {
  const res = await send({ text: 'hi', to: ['claude'] });
  // The kild does not exist, so this must fail on the KILD, never on `to`.
  expect(res.status).not.toBe(400);
  expect(await res.text()).not.toContain('to must');
});

test('omitting `to` is still valid — the lead default is unchanged', async () => {
  const res = await send({ text: 'hi' });
  expect(res.status).not.toBe(400);
  expect(await res.text()).not.toContain('to must');
});
