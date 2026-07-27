// One binary, two roles: `KILD_ROLE=worker` runs a single agent session (one per
// process — the coding-agent SDK requires it); otherwise this is the engine.
if (process.env.KILD_ROLE === 'worker') {
  await (await import('./worker.ts')).runWorker();
}

import { execFile as execFileCb } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

import { type Context, Hono } from 'hono';
import { createBunWebSocket } from 'hono/bun';
import { cors } from 'hono/cors';

import { listAgents } from './kild/agents.ts';
import { reviewCommits, reviewDiff, reviewFiles } from './kild/git-review.ts';
import { addProject, findProject, loadProjects } from './kild/projects.ts';
import {
  resolveCloseRoomActor,
  resolveOpenRoomActor,
  resolvePostRoomActor,
} from './kild/room/rest-room-attribution.ts';
import { roomManager } from './kild/room/room-manager.ts';
import type { CommandResult, ParticipantSpec } from './kild/room/room-types.ts';
import { readSessionTranscript } from './kild/session-transcript.ts';
import { sessionManager } from './kild/sessions.ts';
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
// Reusable role definitions, read from the convention dirs (`.pi/agents` etc. — the
// on-disk dir names are upstream pi convention and unrelated to this route's name).
app.get('/api/personas', async (c) => {
  const ref = await resolveProjectRef(c.req.query('project'), c.req.query('path'));
  // Personas without a project are valid (global-only discovery) — only a BAD ref errors.
  if (!ref.ok && (c.req.query('project') !== undefined || c.req.query('path') !== undefined)) {
    return c.json({ error: ref.error }, ref.status);
  }
  return c.json(await listAgents(ref.ok ? ref.dir : undefined));
});

// ── Worktrees ─────────────────────────────────────────────────────────────────
// Worktree names a live session is using are never pruned.

