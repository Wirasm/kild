// One binary, two roles: `KILD_ROLE=agent` runs a single agent session (one per
// process — the coding-agent SDK requires it); otherwise this is the engine.
if (process.env.KILD_ROLE === 'agent') {
  await (await import('./agent.ts')).runAgent();
}

import { execFile as execFileCb } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

import { type Context, Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';
import { agentManager } from './kild/agent-manager.ts';
import { reviewCommits, reviewDiff, reviewFiles } from './kild/git-review.ts';
import { assessDisposal, removeKildTree } from './kild/kild-disposal.ts';
import { landMerge, landPlan } from './kild/kild-land.ts';
import { kildManager } from './kild/kild-manager.ts';
import { type KildTree, kildTrees, orphanTrees } from './kild/kild-trees.ts';
import type { AgentSpec, CommandResult, KildIdentity, KildStatus } from './kild/kild-types.ts';
import { listPersonas } from './kild/personas.ts';
import { addProject, findProject, loadProjects } from './kild/projects.ts';
import {
  resolveNewKildActor,
  resolveSendActor,
  resolveStopActor,
  UNATTRIBUTED,
} from './kild/rest-attribution.ts';
import { readSessionTranscript } from './kild/session-transcript.ts';
import { pruneMergedWorktrees, worktreesRoot } from './kild/worktree.ts';
import { kildGitStatus } from './kild/worktree-status.ts';

const execFile = promisify(execFileCb);
const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

const PORT = Number(process.env.KILD_PORT ?? 4517);
// Bind to loopback only: the engine holds the user's OAuth and runs bash in their
// repos, so it must never be reachable from the LAN.
const HOST = process.env.KILD_HOST ?? '127.0.0.1';

// Only the known UI clients' own origins may drive the engine from a browser context.
// A request with no Origin is a non-browser client (the CLI / curl); browsers
// always send one, so an unexpected Origin is a hostile web page — rejected.
const ALLOWED_ORIGINS = new Set([
  'http://localhost:1420',
  'http://127.0.0.1:1420',
  'tauri://localhost',
  'https://tauri.localhost',
]);
const originAllowed = (origin: string | undefined): boolean =>
  origin == null || ALLOWED_ORIGINS.has(origin);

const { upgradeWebSocket, websocket } = createBunWebSocket();

const app = new Hono();
app.use('/*', cors({ origin: (origin) => (origin && ALLOWED_ORIGINS.has(origin) ? origin : '') }));

// `bootId` is unique per engine process — external WS clients (e.g. the pi extension)
// compare it to detect a restart and force-reconnect their socket, since a killed engine
// doesn't reliably surface a WS close to a read-only client.
const BOOT_ID = randomUUID();
app.get('/api/health', (c) => c.json({ ok: true, name: 'kild-engine', bootId: BOOT_ID }));

// ── Projects ────────────────────────────────────────────────────────────────
app.get('/api/projects', async (c) => c.json(await loadProjects()));
app.post('/api/projects', async (c) => {
  const { name, path } = (await jsonBody(c)) as { name?: string; path?: string };
  if (typeof name !== 'string' || typeof path !== 'string') {
    return c.json({ error: 'name and path are both required' }, 400);
  }
  try {
    return c.json(await addProject(name, path));
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Project references ────────────────────────────────────────────────────────
// Endpoints that need a project directory take an EXPLICIT reference: `project` is a
// registered project name, `path` an absolute directory. Exactly one of the two — the
// old polymorphic name-or-path guessing is gone.
type ProjectRef = { ok: true; dir: string } | { ok: false; error: string; status: 400 | 404 };

async function resolveProjectRef(
  project: string | undefined,
  dirPath: string | undefined,
): Promise<ProjectRef> {
  if (project !== undefined && dirPath !== undefined) {
    return {
      ok: false,
      error: 'pass either project (registered name) or path, not both',
      status: 400,
    };
  }
  if (project !== undefined) {
    const found = await findProject(project);
    if (!found) return { ok: false, error: `unknown project: ${project}`, status: 404 };
    return { ok: true, dir: found.path };
  }
  if (dirPath !== undefined) {
    if (!path.isAbsolute(dirPath)) {
      return { ok: false, error: `path must be absolute: ${dirPath}`, status: 400 };
    }
    return { ok: true, dir: dirPath };
  }
  return { ok: false, error: 'project (registered name) or path (absolute) required', status: 400 };
}

// ── Personas ──────────────────────────────────────────────────────────────────
// Reusable persona definitions, read from the convention dirs (`.pi/agents` etc. — the
// on-disk dir names are upstream pi convention and unrelated to this route's name).
app.get('/api/personas', async (c) => {
  const ref = await resolveProjectRef(c.req.query('project'), c.req.query('path'));
  // Personas without a project are valid (global-only discovery) — only a BAD ref errors.
  if (!ref.ok && (c.req.query('project') !== undefined || c.req.query('path') !== undefined)) {
    return c.json({ error: ref.error }, ref.status);
  }
  return c.json(await listPersonas(ref.ok ? ref.dir : undefined));
});

/** Read a JSON body, treating an absent or malformed one as `{}`.
 *
 * Every handler validates the fields it requires, so an empty object yields the proper 400
 * naming what is missing. Reading the body unguarded threw instead, which Hono surfaced as
 * a 500 — the server reporting its own failure for what is a client mistake. Worse for a
 * verb whose fields are all optional (`stop`): there an empty body is valid input, and it
 * 500'd. Found by the end-to-end migration test. */
async function jsonBody(c: Context): Promise<Record<string, unknown>> {
  const body = await c.req.json().catch(() => ({}));
  return typeof body === 'object' && body !== null ? (body as Record<string, unknown>) : {};
}

/** The bearer credential a request presents, if any — minted by `attach` and resolved to
 *  the handle that attached (see `rest-attribution.ts`). Absent, blank or another scheme
 *  means "no credential", which is the CLI, curl and helm: they are attributed exactly as
 *  before. A present-but-unknown Bearer token is a different thing entirely and is
 *  rejected downstream rather than downgraded. */
function bearerToken(c: Context): string | undefined {
  const header = c.req.header('authorization');
  const token = /^bearer\s+(\S+)$/i.exec(header?.trim() ?? '')?.[1];
  return token || undefined;
}

function agentSpecs(input: unknown): AgentSpec[] | null {
  if (!Array.isArray(input)) return null;
  const agents: AgentSpec[] = [];
  for (const item of input) {
    if (typeof item !== 'object' || item === null) return null;
    const agent = item as Record<string, unknown>;
    if (typeof agent.handle !== 'string') return null;
    if (agent.persona !== undefined && typeof agent.persona !== 'string') return null;
    if (agent.model !== undefined && typeof agent.model !== 'string') return null;
    agents.push({
      handle: agent.handle,
      persona: typeof agent.persona === 'string' ? agent.persona : undefined,
      model: typeof agent.model === 'string' ? agent.model : undefined,
    });
  }
  return agents;
}

/** A `to` payload: an array of handles naming at least one recipient. The engine never
 *  defaults a recipient, so absent/empty is a client error, never "everyone" or "the
 *  first agent". */
function recipients(input: unknown): { ok: true; to: string[] } | { ok: false; error: string } {
  if (!Array.isArray(input) || input.some((handle) => typeof handle !== 'string')) {
    return { ok: false, error: 'to must be an array of agent handles' };
  }
  if (input.length === 0) return { ok: false, error: 'to must name at least one agent' };
  return { ok: true, to: input as string[] };
}

/** `kickoff: {to, text}` — the first message into a new kild, addressed like any other. */
function kickoffSpec(
  input: unknown,
): { ok: true; to: string[]; text: string } | { ok: false; error: string } {
  if (typeof input !== 'object' || input === null) {
    return { ok: false, error: 'kickoff must be an object: {to: [handle], text}' };
  }
  const { to, text } = input as { to?: unknown; text?: unknown };
  if (typeof text !== 'string' || !text.trim()) {
    return { ok: false, error: 'kickoff.text required' };
  }
  const parsed = recipients(to);
  if (!parsed.ok) return { ok: false, error: `kickoff.${parsed.error}` };
  return { ok: true, to: parsed.to, text };
}

function envRecord(input: unknown): Record<string, string> | undefined {
  if (input === undefined) return undefined;
  if (typeof input !== 'object' || input === null) return undefined;
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(input)) {
    if (typeof value !== 'string') return undefined;
    env[key] = value;
  }
  return env;
}

function kildResultStatus(result: Extract<CommandResult<unknown>, { ok: false }>): 404 | 409 {
  return result.code === 'not_found' ? 404 : 409;
}

const worktreesInUse = (): Set<string> =>
  new Set(
    agentManager
      .list()
      .map((s) => s.worktree)
      .filter((w): w is string => typeof w === 'string'),
  );

// ── Open in OS ────────────────────────────────────────────────────────────────
// Reveal a worktree path in the OS file browser. Only paths under the worktree root
// are allowed — the engine is loopback-only but must never shell `open` on an
// arbitrary path. Keeps UI clients pure-web (no Tauri opener API needed).
app.post('/api/open', async (c) => {
  const { path: target } = (await jsonBody(c)) as { path?: string };
  const root = worktreesRoot();
  const resolved = path.resolve(target ?? '');
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    return c.json({ error: 'path is not under the worktree root' }, 403);
  }
  try {
    const opener = process.platform === 'darwin' ? 'open' : 'xdg-open';
    await execFile(opener, [resolved]);
    return c.json({ ok: true });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// Open an external http(s) URL in the OS browser. UI clients route rendered links
// here so a click never navigates their webview away from the app. Restricted to
// http/https — never file://, app schemes, etc. execFile (no shell) → no injection.
app.post('/api/open-url', async (c) => {
  const { url } = (await jsonBody(c)) as { url?: string };
  let parsed: URL;
  try {
    parsed = new URL(url ?? '');
  } catch {
    return c.json({ error: 'invalid url' }, 400);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    return c.json({ error: 'only http(s) urls may be opened' }, 403);
  }
  try {
    const opener = process.platform === 'darwin' ? 'open' : 'xdg-open';
    await execFile(opener, [parsed.toString()]);
    return c.json({ ok: true });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Transcripts ───────────────────────────────────────────────────────────────
// Compact conversation readback from a pi session file (see session-transcript.ts).
// `tail` bounds the entry count; invalid values are a client error, not a default.
async function serveTranscript(
  c: Context,
  piSessionFile: string | undefined,
  who: string,
): Promise<Response> {
  if (!piSessionFile) {
    return c.json({ error: `${who} has no pi session file (yet)` }, 404);
  }
  const rawTail = c.req.query('tail');
  const tail = rawTail === undefined ? undefined : Number(rawTail);
  if (tail !== undefined && (!Number.isInteger(tail) || tail < 1)) {
    return c.json({ error: 'tail must be a positive integer' }, 400);
  }
  try {
    return c.json(await readSessionTranscript(piSessionFile, tail));
  } catch (err) {
    // The handle is persisted but the file may be gone (pi's dir cleaned up).
    return c.json({ error: `transcript unreadable: ${errText(err)}` }, 404);
  }
}

// An agent's transcript — works for LIVE and ARCHIVED kilds alike: the piSessionFile
// handle persists in $KILD_HOME/kilds/<id>.json past the session's death.
app.get('/api/kilds/:id/agents/:handle/transcript', async (c) => {
  const id = c.req.param('id');
  const handle = c.req.param('handle');
  const kild =
    kildManager.liveKilds().find((k) => k.id === id) ??
    kildManager.archived().find((k) => k.id === id);
  if (!kild) return c.json({ error: `no such kild: ${id}` }, 404);
  const agent = kild.agents.find((a) => a.handle === handle);
  if (!agent) return c.json({ error: `no such agent: @${handle}` }, 404);
  return serveTranscript(c, agent.piSessionFile, `agent @${handle}`);
});

// ── Agents ────────────────────────────────────────────────────────────────────
// An agent inside a kild is addressed ONE way: `/api/kilds/:id/agents/:handle`. The
// parallel `/api/agents/:id/*` family is gone — two identifier schemes for one object is
// the shape this reshape exists to delete, and prompting an owned agent was never a second
// verb: it is `POST /api/kilds/:id/messages`, the one delivery path.
//
// This bare listing survives because it answers a question no kild can: the live agent
// PROCESSES that belong to no kild (one-shot `kild run`, detached spawns) are on no
// roster, so `/api/kilds/:id/agents` cannot see them. It is a process inventory, not an
// addressable resource family.
app.get('/api/agents', (c) => c.json(agentManager.list()));

// ── Kilds ─────────────────────────────────────────────────────────────────────
// Past kilds recovered from disk (read-only history). Live kilds flow over the WS
// (`{kilds}` summaries + `{message}` sends); this is the conversation record of
// kilds from previous engine runs — their agent subprocesses are long gone.
app.get('/api/kilds/archive', (c) => c.json(kildManager.archived()));

// ── The kild collection ───────────────────────────────────────────────────────
// A kild IS a worktree, so there is ONE resource family for it. `/api/worktrees` is gone:
// listing is this collection, disposal is `DELETE /api/kilds/:id`, and prune is a filter
// over the collection (`?state=reclaimable`) rather than a verb of its own.

/** Every repo that might hold a `kild/*` worktree: the registered projects, plus the cwd
 *  of every live kild (a kild can be opened on an unregistered path). */
async function kildRepos(): Promise<string[]> {
  const projects = await loadProjects();
  return [...projects.map((p) => p.path), ...kildManager.liveCwds()];
}

/** The `?project=`/`?path=` scope of a collection request: one repo, or every known one. */
async function kildScope(
  c: Context,
): Promise<{ ok: true; scope?: string } | { ok: false; error: string; status: 400 | 404 }> {
  const project = c.req.query('project');
  const dirPath = c.req.query('path');
  if (project === undefined && dirPath === undefined) return { ok: true };
  const ref = await resolveProjectRef(project, dirPath);
  if (!ref.ok) return ref;
  return { ok: true, scope: ref.dir };
}

/** The `kild/*` trees in scope that no live kild is using — the orphan half of the
 *  collection. This is the ONLY git the cheap listing performs: one
 *  `git worktree list --porcelain` per repo, however many kilds those repos hold. */
async function orphansInScope(scope?: string): Promise<KildTree[]> {
  return orphanTrees(
    await kildTrees(scope ? [scope] : await kildRepos()),
    kildManager.liveWorktrees(),
  );
}

/** A tree git reports but the engine does not remember, as a kild: no agents, no log, and
 *  its WORKTREE NAME for an id — without that it has no address at all, which is how trees
 *  became permanently unreclaimable. */
function orphanIdentity(tree: KildTree): KildIdentity {
  return {
    id: tree.worktree,
    name: tree.worktree,
    cwd: tree.repo,
    worktree: tree.worktree,
    orphan: true,
    agents: [],
  };
}

/**
 * The collection: live kilds **unioned with the `kild/*` worktrees git reports**.
 *
 * Enumerating from git is what makes the fold safe. The registry only knows the kilds this
 * engine process created; a tree from an earlier run has no record, therefore no id,
 * therefore nothing could list it or dispose of it — the exact stranding this fixes. A
 * worktree that is not a kild (the main checkout, any hand-made tree on a non-`kild/`
 * branch) is not listed at all: kild does not claim trees it did not create.
 *
 * Identity only, and deliberately: on a machine with 116 kild worktrees the old shape ran a
 * git status batch per kild per request, which made the one query that is *mostly identity*
 * the most expensive call in the API. Git and cost live on `/api/kilds/status`, the log on
 * `/api/kilds/:id/messages`.
 */
async function kildIdentities(scope?: string): Promise<KildIdentity[]> {
  const live = kildManager.liveIdentities();
  const orphans = (await orphansInScope(scope)).map(orphanIdentity);
  return [...(scope ? live.filter((kild) => kild.cwd === scope) : live), ...orphans];
}

/** The same collection with the costly half attached: git state per kild plus cost rollups. */
async function kildStatuses(scope?: string): Promise<KildStatus[]> {
  const live = await kildManager.liveKildsStatus();
  const orphans: KildStatus[] = await Promise.all(
    (await orphansInScope(scope)).map(async (tree) => ({
      ...orphanIdentity(tree),
      git: await kildGitStatus(tree.path),
    })),
  );
  return [...(scope ? live.filter((kild) => kild.cwd === scope) : live), ...orphans];
}

/** Would disposal accept this kild? Same rule as the guard — no commits the base does not
 *  have — read off the git status the status route already computed (`ahead` IS the count of
 *  commits on HEAD that base lacks), so the filter costs nothing on top. The authoritative
 *  check still runs inside DELETE. */
function reclaimable(kild: KildStatus, inUse: Set<string>): boolean {
  if (!kild.worktree || inUse.has(kild.worktree)) return false;
  return kild.git !== undefined && !kild.git.error && kild.git.ahead === 0;
}

/** `?state=live|orphan` — the only filter the cheap route can answer, because `orphan` is
 *  the one thing enumeration already knows. `reclaimable` is a git question (it reads
 *  `ahead`), so it lives on `/api/kilds/status` and says so here rather than quietly making
 *  this route expensive again. */
function filterByState<T extends { orphan?: boolean }>(
  kilds: T[],
  state: string | undefined,
): { ok: true; kilds: T[] } | { ok: false; error: string } {
  if (state === undefined) return { ok: true, kilds };
  if (state === 'live') return { ok: true, kilds: kilds.filter((kild) => !kild.orphan) };
  if (state === 'orphan') return { ok: true, kilds: kilds.filter((kild) => kild.orphan) };
  return { ok: false, error: `unknown state: ${state} (live, orphan)` };
}

// The CHEAP half: identity and structure for every kild — id, name, cwd, worktree, base,
// orphan, and the roster with each agent's handle/ownership/persona/model/idle. No git, no
// logs, no per-kild cost. Poll this.
// `?state=live|orphan` filters; `?project=`/`?path=` scopes to one repo.
app.get('/api/kilds', async (c) => {
  const scoped = await kildScope(c);
  if (!scoped.ok) return c.json({ error: scoped.error }, scoped.status);
  const state = c.req.query('state');
  if (state === 'reclaimable') {
    return c.json(
      { error: 'state=reclaimable needs git — ask GET /api/kilds/status?state=reclaimable' },
      400,
    );
  }
  const filtered = filterByState(await kildIdentities(scoped.scope), state);
  if (!filtered.ok) return c.json({ error: filtered.error }, 400);
  return c.json(filtered.kilds);
});

// The COSTLY half, on its own cadence: each kild's git state (branch, ahead/behind, dirty,
// uncommittedFiles, changedFiles, conflictsWithBase) plus cost rollups and per-agent
// tokens/cost. Still no logs — those are `/api/kilds/:id/messages`.
// `?state=live|orphan|reclaimable` filters; `?project=`/`?path=` scopes to one repo.
app.get('/api/kilds/status', async (c) => {
  const scoped = await kildScope(c);
  if (!scoped.ok) return c.json({ error: scoped.error }, scoped.status);
  const state = c.req.query('state');
  const kilds = await kildStatuses(scoped.scope);
  if (state === 'reclaimable') {
    const inUse = worktreesInUse();
    return c.json(kilds.filter((kild) => reclaimable(kild, inUse)));
  }
  const filtered = filterByState(kilds, state);
  if (!filtered.ok) {
    return c.json({ error: `unknown state: ${state} (live, orphan, reclaimable)` }, 400);
  }
  return c.json(filtered.kilds);
});

// ONE kild, with its git state and cost — and WITHOUT its log, which is its own cursored
// resource. Bounded cost by construction: one kild's git, never every kild's.
app.get('/api/kilds/:id', async (c) => {
  const id = c.req.param('id');
  const live = await kildManager.liveKildStatus(id);
  if (live) return c.json(live);
  const target = await resolveKild(id);
  if (!target.ok) {
    return c.json({ error: target.message, code: target.code }, kildResultStatus(target));
  }
  return c.json({
    ...orphanIdentity({
      worktree: target.value.worktree ?? id,
      branch: `kild/${target.value.worktree ?? id}`,
      path: target.value.dir,
      repo: target.value.repo,
    }),
    git: await kildGitStatus(target.value.dir, target.value.base),
  });
});

// The log as its own resource, cursored by the message `seq`. `?since=<seq>` is EXCLUSIVE:
// pass back the last seq you saw and get only what arrived after it (omit it for the whole
// log). `ts` cannot do this job — it is `Date.now()` and can go backwards.
//
// Works for an ARCHIVED kild too: its log is the whole of what it still is, and a read-only
// record that could not be read would be no record at all.
app.get('/api/kilds/:id/messages', (c) => {
  const rawSince = c.req.query('since');
  let since: number | undefined;
  if (rawSince !== undefined) {
    // A digit run, nothing else: `Number('')` and `Number(' ')` are both 0, which would turn
    // a client's broken cursor into a silent full replay of the log.
    if (!/^\d+$/.test(rawSince)) {
      return c.json({ error: 'since must be a non-negative integer message seq' }, 400);
    }
    since = Number(rawSince);
  }
  const messages = kildManager.messages(c.req.param('id'), since);
  if (!messages) return c.json({ error: `no such kild: ${c.req.param('id')}` }, 404);
  return c.json(messages);
});
app.post('/api/kilds', async (c) => {
  const body = (await jsonBody(c)) as {
    name?: unknown;
    cwd?: unknown;
    project?: unknown;
    worktree?: unknown;
    base?: unknown;
    agents?: unknown;
    kickoff?: unknown;
    from?: unknown;
    openedBy?: unknown;
  };
  if (typeof body.name !== 'string') return c.json({ error: 'name required' }, 400);
  // The kickoff is an ordinary directed message, so it names its recipients like any
  // other: `{to: ["coder"], text: "…"}`. There is no agent it falls through to.
  const kickoff = kickoffSpec(body.kickoff);
  if (!kickoff.ok) return c.json({ error: kickoff.error }, 400);
  if (body.cwd !== undefined && typeof body.cwd !== 'string')
    return c.json({ error: 'cwd must be a string' }, 400);
  if (body.project !== undefined && typeof body.project !== 'string') {
    return c.json({ error: 'project must be a string' }, 400);
  }
  if (body.worktree !== undefined && typeof body.worktree !== 'string') {
    return c.json({ error: 'worktree must be a string' }, 400);
  }
  if (body.base !== undefined && typeof body.base !== 'string') {
    return c.json({ error: 'base must be a string' }, 400);
  }
  if (body.from !== undefined && typeof body.from !== 'string') {
    return c.json({ error: 'from must be a string' }, 400);
  }
  if (body.openedBy !== undefined && typeof body.openedBy !== 'string') {
    return c.json({ error: 'openedBy must be a string' }, 400);
  }
  const agents = agentSpecs(body.agents);
  if (!agents || agents.length === 0) {
    return c.json({ error: 'agents must name at least one agent' }, 400);
  }

  // Where the kild runs: `project` is a registered name, `cwd` an absolute path —
  // explicit, exactly one (same contract as the worktree/persona endpoints).
  let cwd: string;
  if (typeof body.project === 'string') {
    if (typeof body.cwd === 'string') {
      return c.json({ error: 'pass either project (registered name) or cwd, not both' }, 400);
    }
    const found = await findProject(body.project);
    if (!found) return c.json({ error: `unknown project: ${body.project}` }, 404);
    cwd = found.path;
  } else if (typeof body.cwd === 'string') {
    if (!path.isAbsolute(body.cwd)) {
      return c.json({ error: `cwd must be absolute: ${body.cwd}` }, 400);
    }
    // Absolute is not enough: a caller that resolved an unregistered NAME against its
    // own cwd sends a well-formed path to a directory that does not exist. That created
    // a kild whose worktree could not be created and whose agents never spawned —
    // and the caller got a kild id and a success. Refuse here so the failure is the
    // caller's, not a kild that quietly does nothing.
    const dir = await fs.stat(body.cwd).catch(() => null);
    if (!dir?.isDirectory()) {
      return c.json({ error: `cwd is not an existing directory: ${body.cwd}` }, 400);
    }
    cwd = body.cwd;
  } else {
    return c.json({ error: 'cwd (absolute path) or project (registered name) required' }, 400);
  }

  const attribution = resolveNewKildActor(
    {
      from: typeof body.from === 'string' ? body.from : undefined,
      openedBy: body.openedBy,
      token: bearerToken(c),
    },
    kildManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, kildResultStatus(attribution));

  const id = randomUUID();
  const created = await kildManager.create(id, {
    name: body.name,
    cwd,
    agents,
    worktree: body.worktree,
    base: typeof body.base === 'string' ? body.base : undefined,
  });
  if (!created.ok) return c.json({ error: created.message }, kildResultStatus(created));
  const sent = await kildManager.send(id, attribution.value, kickoff.to, kickoff.text);
  if (!sent.ok) {
    await kildManager.stop(id);
    return c.json({ error: sent.message }, kildResultStatus(sent));
  }
  return c.json({ ok: true, id: created.value.kildId, message: created.value.message });
});
// `to` is the same structured recipient list the in-kild `send` tool takes, and it is
// REQUIRED here for the same reason: the engine never infers a recipient. Addressing is
// never parsed from the message text, so `@handle` in `text` remains just text.
app.post('/api/kilds/:id/messages', async (c) => {
  const { text, from, sessionId, to } = (await jsonBody(c)) as {
    text?: unknown;
    from?: unknown;
    sessionId?: unknown;
    to?: unknown;
  };
  if (typeof text !== 'string') return c.json({ error: 'text required' }, 400);
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  if (sessionId !== undefined && typeof sessionId !== 'string')
    return c.json({ error: 'sessionId must be a string' }, 400);
  const addressed = recipients(to);
  if (!addressed.ok) return c.json({ error: addressed.error }, 400);
  const id = c.req.param('id');
  const attribution = resolveSendActor(
    {
      kildId: id,
      from: typeof from === 'string' ? from : undefined,
      sessionId,
      token: bearerToken(c),
    },
    kildManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, kildResultStatus(attribution));
  const result = await kildManager.send(id, attribution.value, addressed.to, text);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
/**
 * Spawn an agent INTO a live kild.
 *
 * This route exists because spawning used to be WS-only and fire-and-forget: the frame was
 * enqueued, a rejection was `console.warn`ed inside the engine, and the caller was told
 * nothing at all. A caller that cannot learn its spawn failed is the silent-failure shape
 * this whole reshape removes. Both transports now call the SAME manager method
 * ({@link kildManager.spawnAgent}); the difference is only that this one answers.
 */
app.post('/api/kilds/:id/agents', async (c) => {
  const body = (await jsonBody(c)) as {
    handle?: unknown;
    persona?: unknown;
    model?: unknown;
    invitedBy?: unknown;
    forkFrom?: unknown;
  };
  if (typeof body.handle !== 'string' || !body.handle.trim()) {
    return c.json({ error: 'handle required' }, 400);
  }
  if (body.persona !== undefined && typeof body.persona !== 'string') {
    return c.json({ error: 'persona must be a string' }, 400);
  }
  if (body.model !== undefined && typeof body.model !== 'string') {
    return c.json({ error: 'model must be a string' }, 400);
  }
  if (body.invitedBy !== undefined && typeof body.invitedBy !== 'string') {
    return c.json({ error: 'invitedBy must be a string' }, 400);
  }
  // `forkFrom` seeds the new agent from a COPY of an existing pi session file — the way to
  // branch off a conversation, including a LIVE one, since the source is never written.
  // Validated here rather than deeper: a path that is not a file is a client mistake, and
  // the agent process would otherwise fail after spawning, which reads as a spawn that
  // half-worked.
  if (body.forkFrom !== undefined) {
    if (typeof body.forkFrom !== 'string' || !body.forkFrom.trim()) {
      return c.json({ error: 'forkFrom must be a pi session file path' }, 400);
    }
    const stat = await fs.stat(body.forkFrom).catch(() => null);
    if (!stat?.isFile()) {
      return c.json({ error: `forkFrom is not an existing session file: ${body.forkFrom}` }, 400);
    }
  }
  const result = await kildManager.spawnAgent(
    c.req.param('id'),
    {
      handle: body.handle.trim(),
      persona: body.persona,
      model: body.model,
      forkFrom: typeof body.forkFrom === 'string' ? body.forkFrom : undefined,
    },
    body.invitedBy,
  );
  if (!result.ok) {
    return c.json({ error: result.message, code: result.code }, kildResultStatus(result));
  }
  return c.json({ ok: true, handle: body.handle.trim(), message: result.value.message });
});

// Stop ONE agent without stopping the kild. The agent stays on the roster (marked
// `stopped`) — a handle never rebinds, so its transcript stays addressable.
app.delete('/api/kilds/:id/agents/:handle', (c) => {
  const result = kildManager.stopAgent(c.req.param('id'), c.req.param('handle'));
  if (!result.ok) {
    return c.json({ error: result.message, code: result.code }, kildResultStatus(result));
  }
  return c.json({ ok: true, message: result.value.message });
});

// The attached half of the roster: a harness kild does NOT own (a Claude Code session the
// human is driving) claims a `@handle` and gets an inbox. Idempotent by handle, so a hook
// or shell alias can call it on every session start.
//
// It also gets a `token`: send it back as `Authorization: Bearer <token>` and this handle is
// what the engine writes as `from`. Without it an attached sender has no kild session to be
// recognised by and reads as the unattributed label — so two attached agents in one kild
// were indistinguishable on the log. Re-attaching returns the same token (see AttachTokens).
app.post('/api/kilds/:id/agents/attach', async (c) => {
  const { handle } = (await jsonBody(c)) as { handle?: unknown };
  if (typeof handle !== 'string' || !handle.trim())
    return c.json({ error: 'handle required' }, 400);
  const result = await kildManager.attach(c.req.param('id'), handle);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message, token: result.value.token });
});
// The destructive read of that inbox, and with it the agent's idle signal (an empty
// drain = idle). POST, never GET: this MUTATES, so a caching proxy or a retry on a
// GET would silently eat somebody's messages.
app.post('/api/kilds/:id/inbox/drain', async (c) => {
  const { handle } = (await jsonBody(c)) as { handle?: unknown };
  if (typeof handle !== 'string' || !handle.trim())
    return c.json({ error: 'handle required' }, 400);
  const result = kildManager.drain(c.req.param('id'), handle);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, ...result.value });
});
app.post('/api/kilds/:id/stop', async (c) => {
  const { from, sessionId } = (await jsonBody(c)) as {
    from?: unknown;
    sessionId?: unknown;
  };
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  if (sessionId !== undefined && typeof sessionId !== 'string')
    return c.json({ error: 'sessionId must be a string' }, 400);
  const attribution = resolveStopActor(
    {
      kildId: c.req.param('id'),
      from: typeof from === 'string' ? from : undefined,
      sessionId,
      token: bearerToken(c),
    },
    kildManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, kildResultStatus(attribution));
  const result = await kildManager.stop(c.req.param('id'));
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});

// ── Addressing one kild: a live record, or a tree git reports ──────────────────
/** One kild, whether the engine remembers it or only git does. */
interface KildTarget {
  id: string;
  name: string;
  /** True when a live kild record backs this tree. */
  live: boolean;
  /** The main checkout — where a land merges and where the worktree is registered. */
  repo: string;
  /** The kild's effective dir (its worktree, else its cwd). */
  dir: string;
  worktree?: string;
  base?: string;
}

/** Resolve `:id` to a kild. ONE address per kild: a live kild's id, or — for a tree whose
 *  record is gone — its worktree name. An archived id resolves to neither on purpose; the
 *  error says how to address the tree instead of leaving the caller guessing. */
async function resolveKild(id: string): Promise<CommandResult<KildTarget>> {
  const located = kildManager.kildDir(id);
  if (located.ok) return { ok: true, value: { id, live: true, ...located.value } };

  const tree = (await kildTrees(await kildRepos())).find((candidate) => candidate.worktree === id);
  if (tree) {
    return {
      ok: true,
      value: {
        id,
        name: tree.worktree,
        live: false,
        repo: tree.repo,
        dir: tree.path,
        worktree: tree.worktree,
      },
    };
  }

  const archived = kildManager.archived().find((candidate) => candidate.id === id);
  if (archived) {
    return {
      ok: false,
      code: 'invalid_state',
      message:
        `kild ${id} is archived (its agents are gone)` +
        `${archived.worktree ? ` — address its tree as '${archived.worktree}'` : ' and had no worktree'}`,
    };
  }
  return { ok: false, code: 'not_found', message: `no such kild: ${id}` };
}

// ── Disposal ──────────────────────────────────────────────────────────────────
// The verb nothing had: `land` handles success and `stop` keeps the tree, so a kild nobody
// ever lands had no ending at all. Guarded on AUTHORED COMMITS (see kild-disposal.ts) —
// never on working-tree dirt, which is provisioning litter more often than work. The
// `kild/<name>` BRANCH always survives, so nothing committed is ever lost here; `?force=true`
// overrides the commits refusal for exactly that reason. Every outcome is reported: a
// disposal that quietly declines is the defect this replaces.
app.delete('/api/kilds/:id', async (c) => {
  const id = c.req.param('id');
  const forceQuery = c.req.query('force');
  if (forceQuery !== undefined && forceQuery !== 'true' && forceQuery !== 'false') {
    return c.json({ error: "force must be 'true' or 'false'" }, 400);
  }
  const force = forceQuery === 'true';

  const target = await resolveKild(id);
  if (!target.ok) {
    return c.json({ error: target.message, code: target.code }, kildResultStatus(target));
  }
  const { worktree, repo, dir, base, live, name } = target.value;
  if (!worktree) {
    return c.json(
      {
        error: `kild '${name}' ran in the main checkout (${repo}) — it has no worktree to dispose of`,
        code: 'no_worktree',
      },
      409,
    );
  }

  const assessment = await assessDisposal({
    repo,
    dir,
    branch: `kild/${worktree}`,
    base,
    inUse: worktreesInUse().has(worktree),
    force,
  });
  if (!assessment.ok) {
    return c.json(
      {
        error: assessment.message,
        code: assessment.code,
        branch: `kild/${worktree}`,
        ...(assessment.commits !== undefined ? { commits: assessment.commits } : {}),
        ...(assessment.tip ? { tip: assessment.tip } : {}),
      },
      assessment.code === 'not_found' ? 404 : 409,
    );
  }

  // Stop the kild BEFORE the tree goes: its ledger entry is written from the working dir,
  // so those facts have to be collected while the dir still exists.
  if (live) await kildManager.stop(id);
  try {
    await removeKildTree(repo, dir);
  } catch (err) {
    return c.json({ error: errText(err), code: 'rejected', branch: `kild/${worktree}` }, 409);
  }
  return c.json({
    ok: true,
    id,
    worktree,
    branch: `kild/${worktree}`,
    // Removing a worktree frees disk; it never deletes work.
    branchKept: true,
    removed: dir,
    discarded: assessment.discarded,
    forced: assessment.forced,
    message:
      `Removed worktree '${worktree}'${assessment.forced ? ` (forced past ${assessment.commits} unlanded commit${assessment.commits === 1 ? '' : 's'})` : ''}. ` +
      `Branch kild/${worktree} kept.` +
      (assessment.discarded.length > 0
        ? ` Discarded ${assessment.discarded.length} uncommitted file${assessment.discarded.length === 1 ? '' : 's'}.`
        : ''),
  });
});

// ── Landing ───────────────────────────────────────────────────────────────────
// GET is the dry run and touches NOTHING; POST performs the merge and records the sha it
// became on the kild, so the ledger can name the commit instead of inferring containment.
// Same body either way, so the two are comparable line for line.
app.get('/api/kilds/:id/land', async (c) => {
  const target = await resolveKild(c.req.param('id'));
  if (!target.ok) {
    return c.json({ error: target.message, code: target.code }, kildResultStatus(target));
  }
  return c.json({ ...(await landPlan(target.value.dir, target.value.base)), dryRun: true });
});
app.post('/api/kilds/:id/land', async (c) => {
  const id = c.req.param('id');
  const target = await resolveKild(id);
  if (!target.ok) {
    return c.json({ error: target.message, code: target.code }, kildResultStatus(target));
  }
  const result = await landMerge(target.value.repo, target.value.dir, target.value.base);
  // A land that did not happen is NOT a success — the caller must be able to tell.
  if (!result.merged) return c.json({ ...result, dryRun: false }, 409);
  if (target.value.live && result.sha) kildManager.recordLand(id, result.sha);
  return c.json({ ...result, dryRun: false });
});

// ── Review intelligence ───────────────────────────────────────────────────────
// Git drill-down for a live kild (worktree if set, else cwd — the same resolution
// live-kild status uses): commits vs the kild's base, per-file diff stats
// (committed + uncommitted), and one file's unified patch. Live kilds only — an
// archived kild's working dir is gone (409); unknown ids 404. Git failures inside
// a resolved kild are data ({error} in the body, per worktree-status), never a 500.
app.get('/api/kilds/:id/git/commits', async (c) => {
  const located = kildManager.kildDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, kildResultStatus(located));
  }
  return c.json(await reviewCommits(located.value.dir, located.value.base));
});
app.get('/api/kilds/:id/git/files', async (c) => {
  const located = kildManager.kildDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, kildResultStatus(located));
  }
  return c.json(await reviewFiles(located.value.dir, located.value.base));
});
app.get('/api/kilds/:id/git/diff', async (c) => {
  const file = c.req.query('path');
  if (!file) return c.json({ error: 'path query parameter required' }, 400);
  const located = kildManager.kildDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, kildResultStatus(located));
  }
  const diff = await reviewDiff(located.value.dir, located.value.base, file);
  // The traversal guard: only a path git itself reported may be diffed.
  if (diff.unknownPath) return c.json({ error: diff.error }, 404);
  return c.json(diff);
});

