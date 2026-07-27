import { beforeAll, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { ensureWorktree } from './kild/worktree.ts';

/**
 * The reshaped kild surface, against the real Hono app (`server.ts`'s default export) via
 * `fetch` — no port is bound (Bun only serves the default export for the entry module), so
 * the operator's engine on 4517 is never touched. KILD_HOME points at a temp dir BEFORE the
 * dynamic import so the module's load-time side effects see empty state, not the user's.
 *
 * What is proved here:
 *  - the parallel `/api/agents/:id/*` family and the whole `/api/worktrees` family are GONE;
 *  - `GET /api/kilds` enumerates `kild/*` worktrees FROM GIT, so a tree whose kild record is
 *    gone is addressable again instead of stranded;
 *  - `POST /api/kilds/:id/agents` answers with a typed error instead of warning to the
 *    engine's own log;
 *  - `DELETE /api/kilds/:id` refuses authored commits, ignores litter, and keeps the branch;
 *  - `GET .../land` changes nothing and `POST .../land` merges and names the sha;
 *  - the listing is SPLIT: `/api/kilds` is identity (no git, no log), `/api/kilds/status` is
 *    the git/cost half, and the log is its own cursored resource at `/api/kilds/:id/messages`.
 */
const execFile = promisify(execFileCb);

let fetchApp: (req: Request) => Response | Promise<Response>;
let repo: string;

/** An archived kild seeded on DISK before the engine loads, so the message-cursor tests can
 *  read a real read-only record. Its log deliberately holds entries with NO `seq` (written by
 *  an engine that predates the cursor) and a `ts` that goes BACKWARDS. */
const ARCHIVED = 'archived-kild-1';
const ARCHIVED_LOG = [
  { id: 'm1', kildId: ARCHIVED, from: 'human', to: ['coder'], text: 'one', ts: 1000 },
  { id: 'm2', kildId: ARCHIVED, from: 'coder', to: ['human'], text: 'two', ts: 500 },
  { id: 'm3', kildId: ARCHIVED, from: 'human', to: ['coder'], text: 'three', ts: 1500 },
];

const git = (...args: string[]) => execFile('git', ['-C', repo, ...args]);
const gitIn = (dir: string, ...args: string[]) => execFile('git', ['-C', dir, ...args]);

const call = (url: string, init?: RequestInit) =>
  Promise.resolve(fetchApp(new Request(`http://localhost${url}`, init)));
const get = (url: string) => call(url);
const post = (url: string, body?: unknown) =>
  call(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body ?? {}),
  });
const del = (url: string) => call(url, { method: 'DELETE' });

/** A `kild/<name>` tree with one commit on it — real, unlanded work. */
async function treeWithCommit(name: string, file = `${name}.ts`) {
  const wt = await ensureWorktree(repo, name, 'main');
  fs.writeFileSync(path.join(wt.path, file), 'the work\n');
  await gitIn(wt.path, 'add', '.');
  await gitIn(wt.path, 'commit', '-q', '-m', `work on ${name}`);
  return wt;
}

/** A `kild/<name>` tree carrying nothing but provisioning litter. */
async function treeWithLitter(name: string) {
  const wt = await ensureWorktree(repo, name, 'main');
  fs.writeFileSync(path.join(wt.path, '.archon.yaml'), 'provisioned: true\n');
  return wt;
}

beforeAll(async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-kilds-'));
  process.env.KILD_HOME = home;
  fs.mkdirSync(path.join(home, 'kilds'), { recursive: true });
  fs.writeFileSync(
    path.join(home, 'kilds', `${ARCHIVED}.json`),
    JSON.stringify({
      id: ARCHIVED,
      name: 'record',
      cwd: os.tmpdir(),
      agents: [{ handle: 'coder' }],
      log: ARCHIVED_LOG,
    }),
  );
  repo = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-repo-'));
  await execFile('git', ['-C', repo, 'init', '-q', '-b', 'main']);
  await git('config', 'user.email', 't@t');
  await git('config', 'user.name', 't');
  fs.writeFileSync(path.join(repo, 'README.md'), 'base\n');
  await git('add', '.');
  await git('commit', '-q', '-m', 'init');

  const server = (await import('./server.ts')).default;
  fetchApp = server.fetch as typeof fetchApp;
  // Registering the project is what puts this repo in the enumeration's scope for the
  // UNSCOPED `GET /api/kilds` — the call helm makes.
  expect((await post('/api/projects', { name: 'proj', path: repo })).status).toBe(200);
});