function participantSpecs(input: unknown): ParticipantSpec[] | null {
  if (!Array.isArray(input)) return null;
  const participants: ParticipantSpec[] = [];
  for (const item of input) {
    if (typeof item !== 'object' || item === null) return null;
    const participant = item as Record<string, unknown>;
    if (typeof participant.name !== 'string') return null;
    if (participant.persona !== undefined && typeof participant.persona !== 'string') return null;
    if (participant.model !== undefined && typeof participant.model !== 'string') return null;
    participants.push({
      name: participant.name,
      persona: typeof participant.persona === 'string' ? participant.persona : undefined,
      model: typeof participant.model === 'string' ? participant.model : undefined,
    });
  }
  return participants;
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

function roomResultStatus(result: Extract<CommandResult<unknown>, { ok: false }>): 404 | 409 {
  return result.code === 'not_found' ? 404 : 409;
}

const worktreesInUse = (): Set<string> =>
  new Set(
    sessionManager
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

// A room participant's transcript — works for LIVE and ARCHIVED rooms alike: the
// piSessionFile handle persists in $KILD_HOME/rooms/<id>.json past the session's death.
app.get('/api/rooms/:id/participants/:name/transcript', async (c) => {
  const id = c.req.param('id');
  const name = c.req.param('name');
  const room =
    roomManager.liveRooms().find((r) => r.id === id) ??
    roomManager.archived().find((r) => r.id === id);
  if (!room) return c.json({ error: `no such room: ${id}` }, 404);
  const participant = room.participants.find((p) => p.name === name);
  if (!participant) return c.json({ error: `no such participant: @${name}` }, 404);
  return serveTranscript(c, participant.piSessionFile, `participant @${name}`);
});

// A live session's transcript (operator sessions, one-shot runs) via SessionInfo.
app.get('/api/sessions/:id/transcript', async (c) => {
  const id = c.req.param('id');
  const info = sessionManager.list().find((s) => s.id === id);
  if (!info) return c.json({ error: `no such session: ${id}` }, 404);
  return serveTranscript(c, info.piSessionFile, `session ${id}`);
});

// ── Sessions ──────────────────────────────────────────────────────────────────
app.get('/api/sessions', (c) => c.json(sessionManager.list()));

// Spawn a detached session — the CLI/scripts drive this over REST instead of holding a
// WS open. `forkFrom` spawns the session from a frozen copy of an existing pi session
// file: the fork gets a NEW session file and never writes the source.
app.post('/api/sessions', async (c) => {
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
  sessionManager.spawn(
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
  if (body.prompt) sessionManager.prompt(id, body.prompt);
  return c.json({ ok: true, id });
});

app.post('/api/sessions/:id/prompt', async (c) => {
  const { text } = (await c.req.json().catch(() => ({}))) as { text?: string };
  if (!text) return c.json({ error: 'text required' }, 400);
  const delivered = sessionManager.prompt(c.req.param('id'), text);
  return delivered ? c.json({ ok: true }) : c.json({ error: 'no such session' }, 404);
});

app.post('/api/sessions/:id/stop', (c) => {
  sessionManager.stop(c.req.param('id'));
  return c.json({ ok: true });
});

// ── Rooms ─────────────────────────────────────────────────────────────────────
// Past rooms recovered from disk (read-only history). Live rooms flow over the WS
// (`{rooms}` summaries + `{roomMessage}` posts); this is the conversation record of
// rooms from previous engine runs — their participant subprocesses are long gone.
app.get('/api/rooms/archive', (c) => c.json(roomManager.archived()));
// Live rooms WITH their logs — so a UI client joining a room it didn't open (or after a
// refresh) can load the conversation so far. The WS only streams *new* messages.
app.get('/api/rooms/live', async (c) => c.json(await roomManager.liveRoomsStatus()));
app.post('/api/rooms', async (c) => {
  const body = await c.req.json<{
    name?: unknown;
    cwd?: unknown;
    project?: unknown;
    worktree?: unknown;
    base?: unknown;
    participants?: unknown;
    kickoff?: unknown;
    from?: unknown;
    openedBy?: unknown;
  }>();
  if (typeof body.name !== 'string') return c.json({ error: 'name required' }, 400);
  if (typeof body.kickoff !== 'string' || !body.kickoff.trim()) {
    return c.json({ error: 'kickoff required' }, 400);
  }
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
  const participants = participantSpecs(body.participants);
  if (!participants || participants.length === 0) {
    return c.json({ error: 'participants must name at least one participant' }, 400);
  }

  // Where the room runs: `project` is a registered name, `cwd` an absolute path —
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
    // own cwd sends a well-formed path to a directory that does not exist. That opened
    // a room whose worktree could not be created and whose participants never spawned —
    // and the caller got a room id and a success. Refuse here so the failure is the
    // caller's, not a room that quietly does nothing.
    const dir = await fs.stat(body.cwd).catch(() => null);
    if (!dir?.isDirectory()) {
      return c.json({ error: `cwd is not an existing directory: ${body.cwd}` }, 400);
    }
    cwd = body.cwd;
  } else {
    return c.json({ error: 'cwd (absolute path) or project (registered name) required' }, 400);
  }

  const attribution = resolveOpenRoomActor(
    {
      from: typeof body.from === 'string' ? body.from : undefined,
      openedBy: body.openedBy,
    },
    sessionManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, roomResultStatus(attribution));

  const id = randomUUID();
  const opened = await roomManager.open(id, {
    name: body.name,
    cwd,
    participants,
    worktree: body.worktree,
    base: typeof body.base === 'string' ? body.base : undefined,
    openedBy: body.openedBy,
  });
  if (!opened.ok) return c.json({ error: opened.message }, roomResultStatus(opened));
  // Addressing is structured now: the manager defaults an untargeted post to the room
  // lead, so the kickoff reaches the orchestrator without munging the text.
  const posted = attribution.value.human
    ? await roomManager.postFromHuman(id, body.kickoff)
    : await roomManager.postAs(id, attribution.value.actor, body.kickoff);
  if (!posted.ok) {
    await roomManager.close(id);
    return c.json({ error: posted.message }, roomResultStatus(posted));
  }
  return c.json({ ok: true, id: opened.value.roomId, message: opened.value.message });
});
// `to` is the structured recipient list the in-room `post_message` tool already takes —
// exposed here so a caller outside the room (the CLI, a script, helm) can address a
// specific participant instead of always falling through to the room lead. Addressing is
// never parsed from the message text, so `@name` in `text` remains just text.
app.post('/api/rooms/:id/post', async (c) => {
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
  if (to !== undefined && (!Array.isArray(to) || to.some((t) => typeof t !== 'string')))
    return c.json({ error: 'to must be an array of participant handles' }, 400);
  // An empty array would read as "addressed to nobody" while behaving as "addressed to
  // the lead" — reject it rather than silently doing something else.
  if (Array.isArray(to) && to.length === 0)
    return c.json({ error: 'to must name at least one participant' }, 400);
  const recipients = to as string[] | undefined;
  const id = c.req.param('id');
  const attribution = resolvePostRoomActor(
    { from: typeof from === 'string' ? from : undefined, sessionId },
    sessionManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, roomResultStatus(attribution));
  const result = attribution.value.human
    ? await roomManager.postFromHuman(id, text, recipients)
    : await roomManager.postAs(id, attribution.value.actor, text, recipients);
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
// The attached half of the roster: a harness kild does NOT own (a Claude Code session the
// human is driving) claims a `@handle` and gets a mailbox. Idempotent by name, so a hook
// or shell alias can call it on every session start.
app.post('/api/rooms/:id/join', async (c) => {
  const { name } = await c.req.json<{ name?: unknown }>();
  if (typeof name !== 'string' || !name.trim()) return c.json({ error: 'name required' }, 400);
  const result = await roomManager.join(c.req.param('id'), name);
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});
// The destructive read of that mailbox, and with it the participant's idle signal (an
// empty drain = idle). POST, never GET: this MUTATES, so a caching proxy or a retry on a
// GET would silently eat somebody's messages.
app.post('/api/rooms/:id/drain', async (c) => {
  const { name } = await c.req.json<{ name?: unknown }>();
  if (typeof name !== 'string' || !name.trim()) return c.json({ error: 'name required' }, 400);
  const result = roomManager.drain(c.req.param('id'), name);
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, ...result.value });
});
app.post('/api/rooms/:id/close', async (c) => {
  const { from, sessionId, force } = await c.req.json<{
    from?: unknown;
    sessionId?: unknown;
    force?: unknown;
  }>();
  if (from !== undefined && typeof from !== 'string')
    return c.json({ error: 'from must be a string' }, 400);
  if (sessionId !== undefined && typeof sessionId !== 'string')
    return c.json({ error: 'sessionId must be a string' }, 400);
  if (force !== undefined && typeof force !== 'boolean')
    return c.json({ error: 'force must be a boolean' }, 400);
  const attribution = resolveCloseRoomActor(
    { from: typeof from === 'string' ? from : undefined, sessionId },
    sessionManager,
  );
  if (!attribution.ok) return c.json({ error: attribution.message }, roomResultStatus(attribution));
  const result = await roomManager.close(c.req.param('id'), { force: force === true });
  if (!result.ok) return c.json({ error: result.message }, roomResultStatus(result));
  return c.json({ ok: true, message: result.value.message });
});

// ── Review intelligence ───────────────────────────────────────────────────────
// Git drill-down for a live room (worktree if set, else cwd — the same resolution
// live-room status uses): commits vs the room's base, per-file diff stats
// (committed + uncommitted), and one file's unified patch. Live rooms only — an
// archived room's working dir is gone (409); unknown ids 404. Git failures inside
// a resolved room are data ({error} in the body, per room-git-status), never a 500.
app.get('/api/rooms/:id/git/commits', async (c) => {
  const located = roomManager.roomDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, roomResultStatus(located));
  }
  return c.json(await reviewCommits(located.value.dir, located.value.base));
});
app.get('/api/rooms/:id/git/files', async (c) => {
  const located = roomManager.roomDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, roomResultStatus(located));
  }
  return c.json(await reviewFiles(located.value.dir, located.value.base));
});
app.get('/api/rooms/:id/git/diff', async (c) => {
  const file = c.req.query('path');
  if (!file) return c.json({ error: 'path query parameter required' }, 400);
  const located = roomManager.roomDir(c.req.param('id'));
  if (!located.ok) {
    return c.json({ error: located.message, code: located.code }, roomResultStatus(located));
  }
  const diff = await reviewDiff(located.value.dir, located.value.base, file);
  // The traversal guard: only a path git itself reported may be diffed.
  if (diff.unknownPath) return c.json({ error: diff.error }, 404);
  return c.json(diff);
});