// ── Live stream (WebSocket) ─────────────────────────────────────────────────
// Every connection subscribes to the kild broadcast — so kilds created by any client
// (UI client or CLI) are visible to all. Kilds are engine-owned and survive a drop.
// Frames carry the kild id as `id`; sessions are the internal substrate, not on the wire.
type ClientMessage =
  | {
      type: 'spawn';
      id: string;
      cwd?: string;
      persona?: string;
      model?: string;
      worktree?: string;
      base?: string;
      label?: string;
      env?: Record<string, string>;
    }
  | { type: 'prompt'; id: string; text: string; from?: string }
  | { type: 'stop'; id: string }
  | {
      type: 'kild_new';
      id: string;
      name: string;
      cwd: string;
      agents: Array<{ handle: string; persona?: string; model?: string }>;
      worktree?: string;
      base?: string;
    }
  | { type: 'kild_send'; id: string; to: string[]; text: string }
  | {
      type: 'kild_spawn';
      id: string;
      agent: { handle: string; persona?: string; model?: string };
    }
  | { type: 'kild_stop'; id: string };

function parseClientMessage(data: string): ClientMessage | null {
  let msg: unknown;
  try {
    msg = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof msg !== 'object' || msg === null) return null;
  const m = msg as Record<string, unknown>;
  if (typeof m.id !== 'string') return null;
  if (m.type === 'spawn') {
    if (m.cwd !== undefined && typeof m.cwd !== 'string') return null;
    if (m.persona !== undefined && typeof m.persona !== 'string') return null;
    if (m.model !== undefined && typeof m.model !== 'string') return null;
    if (m.worktree !== undefined && typeof m.worktree !== 'string') return null;
    if (m.label !== undefined && typeof m.label !== 'string') return null;
    const env = envRecord(m.env);
    if (m.env !== undefined && env === undefined) return null;
    return { ...(m as Omit<Extract<ClientMessage, { type: 'spawn' }>, 'env'>), env };
  }
  if (m.type === 'prompt') {
    if (typeof m.text !== 'string') return null;
    if (m.from !== undefined && typeof m.from !== 'string') return null;
    return m as ClientMessage;
  }
  if (m.type === 'stop') return m as ClientMessage;
  if (m.type === 'kild_new') {
    if (typeof m.name !== 'string' || typeof m.cwd !== 'string' || !Array.isArray(m.agents)) {
      return null;
    }
    if (m.worktree !== undefined && typeof m.worktree !== 'string') return null;
    return m as ClientMessage;
  }
  if (m.type === 'kild_send' && typeof m.text === 'string' && recipients(m.to).ok) {
    return m as ClientMessage;
  }
  if (m.type === 'kild_spawn' && typeof m.agent === 'object' && m.agent !== null) {
    return m as ClientMessage;
  }
  if (m.type === 'kild_stop') return m as ClientMessage;
  return null;
}