// ── §1: one address for an agent ─────────────────────────────────────────────────────

test('the parallel /api/agents/:id/* family is gone', async () => {
  expect((await post('/api/agents', { persona: 'default' })).status).toBe(404);
  expect((await post('/api/agents/some-id/prompt', { text: 'hi' })).status).toBe(404);
  expect((await post('/api/agents/some-id/stop')).status).toBe(404);
  expect((await get('/api/agents/some-id/transcript')).status).toBe(404);
});

test('the bare agent listing survives — it is the only view of agents in no kild', async () => {
  const res = await get('/api/agents');
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual([]);
});

test('an agent transcript is addressed one way: on its kild, by handle', async () => {
  const res = await get('/api/kilds/no-such-kild/agents/coder/transcript');
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: no-such-kild' });
});

// ── §3: the worktree family is folded into the kild collection ───────────────────────

test('the /api/worktrees family is gone', async () => {
  expect((await get(`/api/worktrees?path=${encodeURIComponent(repo)}`)).status).toBe(404);
  expect((await del('/api/worktrees')).status).toBe(404);
  expect((await post('/api/worktrees/prune', { path: repo })).status).toBe(404);
});

test('GET /api/kilds enumerates kild/* worktrees FROM GIT, so an orphan is addressable', async () => {
  await treeWithLitter('stranded');

  const kilds = (await (await get('/api/kilds')).json()) as Array<{
    id: string;
    name: string;
    worktree?: string;
    orphan?: boolean;
    agents: unknown[];
    cwd?: string;
  }>;
  const orphan = kilds.find((kild) => kild.worktree === 'stranded');
  expect(orphan).toBeDefined();
  // Its worktree name IS its id — without that it has no address at all, which is exactly
  // how trees became permanently unreclaimable.
  expect(orphan?.id).toBe('stranded');
  expect(orphan?.orphan).toBe(true);
  expect(orphan?.agents).toEqual([]);
  expect(orphan?.cwd).toBe(repo);
});

// ── §5: the listing is split — cheap identity here, costly git on /status ─────────────

test('GET /api/kilds is the CHEAP half: no git, no log, on any entry', async () => {
  const kilds = (await (await get('/api/kilds')).json()) as Array<Record<string, unknown>>;
  expect(kilds.length).toBeGreaterThan(0);
  for (const kild of kilds) {
    // The two unbounded costs: a git status batch per kild, and the whole conversation.
    expect(kild).not.toHaveProperty('git');
    expect(kild).not.toHaveProperty('log');
    expect(kild).not.toHaveProperty('totals');
    expect(Object.keys(kild).sort()).toEqual(
      expect.arrayContaining(['agents', 'cwd', 'id', 'name']),
    );
  }
});

test('GET /api/kilds/status is the COSTLY half: git per kild, still no log', async () => {
  const kilds = (await (await get('/api/kilds/status')).json()) as Array<{
    worktree?: string;
    git?: { branch: string | null; ahead: number; changedFiles: string[] };
    log?: unknown;
  }>;
  const orphan = kilds.find((kild) => kild.worktree === 'stranded');
  expect(orphan?.git?.branch).toBe('kild/stranded');
  expect(orphan?.git?.changedFiles).toEqual([]);
  // The split is git-vs-identity, not git-vs-everything: the log left BOTH payloads.
  for (const kild of kilds) expect(kild).not.toHaveProperty('log');
});

test('GET /api/kilds/:id is one kild with its git and WITHOUT its log', async () => {
  const res = await get('/api/kilds/stranded');
  expect(res.status).toBe(200);
  const body = (await res.json()) as Record<string, unknown>;
  expect(body).toMatchObject({ id: 'stranded', worktree: 'stranded', orphan: true });
  expect((body.git as { branch: string }).branch).toBe('kild/stranded');
  expect(body).not.toHaveProperty('log');
  expect((await get('/api/kilds/never-existed')).status).toBe(404);
});