// ── Live stream (WebSocket) ─────────────────────────────────────────────────
// Every connection subscribes to the room broadcast — so rooms opened by any client
// (UI client or CLI) are visible to all. Rooms are engine-owned and survive a drop.
// Frames carry the room id as `id`; sessions are the internal substrate, not on the wire.
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
      type: 'room_open';
      id: string;
      name: string;
      cwd: string;
      participants: Array<{ name: string; persona?: string; model?: string }>;
      worktree?: string;
      base?: string;
    }
  | { type: 'room_post'; id: string; text: string }
  | {
      type: 'room_add';
      id: string;
      participant: { name: string; persona?: string; model?: string };
    }
  | { type: 'room_halt'; id: string }
  | { type: 'room_close'; id: string };

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
  if (m.type === 'room_open') {
    if (typeof m.name !== 'string' || typeof m.cwd !== 'string' || !Array.isArray(m.participants)) {
      return null;
    }
    if (m.worktree !== undefined && typeof m.worktree !== 'string') return null;
    return m as ClientMessage;
  }
  if (m.type === 'room_post' && typeof m.text === 'string') return m as ClientMessage;
  if (m.type === 'room_add' && typeof m.participant === 'object' && m.participant !== null) {
    return m as ClientMessage;
  }
  if (m.type === 'room_halt') return m as ClientMessage;
  if (m.type === 'room_close') return m as ClientMessage;
  return null;
}

