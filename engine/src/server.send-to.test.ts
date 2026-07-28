import { beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * `to` on the two REST surfaces that carry a message — `POST /api/kilds/:id/messages`
 * and the `kickoff` of `POST /api/kilds`. Both require it, for the same reason: the
 * engine never infers a recipient, so "who is this for?" is answered by the caller or
 * not at all.
 *
 * Against the real Hono app via `fetch` — no port is bound (Bun only serves the default
 * export for the entry module), so the operator's engine on 4517 is never touched.
 * KILD_HOME points at a temp dir BEFORE the dynamic import so load-time side effects see
 * empty state, not the user's.
 *
 * Only validation is covered here: it runs before the kild lookup, so these need no live
 * kild. Delivery is the kild manager's own test.
 */
let fetchApp: (req: Request) => Response | Promise<Response>;

beforeAll(async () => {
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-send-to-'));
  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
});

const post = (url: string, body: unknown): Promise<Response> =>
  Promise.resolve(
    fetchApp(
      new Request(`http://localhost${url}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      }),
    ),
  );

const send = (body: unknown) => post('/api/kilds/any-kild/messages', body);
const newKild = (kickoff: unknown) =>
  post('/api/kilds', { name: 'r', agents: [{ handle: 'agent' }], cwd: os.tmpdir(), kickoff });

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

test('an empty `to` is rejected — it addresses nobody, so it delivers to nobody', async () => {
  const res = await send({ text: 'hi', to: [] });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'to must name at least one agent' });
});

test('an OMITTED `to` is rejected too — there is no recipient to fall back to', async () => {
  const res = await send({ text: 'hi' });
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'to must be an array of agent handles' });
});

test('a well-formed `to` passes validation and reaches the kild lookup', async () => {
  const res = await send({ text: 'hi', to: ['claude'] });
  // The kild does not exist, so this must fail on the KILD, never on `to`.
  expect(res.status).not.toBe(400);
  expect(await res.text()).not.toContain('to must');
});

test('a kickoff must name its recipients like any other message', async () => {
  expect(await (await newKild({ text: 'go' })).json()).toEqual({
    error: 'kickoff.to must be an array of agent handles',
  });
  expect(await (await newKild({ to: [], text: 'go' })).json()).toEqual({
    error: 'kickoff.to must name at least one agent',
  });
  expect(await (await newKild({ to: ['agent'] })).json()).toEqual({
    error: 'kickoff.text required',
  });
  expect(await (await newKild('go')).json()).toEqual({
    error: 'kickoff must be an object: {to: [handle], text}',
  });
});

test('a well-formed kickoff passes validation and reaches kild creation', async () => {
  const body = await (await newKild({ to: ['agent'], text: 'go' })).text();
  expect(body).not.toContain('kickoff');
});