app.get(
  '/ws',
  async (c, next) => {
    if (!originAllowed(c.req.header('origin'))) return c.text('forbidden', 403);
    return next();
  },
  upgradeWebSocket(() => {
    let unsubscribeKilds: (() => void) | undefined;
    let unsubscribeAgents: (() => void) | undefined;
    // Kild commands became async when create() gained validation, which broke the
    // implicit frame ordering clients rely on (create immediately followed by the
    // kickoff send raced, and the send was rejected with "no such kild"). Frames
    // on one connection execute strictly in arrival order.
    let queue: Promise<void> = Promise.resolve();
    const enqueue = (label: string, task: () => Promise<{ ok: boolean; message?: string }>) => {
      queue = queue.then(async () => {
        const result = await task();
        if (!result.ok) console.warn(`kild: ${label} rejected: ${result.message}`);
      });
      queue = queue.catch((err) => console.warn(`kild: ${label} failed: ${errText(err)}`));
    };
    return {
      onOpen(_evt, ws) {
        unsubscribeKilds = kildManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
        unsubscribeAgents = agentManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = parseClientMessage(String(evt.data));
        if (!msg) return; // ignore malformed / unknown frames
        if (msg.type === 'spawn') {
          agentManager.spawn(
            msg.id,
            {
              cwd: msg.cwd,
              persona: msg.persona,
              model: msg.model,
              worktree: msg.worktree,
              base: msg.base,
              label: msg.label,
              env: msg.env,
            },
            'cli',
          );
        } else if (msg.type === 'prompt') {
          agentManager.prompt(msg.id, msg.text, msg.from);
        } else if (msg.type === 'stop') {
          agentManager.stop(msg.id);
        } else if (msg.type === 'kild_new') {
          enqueue(`kild_new ${msg.id}`, () =>
            kildManager.create(msg.id, {
              name: msg.name,
              cwd: msg.cwd,
              agents: msg.agents,
              worktree: msg.worktree,
              base: msg.base,
            }),
          );
        } else if (msg.type === 'kild_send') {
          enqueue(`kild_send ${msg.id}`, () =>
            kildManager.send(msg.id, UNATTRIBUTED, msg.to, msg.text),
          );
        } else if (msg.type === 'kild_spawn') {
          enqueue(`kild_spawn ${msg.id}`, () => kildManager.spawnAgent(msg.id, msg.agent));
        } else if (msg.type === 'kild_stop') {
          enqueue(`kild_stop ${msg.id}`, () => kildManager.stop(msg.id));
        }
      },
      onClose() {
        unsubscribeKilds?.();
        unsubscribeAgents?.();
      },
    };
  }),
);

console.log(`kild-engine listening on http://${HOST}:${PORT}`);

// One-shot merge-prune on start: clean up worktrees whose kild/* branch already
// landed in the default branch. Fire-and-forget per registered project; no timer.
void loadProjects()
  .then((projects) =>
    Promise.all(
      projects.map((p) =>
        pruneMergedWorktrees(p.path, worktreesInUse()).catch((err) => {
          // A non-git/unreadable project dir is expected (skip quietly-ish); anything
          // else is logged rather than hidden.
          console.warn(`kild: startup prune skipped ${p.name}: ${errText(err)}`);
        }),
      ),
    ),
  )
  .catch((err) => console.warn(`kild: startup prune failed: ${errText(err)}`));

// Kill child agent processes on shutdown so a `--watch` reload or Ctrl-C never orphans
// them (otherwise they reparent to init and linger as zombie sessions).
for (const sig of ['SIGINT', 'SIGTERM'] as const) {
  process.on(sig, () => {
    agentManager.shutdown();
    process.exit(0);
  });
}

export default { port: PORT, hostname: HOST, fetch: app.fetch, websocket };