app.get(
  '/ws',
  async (c, next) => {
    if (!originAllowed(c.req.header('origin'))) return c.text('forbidden', 403);
    return next();
  },
  upgradeWebSocket(() => {
    let unsubscribeRooms: (() => void) | undefined;
    let unsubscribeSessions: (() => void) | undefined;
    // Room commands became async when open() gained validation, which broke the
    // implicit frame ordering clients rely on (open immediately followed by the
    // kickoff post raced, and the post was rejected with "no such room"). Frames
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
        unsubscribeRooms = roomManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
        unsubscribeSessions = sessionManager.subscribe((msg) => ws.send(JSON.stringify(msg)));
      },
      onMessage(evt) {
        const msg = parseClientMessage(String(evt.data));
        if (!msg) return; // ignore malformed / unknown frames
        if (msg.type === 'spawn') {
          sessionManager.spawn(
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
          sessionManager.prompt(msg.id, msg.text, msg.from);
        } else if (msg.type === 'stop') {
          sessionManager.stop(msg.id);
        } else if (msg.type === 'room_open') {
          enqueue(`room_open ${msg.id}`, () =>
            roomManager.open(msg.id, {
              name: msg.name,
              cwd: msg.cwd,
              participants: msg.participants,
              worktree: msg.worktree,
              base: msg.base,
            }),
          );
        } else if (msg.type === 'room_post') {
          enqueue(`room_post ${msg.id}`, () => roomManager.postFromHuman(msg.id, msg.text));
        } else if (msg.type === 'room_add') {
          enqueue(`room_add ${msg.id}`, () => roomManager.addParticipant(msg.id, msg.participant));
        } else if (msg.type === 'room_halt') {
          enqueue(`room_halt ${msg.id}`, () => roomManager.halt(msg.id));
        } else if (msg.type === 'room_close') {
          enqueue(`room_close ${msg.id}`, () => roomManager.close(msg.id));
        }
      },
      onClose() {
        unsubscribeRooms?.();
        unsubscribeSessions?.();
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

// Kill child workers on shutdown so a `--watch` reload or Ctrl-C never orphans
// them (otherwise they reparent to init and linger as zombie sessions).
for (const sig of ['SIGINT', 'SIGTERM'] as const) {
  process.on(sig, () => {
    sessionManager.shutdown();
    process.exit(0);
  });
}

export default { port: PORT, hostname: HOST, fetch: app.fetch, websocket };
