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
 * harness, so the only remedy was relaunching from a prepared shell. Worse, a resumed or
 * forked session rebuilds its environment from the original session's, so for those there is
 * no shell whose environment reaches the process at all.
 *
 * Keyed by the harness's own session id — an opaque string kild stores and hands back. Only
 * the hook script knows where that id comes from; the engine learns nothing about any
 * harness here, which is what keeps this on kild's side of the boundary.
 *
 * What is NOT here: the attach token. It is minted in memory and dies with the engine
 * ({@link ../kild/attach-token.ts}), so a copy on disk would outlive the thing it names and
 * be rejected as unknown on the next send. The token is fetched at call time from the
 * idempotent `attach` instead, which is always fresh and needs no secret on disk.
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

async function readRecord(file: string): Promise<Attachment | null> {
  let raw: string;
  try {
    raw = await fs.readFile(file, 'utf8');
  } catch {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as Partial<Attachment>;
    if (typeof parsed.kildId !== 'string' || typeof parsed.handle !== 'string') return null;
    return {
      kildId: parsed.kildId,
      handle: parsed.handle,
      attachedAt: typeof parsed.attachedAt === 'number' ? parsed.attachedAt : 0,
    };
  } catch {
    return null;
  }
}

/**
 * Record (or re-point) a session's attachment, and **supersede** any other session holding
 * the same (kild, handle).
 *
 * Superseding is not tidiness. A session id is not stable: `--resume`/`--fork-session` mints
 * a new one for what the operator experiences as the same session, so without this every
 * fork would leave an orphan record behind and the directory would grow one entry per
 * relaunch. And a handle is one identity — two live sessions sharing it would race on the
 * same inbox, each destructively draining mail meant for the other. Last attach wins.
 */
export async function recordAttachment(
  session: string,
  kildId: string,
  handle: string,
): Promise<void> {
  const file = recordPath(session);
  await fs.mkdir(path.dirname(file), { recursive: true });
  const record: Attachment = { kildId, handle, attachedAt: Date.now() };
  await fs.writeFile(file, `${JSON.stringify(record)}\n`);
  await supersede(file, kildId, handle);
}

/** Drop every OTHER session's record of this (kild, handle). Failure to enumerate is not
 *  fatal — the record we just wrote is the one that matters, and a leftover sibling costs a
 *  stale entry, never a wrong answer for the session that just attached. */
async function supersede(keep: string, kildId: string, handle: string): Promise<void> {
  let entries: string[];
  try {
    entries = await fs.readdir(attachmentsDir());
  } catch {
    return;
  }
  await Promise.all(
    entries
      .filter((entry) => entry.endsWith('.json'))
      .map(async (entry) => {
        const file = path.join(attachmentsDir(), entry);
        if (file === keep) return;
        const record = await readRecord(file);
        if (record?.kildId === kildId && record.handle === handle) {
          await fs.rm(file, { force: true });
        }
      }),
  );
}

/**
 * The session's attachment, or null when it has none.
 *
 * Unreadable, malformed, and absent all read as "not attached". This is the property that
 * makes it safe to call from a turn-end hook: a hook must degrade to silence, and a dead key
 * left behind by a fork must not become an error on somebody's every turn.
 */
export async function findAttachment(session: string): Promise<Attachment | null> {
  return readRecord(recordPath(session));
}

/** Drop a session's record. Used when its kild stops, so a stale id cannot point a later
 *  session at a kild that is gone. */
export async function forgetAttachment(session: string): Promise<void> {
  await fs.rm(recordPath(session), { force: true });
}