test('?state= filters both halves, and reclaimable is a GIT question so it lives on /status', async () => {
  await treeWithCommit('filtered');
  for (const route of ['/api/kilds', '/api/kilds/status']) {
    const orphans = (await (await get(`${route}?state=orphan`)).json()) as Array<{
      worktree?: string;
      orphan?: boolean;
    }>;
    expect(orphans.length).toBeGreaterThan(0);
    expect(orphans.every((kild) => kild.orphan === true)).toBe(true);
    expect(orphans.some((kild) => kild.worktree === 'filtered')).toBe(true);
    // No live kilds in this engine, so the live half is empty.
    expect(await (await get(`${route}?state=live`)).json()).toEqual([]);

    const bad = await get(`${route}?state=nonsense`);
    expect(bad.status).toBe(400);
    expect(((await bad.json()) as { error: string }).error).toContain('unknown state');
  }

  // prune-as-a-filter reports only what disposal would actually accept: `filtered` has
  // commits, so it is out. It reads `ahead`, so it is only answerable where git ran.
  const reclaimable = (await (await get('/api/kilds/status?state=reclaimable')).json()) as Array<{
    worktree?: string;
  }>;
  expect(reclaimable.some((kild) => kild.worktree === 'stranded')).toBe(true);
  expect(reclaimable.some((kild) => kild.worktree === 'filtered')).toBe(false);

  // On the cheap route it is refused outright rather than silently making it expensive.
  const onCheap = await get('/api/kilds?state=reclaimable');
  expect(onCheap.status).toBe(400);
  expect(((await onCheap.json()) as { error: string }).error).toContain('/api/kilds/status');
});

test('both halves can be scoped to one checkout', async () => {
  const other = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-other-'));
  for (const route of ['/api/kilds', '/api/kilds/status']) {
    expect((await get(`${route}?path=${encodeURIComponent(other)}`)).status).toBe(200);
    expect(await (await get(`${route}?path=${encodeURIComponent(other)}`)).json()).toEqual([]);
    const scoped = (await (
      await get(`${route}?path=${encodeURIComponent(repo)}`)
    ).json()) as unknown[];
    expect(scoped.length).toBeGreaterThan(0);
  }
  fs.rmSync(other, { recursive: true, force: true });
});

// ── §5: the log is its own resource, cursored by seq ─────────────────────────────────

const logOf = async (url: string) =>
  (await (await get(url)).json()) as Array<{ seq: number; text: string; ts: number }>;

test('an ARCHIVED kild serves its log — it is the read-only record', async () => {
  const res = await get(`/api/kilds/${ARCHIVED}/messages`);
  expect(res.status).toBe(200);
  const log = await res.json();
  expect((log as Array<{ text: string }>).map((m) => m.text)).toEqual(['one', 'two', 'three']);
  // Stored order IS arrival order, so a log written before `seq` still reads back cursored.
  expect((log as Array<{ seq: number }>).map((m) => m.seq)).toEqual([1, 2, 3]);
  // …and the cursor is strictly increasing exactly where `ts` is not.
  expect((log as Array<{ ts: number }>).map((m) => m.ts)).toEqual([1000, 500, 1500]);
});

test('?since= is an EXCLUSIVE cursor: only what arrived after that seq', async () => {
  expect((await logOf(`/api/kilds/${ARCHIVED}/messages?since=1`)).map((m) => m.text)).toEqual([
    'two',
    'three',
  ]);
  expect((await logOf(`/api/kilds/${ARCHIVED}/messages?since=2`)).map((m) => m.seq)).toEqual([3]);
  // Caught up: the same cursor keeps answering "nothing new", never replaying the log.
  expect(await logOf(`/api/kilds/${ARCHIVED}/messages?since=3`)).toEqual([]);
  expect(await logOf(`/api/kilds/${ARCHIVED}/messages?since=99`)).toEqual([]);
  // seqs start at 1, so since=0 is the whole log and a fresh client skips nothing.
  expect((await logOf(`/api/kilds/${ARCHIVED}/messages?since=0`)).map((m) => m.seq)).toEqual([
    1, 2, 3,
  ]);
});

test('a malformed cursor is a client error, never a silent full replay', async () => {
  for (const bad of ['abc', '-1', '1.5', '']) {
    const res = await get(`/api/kilds/${ARCHIVED}/messages?since=${bad}`);
    expect(res.status).toBe(400);
    expect(((await res.json()) as { error: string }).error).toContain('since must be');
  }
});

