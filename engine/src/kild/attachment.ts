import fs from 'node:fs/promises';
import path from 'node:path';

import { kildHome } from './config.ts';

/**
 * Which kild a harness session attached to, recorded so the session can find its own handle
 * again without being told.
 *
 * The problem this removes: an attached agent was reachable only if `KILD_KILD_ID` was in
 * the harness process's environment. A process's environment is fixed at exec time, so a
 * session could not attach to a kild opened *after* it started — the normal case, since you
 * decide to delegate mid-session. `export` in a tool call changes a child shell, never the
 * harness. Worse, a resumed or forked session rebuilds its environment from a harness
 * settings file no shell owns, so for those there is nothing a shell can do at all.
 *
 * Keyed by the harness's own session id — an opaque string kild stores and hands back. Only
 * the hook script knows where that id comes from; the engine learns nothing about any
 * harness here, which is what keeps this on kild's side of the boundary.
 *
 * What is NOT here: the attach token. It is minted in memory and dies with the engine
 * (`attach-token.ts`), so a copy on disk would outlive the thing it names and be rejected as
 * unknown on the next send. The token is fetched at call time from the idempotent `attach`
 * instead, which is always fresh and needs no secret on disk.
 *
 * ## Two files, because a handle is one identity
 *
 * A handle is held by exactly one session: two sessions sharing one would race on a
 * destructive inbox, each eating mail meant for the other. And session ids are not stable —
 * `--resume`/`--fork-session` mints a new one for what the operator experiences as the same
 * session — so "who holds this handle" changes and must have a single answer at all times.
 *
 *   `attached/<session>.json`            what this session attached to
 *   `attached/claims/<kild>/<handle>`    which session currently holds that handle
 *
 * The claim is written by **atomic rename**, so concurrent attaches to one handle resolve to
 * exactly one winner with no lock, no timeout and no staleness heuristic — the last rename
 * wins and no intermediate state is ever observable. A session resolves only if the claim
 * still names it.
 *
 * The obvious alternative — one file per session plus a scan that deletes the others — is
 * what this replaced. It is a read-modify-write over a shared directory: two sessions each
 * saw the other's freshly written record as somebody else's and deleted it, leaving NEITHER
 * attached. Guarding it with a lock re-introduced timing questions (how old is too old, who
 * owns the lock, what if acquisition fails) that an atomic rename simply does not raise.
 */
export interface Attachment {
  /** The kild this session holds a handle in. */
  kildId: string;
  /** The handle claimed — what other agents address, and what `to[]` names. */
  handle: string;
  /** Epoch ms. Recorded for diagnosis; nothing expires on it. */
  attachedAt: number;
}

function attachmentsDir(): string {
  return path.join(kildHome(), 'attached');
}

/**
 * These strings become path segments, so anything that is not a plain token is refused rather
 * than allowed to walk out of the directory with `../`. A caller that supplies an exotic id
 * gets a loud error here, not a write to somewhere else.
 */
function safeSegment(kind: string, value: string): string {
  // `.` is allowed inside a value (worktree names carry dots), which means the character
  // class alone lets `.` and `..` through whole — and `path.join` then collapses `..` against
  // the segment before it, so `<kild>/..` resolves to the shared `claims` directory itself
  // rather than to a file in it. One such write turns that directory into a plain file and
  // every later attach fails with ENOTDIR, permanently. Blocking `/` is not enough; the
  // traversal tokens have to go too.
  if (value === '.' || value === '..' || !/^[A-Za-z0-9._-]+$/.test(value)) {
    throw new Error(`invalid ${kind}: ${value}`);
  }
  return value;
}

function recordPath(session: string): string {
  return path.join(attachmentsDir(), `${safeSegment('session id', session)}.json`);
}

/**
 * Kild and handle are separate path segments rather than one joined name: a separator inside
 * either value could otherwise make two different pairs collide on one filename.
 *
 * KNOWN GAP, deliberately not closed here: on a case-insensitive filesystem (the macOS
 * default) two handles differing only in case — `Alice` and `alice` — are distinct agents to
 * the engine's roster but fold to ONE claim file. The second attach overwrites the first, and
 * the first session then reads as not attached. It fails closed, never to somebody else's
 * identity, so it cannot mis-attribute; but it does go quiet, which is the failure this file
 * exists to make impossible. Handles that differ only in case are ambiguous to a reader too,
 * so the durable fix is for the engine to refuse them on the roster rather than for this file
 * to encode around them — that is a change to agent admission, not to storage.
 */
function claimPath(kildId: string, handle: string): string {
  return path.join(
    attachmentsDir(),
    'claims',
    safeSegment('kild id', kildId),
    safeSegment('handle', handle),
  );
}

