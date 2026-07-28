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

const message = (url: string, body: unknown) =>
  fetchApp(
    new Request(`http://localhost${url}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    }),
  );

test.each(['agents/attach', 'inbox/drain'])('POST %s without a handle is a 400', async (verb) => {
  const res = await message(`/api/kilds/kild-1/${verb}`, {});
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'handle required' });
});

test.each([
  'agents/attach',
  'inbox/drain',
])('POST %s with a blank handle is a 400', async (verb) => {
  const res = await message(`/api/kilds/kild-1/${verb}`, { handle: '   ' });
  expect(res.status).toBe(400);
});

test.each([
  'agents/attach',
  'inbox/drain',
])('POST %s on an unknown kild is a clean 404', async (verb) => {
  const res = await message(`/api/kilds/no-such-kild/${verb}`, { handle: 'claude' });
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: no-such-kild' });
});

test('drain is not exposed as a GET — a destructive read must not be retryable', async () => {
  const res = await fetchApp(new Request('http://localhost/api/kilds/kild-1/inbox/drain'));
  expect(res.status).toBe(404); // no route matches the GET
});

// ── The attach credential, as the wire presents it ────────────────────────────
// `Authorization: Bearer <token>` is how an attached harness names itself when it sends.
// The token itself is minted against a LIVE kild (covered in kild-manager.test.ts); what
// matters here is that the header is read, and what happens when it is wrong or absent.

const sendWithAuth = (authorization?: string) =>
  fetchApp(
    new Request('http://localhost/api/kilds/kild-1/messages', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(authorization === undefined ? {} : { authorization }),
      },
      body: JSON.stringify({ text: 'hi', to: ['coder'] }),
    }),
  );

test('a bogus bearer token is rejected, not silently downgraded to the label', async () => {
  const res = await sendWithAuth('Bearer definitely-not-minted');
  // 409, and it never reaches the kild lookup: the caller presented a credential and is
  // told it means nothing, rather than having someone else's name written on its message.
  expect(res.status).toBe(409);
  expect(await res.json()).toEqual({ error: 'unknown attach token' });
});

test('no Authorization header keeps the old path exactly — the CLI and curl are unchanged', async () => {
  const res = await sendWithAuth();
  // Attribution succeeded (the unattributed label) and the request failed on the KILD,
  // which is the pre-token behaviour to the byte.
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: kild-1' });
});

test('an Authorization header that is not a bearer token is no credential at all', async () => {
  for (const header of ['Basic dXNlcjpwYXNz', 'Bearer', 'Bearer    ']) {
    const res = await sendWithAuth(header);
    expect(res.status).toBe(404); // fell through to the unattributed label
  }
});
