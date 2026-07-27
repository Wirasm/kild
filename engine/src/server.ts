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
import { kildManager } from './kild/kild-manager.ts';
import type { AgentSpec, CommandResult } from './kild/kild-types.ts';
import { listPersonas } from './kild/personas.ts';
import { addProject, findProject, loadProjects } from './kild/projects.ts';
import {
  resolveNewKildActor,
  resolveSendActor,
  resolveStopActor,
  UNATTRIBUTED,
} from './kild/rest-attribution.ts';
import { readSessionTranscript } from './kild/session-transcript.ts';
import {
  assertSafeBranch,
  forceRemoveWorktree,
  listWorktrees,
  pruneMergedWorktrees,
  removeWorktree,
  worktreePath,
  worktreesRoot,
} from './kild/worktree.ts';

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
  const { name, path } = await c.req.json<{ name: string; path: string }>();
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

const optionalString = (value: unknown): string | undefined =>
  typeof value === 'string' ? value : undefined;

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

// ── Worktrees ─────────────────────────────────────────────────────────────────
// Worktree names a live session is using are never pruned.

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

app.get('/api/worktrees', async (c) => {
  const ref = await resolveProjectRef(c.req.query('project'), c.req.query('path'));
  if (!ref.ok) return c.json({ error: ref.error }, ref.status);
  const repo = ref.dir;
  try {
    await pruneMergedWorktrees(repo, worktreesInUse()); // prune-merged on every list
    const trees = (await listWorktrees(repo)).filter((t) => t.branch.startsWith('kild/'));
    return c.json(trees);
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

app.delete('/api/worktrees', async (c) => {
  const body = await c.req.json<{
    project?: string;
    path?: string;
    name: string;
    force?: boolean;
  }>();
  const { name, force } = body;
  const ref = await resolveProjectRef(optionalString(body.project), optionalString(body.path));
  if (!ref.ok) return c.json({ error: ref.error }, ref.status);
  const repo = ref.dir;
  if (force !== undefined && typeof force !== 'boolean') {
    return c.json({ error: 'force must be a boolean' }, 400);
  }
  try {
    assertSafeBranch(name); // allowlist before building a path under worktreesRoot()
    const wtPath = worktreePath(name);
    const result = worktreesInUse().has(name)
      ? { ok: false as const, code: 'in_use' as const }
      : force
        ? await forceRemoveWorktree(repo, wtPath)
        : await removeWorktree(repo, wtPath);
    if (!result.ok) {
      const error =
        result.code === 'dirty'
          ? `worktree '${name}' has uncommitted or untracked files; retry with force: true to discard them`
          : result.code === 'in_use'
            ? `worktree '${name}' is in use by a live session`
            : `worktree '${name}' was not found`;
      return c.json(
        { error, code: result.code, ...(result.files ? { files: result.files } : {}) },
        result.code === 'not_found' ? 404 : 409,
      );
    }
    return c.json({ ok: true, name });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

app.post('/api/worktrees/prune', async (c) => {
  const body = await c.req.json<{ project?: string; path?: string }>();
  const ref = await resolveProjectRef(optionalString(body.project), optionalString(body.path));
  if (!ref.ok) return c.json({ error: ref.error }, ref.status);
  const repo = ref.dir;
  try {
    const pruned = await pruneMergedWorktrees(repo, worktreesInUse());
    return c.json({ pruned });
  } catch (err) {
    return c.json({ error: String(err instanceof Error ? err.message : err) }, 400);
  }
});

// ── Open in OS ────────────────────────────────────────────────────────────────
// Reveal a worktree path in the OS file browser. Only paths under the worktree root
// are allowed — the engine is loopback-only but must never shell `open` on an
// arbitrary path. Keeps UI clients pure-web (no Tauri opener API needed).
app.post('/api/open', async (c) => {
  const { path: target } = await c.req.json<{ path: string }>();
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
  const { url } = await c.req.json<{ url: string }>();
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

// A live agent's transcript (detached agents, one-shot runs) via AgentInfo.
app.get('/api/agents/:id/transcript', async (c) => {
  const id = c.req.param('id');
  const info = agentManager.list().find((a) => a.id === id);
  if (!info) return c.json({ error: `no such session: ${id}` }, 404);
  return serveTranscript(c, info.piSessionFile, `session ${id}`);
});

// ── Agents ────────────────────────────────────────────────────────────────────
app.get('/api/agents', (c) => c.json(agentManager.list()));

// Spawn a detached agent — the CLI/scripts drive this over REST instead of holding a
// WS open. `forkFrom` spawns the agent from a frozen copy of an existing pi session
// file: the fork gets a NEW session file and never writes the source.
app.post('/api/agents', async (c) => {
  const body = (await c.req.json().catch(() => ({}))) as {
    persona?: string;
    model?: string;
    cwd?: string;
    worktree?: string;
    base?: string;
    label?: string;
    prompt?: string;
    forkFrom?: unknown;
  };
  if (body.forkFrom !== undefined) {
    if (typeof body.forkFrom !== 'string' || !body.forkFrom.trim()) {
      return c.json({ error: 'forkFrom must be a session file path' }, 400);
    }
    const stat = await fs.stat(body.forkFrom).catch(() => null);
    if (!stat?.isFile()) {
      return c.json({ error: `forkFrom is not an existing session file: ${body.forkFrom}` }, 400);
    }
  }
  const id = randomUUID();
  agentManager.spawn(
    id,
    {
      cwd: body.cwd,
      persona: body.persona,
      model: body.model,
      worktree: body.worktree,
      base: body.base,
      label: body.label,
      forkFrom: body.forkFrom,
    },
    'cli',
  );
  if (body.prompt) agentManager.prompt(id, body.prompt);
  return c.json({ ok: true, id });
});

app.post('/api/agents/:id/prompt', async (c) => {
  const { text } = (await c.req.json().catch(() => ({}))) as { text?: string };
  if (!text) return c.json({ error: 'text required' }, 400);
  const delivered = agentManager.prompt(c.req.param('id'), text);
  return delivered ? c.json({ ok: true }) : c.json({ error: 'no such session' }, 404);
});

app.post('/api/agents/:id/stop', (c) => {
  agentManager.stop(c.req.param('id'));
  return c.json({ ok: true });
});

// ── Kilds ─────────────────────────────────────────────────────────────────────
// Past kilds recovered from disk (read-only history). Live kilds flow over the WS
// (`{kilds}` summaries + `{message}` sends); this is the conversation record of
// kilds from previous engine runs — their agent subprocesses are long gone.
app.get('/api/kilds/archive', (c) => c.json(kildManager.archived()));
// Live kilds WITH their logs — so a UI client joining a kild it didn't create (or after a
// refresh) can load the conversation so far. The WS only streams *new* messages.
app.get('/api/kilds', async (c) => c.json(await kildManager.liveKildsStatus()));
app.post('/api/kilds', async (c) => {
  const body = await c.req.json<{
    name?: unknown;
    cwd?: unknown;
    project?: unknown;
    worktree?: unknown;
    base?: unknown;
    agents?: unknown;
    kickoff?: unknown;
    from?: unknown;
    openedBy?: unknown;
  }>();
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
    },
    agentManager,
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
  const { text, from, sessionId, to } = await c.req.json<{
    text?: unknown;
    from?: unknown;
    sessionId?: unknown;
    to?: unknown;
  }>();
  if (typeof text !== 'string') return c.json({ error: 'text required' }, 400);
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  if (sessionId !== undefined && typeof sessionId !== 'string')
    return c.json({ error: 'sessionId must be a string' }, 400);
  const addressed = recipients(to);
  if (!addressed.ok) return c.json({ error: addressed.error }, 400);
  const id = c.req.param('id');
  const attribution = resolveSendActor(
    { from: typeof from === 'string' ? from : undefined, sessionId },
    agentManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, kildResultStatus(attribution));
  const result = await kildManager.send(id, attribution.value, addressed.to, text);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
// The attached half of the roster: a harness kild does NOT own (a Claude Code session the
// human is driving) claims a `@handle` and gets an inbox. Idempotent by handle, so a hook
// or shell alias can call it on every session start.
app.post('/api/kilds/:id/agents/attach', async (c) => {
  const { handle } = await c.req.json<{ handle?: unknown }>();
  if (typeof handle !== 'string' || !handle.trim())
    return c.json({ error: 'handle required' }, 400);
  const result = await kildManager.attach(c.req.param('id'), handle);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
// The destructive read of that inbox, and with it the agent's idle signal (an empty
// drain = idle). POST, never GET: this MUTATES, so a caching proxy or a retry on a
// GET would silently eat somebody's messages.
app.post('/api/kilds/:id/inbox/drain', async (c) => {
  const { handle } = await c.req.json<{ handle?: unknown }>();
  if (typeof handle !== 'string' || !handle.trim())
    return c.json({ error: 'handle required' }, 400);
  const result = kildManager.drain(c.req.param('id'), handle);
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, ...result.value });
});
app.post('/api/kilds/:id/stop', async (c) => {
  const { from, sessionId } = await c.req.json<{
    from?: unknown;
    sessionId?: unknown;
  }>();
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  if (sessionId !== undefined && typeof sessionId !== 'string')
    return c.json({ error: 'sessionId must be a string' }, 400);
  const attribution = resolveStopActor(
    { from: typeof from === 'string' ? from : undefined, sessionId },
    agentManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, kildResultStatus(attribution));
  const result = await kildManager.stop(c.req.param('id'));
  if (!result.ok) return c.json({ error: result.message }, kildResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
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
