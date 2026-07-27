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
 *  - `GET .../land` changes nothing and `POST .../land` merges and names the sha.
 */
const execFile = promisify(execFileCb);

let fetchApp: (req: Request) => Response | Promise<Response>;
let repo: string;

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
  process.env.KILD_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-kilds-'));
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
    log: unknown[];
    cwd?: string;
    git?: { branch: string | null; ahead: number };
  }>;
  const orphan = kilds.find((kild) => kild.worktree === 'stranded');
  expect(orphan).toBeDefined();
  // Its worktree name IS its id — without that it has no address at all, which is exactly
  // how trees became permanently unreclaimable.
  expect(orphan?.id).toBe('stranded');
  expect(orphan?.orphan).toBe(true);
  expect(orphan?.agents).toEqual([]);
  expect(orphan?.log).toEqual([]);
  expect(orphan?.cwd).toBe(repo);
  expect(orphan?.git?.branch).toBe('kild/stranded');
});

test('?state= filters the collection, and an unknown state is a client error', async () => {
  await treeWithCommit('filtered');
  const orphans = (await (await get('/api/kilds?state=orphan')).json()) as Array<{
    worktree?: string;
    orphan?: boolean;
  }>;
  expect(orphans.length).toBeGreaterThan(0);
  expect(orphans.every((kild) => kild.orphan === true)).toBe(true);
  expect(orphans.some((kild) => kild.worktree === 'filtered')).toBe(true);

  // No live kilds in this engine, so the live half is empty — prune-as-a-filter meanwhile
  // reports only what disposal would actually accept: `filtered` has commits, so it is out.
  expect(await (await get('/api/kilds?state=live')).json()).toEqual([]);
  const reclaimable = (await (await get('/api/kilds?state=reclaimable')).json()) as Array<{
    worktree?: string;
  }>;
  expect(reclaimable.some((kild) => kild.worktree === 'stranded')).toBe(true);
  expect(reclaimable.some((kild) => kild.worktree === 'filtered')).toBe(false);

  const bad = await get('/api/kilds?state=nonsense');
  expect(bad.status).toBe(400);
  expect(((await bad.json()) as { error: string }).error).toContain('unknown state');
});

test('the collection can be scoped to one checkout', async () => {
  const other = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-server-other-'));
  expect((await get(`/api/kilds?path=${encodeURIComponent(other)}`)).status).toBe(200);
  expect(await (await get(`/api/kilds?path=${encodeURIComponent(other)}`)).json()).toEqual([]);
  const scoped = (await (
    await get(`/api/kilds?path=${encodeURIComponent(repo)}`)
  ).json()) as unknown[];
  expect(scoped.length).toBeGreaterThan(0);
  fs.rmSync(other, { recursive: true, force: true });
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