test('messages on an id that is no kild at all is a clean 404', async () => {
  const res = await get('/api/kilds/never-existed/messages');
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: never-existed' });
});

// ── §2: spawning answers ─────────────────────────────────────────────────────────────

test('POST /api/kilds/:id/agents returns a REAL error instead of a silent warning', async () => {
  const res = await post('/api/kilds/no-such-kild/agents', { handle: 'reviewer' });
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: no-such-kild', code: 'not_found' });
});

test('POST /api/kilds/:id/agents validates its body before touching a kild', async () => {
  expect(await (await post('/api/kilds/k/agents', {})).json()).toEqual({
    error: 'handle required',
  });
  expect(await (await post('/api/kilds/k/agents', { handle: '  ' })).json()).toEqual({
    error: 'handle required',
  });
  expect(await (await post('/api/kilds/k/agents', { handle: 'a', model: 7 })).json()).toEqual({
    error: 'model must be a string',
  });
});

test('DELETE /api/kilds/:id/agents/:handle is the stop verb for one agent', async () => {
  const res = await del('/api/kilds/no-such-kild/agents/coder');
  expect(res.status).toBe(404);
  expect(await res.json()).toEqual({ error: 'no such kild: no-such-kild', code: 'not_found' });
});

// ── §2: disposal, guarded on authored commits ────────────────────────────────────────

test('DELETE refuses a branch carrying commits, and leaves the tree exactly where it was', async () => {
  const wt = await treeWithCommit('unlanded');
  const res = await del('/api/kilds/unlanded');
  expect(res.status).toBe(409);
  const body = (await res.json()) as { error: string; code: string; commits: number; tip: string };
  expect(body.code).toBe('authored');
  expect(body.commits).toBe(1);
  expect(body.tip).toMatch(/^[0-9a-f]{7}$/);
  expect(body.error).toContain('land it');
  expect(fs.existsSync(wt.path)).toBe(true);
});

test('DELETE succeeds on a tree carrying only litter, names it, and KEEPS the branch', async () => {
  const wt = await treeWithLitter('litter-only');
  const res = await del('/api/kilds/litter-only');
  expect(res.status).toBe(200);
  const body = (await res.json()) as {
    ok: true;
    branch: string;
    branchKept: boolean;
    discarded: string[];
    forced: boolean;
    message: string;
  };
  expect(body).toMatchObject({
    ok: true,
    branch: 'kild/litter-only',
    branchKept: true,
    discarded: ['.archon.yaml'],
    forced: false,
  });
  expect(body.message).toContain('Branch kild/litter-only kept.');
  expect(fs.existsSync(wt.path)).toBe(false);
  // The branch is the safety net, and it is still there.
  expect((await git('branch')).stdout).toContain('kild/litter-only');
});

test('DELETE ?force=true reclaims a tree with commits — the branch still holds them', async () => {
  const wt = await treeWithCommit('forced-away');
  const sha = (await gitIn(wt.path, 'rev-parse', 'HEAD')).stdout.trim();
  const res = await del('/api/kilds/forced-away?force=true');
  expect(res.status).toBe(200);
  expect(await res.json()).toMatchObject({ ok: true, forced: true, branchKept: true });
  expect(fs.existsSync(wt.path)).toBe(false);
  expect((await git('rev-parse', 'kild/forced-away')).stdout.trim()).toBe(sha);
});

test('DELETE on an unknown id is a clean 404, and a bad force value a 400', async () => {
  const missing = await del('/api/kilds/never-existed');
  expect(missing.status).toBe(404);
  expect(await missing.json()).toEqual({ error: 'no such kild: never-existed', code: 'not_found' });
  expect((await del('/api/kilds/x?force=maybe')).status).toBe(400);
});

// ── land: GET is a dry run, POST performs it ──────────────────────────────────────────