/**
 * Read one record. **Absent is null; anything else throws.**
 *
 * Only "no such file" means *not attached*. A permission error, an I/O error or a corrupt
 * record all mean "there may be an attachment and I cannot read it", which is a different
 * fact and must not be flattened into the first one. This primitive reports what it found
 * and lets each caller decide: the turn-end hook degrades to silence, and `kild send` fails
 * loudly rather than posting a message with no credential.
 *
 * Flattening them is how the original bug worked — a send that silently lost its attribution
 * looked exactly like a successful one.
 */
async function readRecord(file: string): Promise<Attachment | null> {
  let raw: string;
  try {
    raw = await fs.readFile(file, 'utf8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw err;
  }
  let parsed: Partial<Attachment>;
  try {
    parsed = JSON.parse(raw) as Partial<Attachment>;
  } catch (err) {
    throw new Error(`unreadable attachment record ${file}: ${(err as Error).message}`);
  }
  if (typeof parsed.kildId !== 'string' || typeof parsed.handle !== 'string') {
    throw new Error(`malformed attachment record ${file}: kildId and handle are required`);
  }
  return {
    kildId: parsed.kildId,
    handle: parsed.handle,
    attachedAt: typeof parsed.attachedAt === 'number' ? parsed.attachedAt : 0,
  };
}

/** The session currently holding a handle, or null when nobody does. Absent is the only
 *  null; an unreadable claim throws, for the same reason an unreadable record does. */
async function readClaim(kildId: string, handle: string): Promise<string | null> {
  try {
    return (await fs.readFile(claimPath(kildId, handle), 'utf8')).trim();
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
    throw err;
  }
}

/**
 * Record a session's attachment and claim the handle for it.
 *
 * The claim is written FIRST, so a crash between the two writes leaves an unreferenced claim
 * (harmless) rather than a record that resolves against a claim that was never made. Both
 * writes go through a temp file and `rename`, which is atomic: a reader sees the old content
 * or the new one, never a half-written file, and concurrent writers produce exactly one
 * winner without coordinating.
 */
export async function recordAttachment(
  session: string,
  kildId: string,
  handle: string,
): Promise<void> {
  const claim = claimPath(kildId, handle);
  const file = recordPath(session);
  await fs.mkdir(path.dirname(claim), { recursive: true });
  await writeAtomic(claim, `${session}\n`);
  const record: Attachment = { kildId, handle, attachedAt: Date.now() };
  await writeAtomic(file, `${JSON.stringify(record)}\n`);
}

/** Write via a unique temp file in the same directory, then rename over the target. Same
 *  directory so the rename never crosses a filesystem, and unique so two concurrent writers
 *  cannot share a temp path. */
async function writeAtomic(file: string, contents: string): Promise<void> {
  const temp = `${file}.${process.pid}.${Math.random().toString(36).slice(2)}.tmp`;
  await fs.writeFile(temp, contents);
  try {
    await fs.rename(temp, file);
  } catch (err) {
    await fs.rm(temp, { force: true });
    throw err;
  }
}

/**
 * Records of superseded sessions are NOT deleted, and that is deliberate.
 *
 * A sweep used to run here to bound growth. It could not be made safe: it decided from a
 * snapshot it had already read but unlinked the live path, so an ordinary re-attach racing
 * any other session's sweep deleted a record whose claim still named it — leaving a session
 * holding a claim with no record, which resolves to "not attached", which makes the next send
 * go out with no credential. That is the exact failure this module exists to prevent, so a
 * disk-tidiness pass is not worth reaching for it.
 *
 * Leaving them costs one small inert file per forked session. Nothing scans this directory —
 * every read is a direct path lookup — so the cost is bytes and never latency or a wrong
 * answer. If it ever needs reclaiming, that belongs in an explicit maintenance verb the
 * operator runs, not in an implicit delete on somebody else's attach.
 */

/**
 * The session's attachment, or null when it has none.
 *
 * Null covers two cases that are the same fact from the caller's side: this session never
 * attached, and this session's claim on the handle has since been taken by another session
 * (the fork case). It does NOT cover "there may be an attachment and I cannot read it" —
 * that throws, so a caller which must not act on a guess does not get one. The turn-end hook
 * catches and degrades to silence; `kild send` lets it through and fails.
 *
 * Read-only by design: this runs on every turn from a hook, and a read path that mutates
 * state is a read path that can fail somebody's turn.
 */
export async function findAttachment(session: string): Promise<Attachment | null> {
  const record = await readRecord(recordPath(session));
  if (!record) return null;
  const owner = await readClaim(record.kildId, record.handle);
  return owner === session ? record : null;
}
