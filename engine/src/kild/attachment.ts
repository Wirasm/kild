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
 * Session ids reach the filesystem as filenames, so anything that is not a plain handle is
 * refused rather than allowed to walk out of the directory with `../`. A harness that
 * reports an exotic id gets a loud error here, not a write to somewhere else.
 */
function recordPath(session: string): string {
  if (!/^[A-Za-z0-9._-]+$/.test(session)) {
    throw new Error(`invalid session id: ${session}`);
  }
  return path.join(attachmentsDir(), `${session}.json`);
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

/** How long to wait for another attach to finish before treating its lock as abandoned.
 *  Attaching is one small write and a scan of a handful of files, so a lock still held after
 *  this long is a crashed process, not a slow one. */
const LOCK_STALE_MS = 5_000;
const LOCK_POLL_MS = 25;

/**
 * Run `fn` with exclusive access to the attachments directory.
 *
 * `mkdir` is atomic and fails with EEXIST when the directory is already there, which makes it
 * the smallest mutual exclusion available without a dependency. It is needed because
 * superseding is a read-modify-write over a shared directory: two sessions attaching to the
 * same (kild, handle) at once would each scan, each see the other's freshly written record as
 * "somebody else's", and each delete it — leaving NEITHER session attached rather than the
 * last one. That is data loss, and it is the fork case this whole record exists to serve.
 *
 * Failing to acquire is not fatal. The record itself is written under the lock either way;
 * only superseding is skipped, and it says so. A wedged lock must never stop an operator
 * attaching.
 */
async function withLock<T>(fn: () => Promise<T>): Promise<T | undefined> {
  const lock = path.join(attachmentsDir(), '.lock');
  const deadline = Date.now() + LOCK_STALE_MS;
  for (;;) {
    try {
      await fs.mkdir(lock);
      break;
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== 'EEXIST') throw err;
      if (Date.now() >= deadline) {
        // Break a lock this old rather than spin forever, then take it ourselves. If that
        // race is lost too, give up loudly instead of looping.
        await fs.rm(lock, { recursive: true, force: true });
        try {
          await fs.mkdir(lock);
          break;
        } catch {
          console.error(`kild: could not lock ${attachmentsDir()}; skipped superseding`);
          return undefined;
        }
      }
      await new Promise((resolve) => setTimeout(resolve, LOCK_POLL_MS));
    }
  }
  try {
    return await fn();
  } finally {
    await fs.rm(lock, { recursive: true, force: true });
  }
}

/**
 * Record (or re-point) a session's attachment, and **supersede** any other session holding
 * the same (kild, handle).
 *
 * Superseding is not tidiness. A session id is not stable: `--resume`/`--fork-session` mints
 * a new one for what the operator experiences as the same session, so without this every fork
 * leaves an orphan record and the directory grows one entry per relaunch. And a handle is one
 * identity — two live sessions sharing it would race on one destructive inbox, each eating
 * mail meant for the other. Last attach wins.
 */
export async function recordAttachment(
  session: string,
  kildId: string,
  handle: string,
): Promise<void> {
  const file = recordPath(session);
  await fs.mkdir(path.dirname(file), { recursive: true });
  await withLock(async () => {
    const record: Attachment = { kildId, handle, attachedAt: Date.now() };
    await fs.writeFile(file, `${JSON.stringify(record)}\n`);
    await supersede(file, kildId, handle);
  });
}

/**
 * Drop every OTHER session's record of this (kild, handle). Runs under the lock.
 *
 * Not fatal to the attach that just succeeded — its own record is written and correct. But it
 * is not nothing either: superseding is what backs "two sessions can never race on one
 * destructive inbox", so a failure means that guarantee quietly does not hold. Each failure is
 * REPORTED rather than swallowed; the attach stands, and the operator is told a stale sibling
 * may have survived it.
 */
async function supersede(keep: string, kildId: string, handle: string): Promise<void> {
  const dir = attachmentsDir();
  let entries: string[];
  try {
    entries = await fs.readdir(dir);
  } catch (err) {
    console.error(
      `kild: could not check ${dir} for other sessions holding @${handle}: ${(err as Error).message}`,
    );
    return;
  }
  await Promise.all(
    entries
      .filter((entry) => entry.endsWith('.json'))
      .map(async (entry) => {
        const file = path.join(dir, entry);
        if (file === keep) return;
        // One unreadable sibling must not fail the attach, but it is exactly the case where a
        // duplicate claim could survive unseen — so say so instead of skipping in silence.
        try {
          const record = await readRecord(file);
          if (record?.kildId === kildId && record.handle === handle) {
            await fs.rm(file, { force: true });
          }
        } catch (err) {
          console.error(`kild: could not supersede ${file}: ${(err as Error).message}`);
        }
      }),
  );
}

/**
 * The session's attachment, or null when it has none.
 *
 * Only *absent* is null. Anything that means "there may be an attachment and I cannot read
 * it" throws, so a caller that must not act on a guess does not get one. The turn-end hook
 * catches and degrades to silence; `kild send` lets it through and fails.
 */
export async function findAttachment(session: string): Promise<Attachment | null> {
  return readRecord(recordPath(session));
}