test('GET .../land is a dry run that changes nothing; POST merges and names the sha', async () => {
  const wt = await treeWithCommit('landable');
  const before = {
    head: (await git('rev-parse', 'HEAD')).stdout.trim(),
    refs: (await git('show-ref')).stdout,
    status: (await git('status', '--porcelain')).stdout,
  };

  const dry = (await (await get('/api/kilds/landable/land')).json()) as {
    dryRun: boolean;
    wouldMerge: boolean;
    merged: boolean;
    sha?: string;
    branch: string;
    commits: unknown[];
  };
  expect(dry).toMatchObject({
    dryRun: true,
    wouldMerge: true,
    merged: false,
    branch: 'kild/landable',
  });
  expect(dry.sha).toBeUndefined();
  expect(dry.commits).toHaveLength(1);
  expect({
    head: (await git('rev-parse', 'HEAD')).stdout.trim(),
    refs: (await git('show-ref')).stdout,
    status: (await git('status', '--porcelain')).stdout,
  }).toEqual(before);

  const landed = await post('/api/kilds/landable/land');
  expect(landed.status).toBe(200);
  const body = (await landed.json()) as { merged: boolean; sha: string; dryRun: boolean };
  expect(body.merged).toBe(true);
  expect(body.dryRun).toBe(false);
  expect(body.sha).toMatch(/^[0-9a-f]{40}$/);
  expect((await git('rev-parse', 'HEAD')).stdout.trim()).toBe(body.sha);
  expect(fs.existsSync(path.join(wt.path, 'landable.ts'))).toBe(true); // the tree is untouched
});

test('a land that did not happen is a 409, never a 200 with merged:false', async () => {
  await ensureWorktree(repo, 'nothing-to-land', 'main');
  const res = await post('/api/kilds/nothing-to-land/land');
  expect(res.status).toBe(409);
  const body = (await res.json()) as { merged: boolean; error: string };
  expect(body.merged).toBe(false);
  expect(body.error).toContain('nothing committed');
});

test('land on an unknown kild is a clean 404', async () => {
  expect((await get('/api/kilds/never-existed/land')).status).toBe(404);
  expect((await post('/api/kilds/never-existed/land')).status).toBe(404);
});

// ── A bad request body is never a 500 ────────────────────────────────────────────────────
// A missing or malformed body is a CLIENT mistake, so it must produce a 400 naming what is
// wrong. Reading the body unguarded threw instead, and Hono surfaced that as "Internal
// Server Error" — the server blaming itself for a bad request.
//
// The sharpest case is a verb that requires nothing. `POST /api/kilds/:id/stop` takes no
// required field, so a request with NO body is valid input — and it 500'd. A caller with
// nothing to say, doing the obvious thing, got a server error. Found by the end-to-end
// migration test, which called stop with no body for exactly that reason.

/** Post with no body at all, and with a malformed one — the two ways a caller gets it wrong. */
const postRaw = (url: string, body?: string) =>
  call(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    ...(body === undefined ? {} : { body }),
  });

const BODY_ROUTES = [
  '/api/projects',
  '/api/open',
  '/api/open-url',
  '/api/kilds',
  '/api/kilds/some-id/messages',
  '/api/kilds/some-id/agents',
  '/api/kilds/some-id/agents/attach',
  '/api/kilds/some-id/inbox/drain',
  '/api/kilds/some-id/stop',
];

for (const route of BODY_ROUTES) {
  test(`POST ${route} never 500s on an absent or malformed body`, async () => {
    expect((await postRaw(route)).status).toBeLessThan(500);
    expect((await postRaw(route, '{not json')).status).toBeLessThan(500);
  });
}

test('stop requires nothing, so an empty body reaches the handler', async () => {
  // 404, not 400 and certainly not 500: the body was fine, the kild just does not exist.
  // That distinction is the whole point.
  const res = await postRaw('/api/kilds/definitely-not-a-kild/stop');
  expect(res.status).toBe(404);
  expect((await res.json()) as { error: string }).toMatchObject({
    error: 'no such kild: definitely-not-a-kild',
  });
});

test('a required field still fails loudly, naming itself', async () => {
  const messages = await post('/api/kilds/some-id/messages', {});
  expect(messages.status).toBe(400);
  expect((await messages.json()) as { error: string }).toMatchObject({ error: 'text required' });

  const projects = await post('/api/projects', {});
  expect(projects.status).toBe(400);
  expect((await projects.json()) as { error: string }).toMatchObject({
    error: 'name and path are both required',
  });

  expect((await post('/api/kilds/some-id/agents/attach', {})).status).toBe(400);
});
