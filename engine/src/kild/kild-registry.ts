import fs from 'node:fs';
import path from 'node:path';

import { kildHome } from './config.ts';
import {
  type ArchivedKild,
  agentView,
  type Kild,
  type KildIdentity,
  type KildSummary,
  kildIdentity,
  type Message,
  type MessageDraft,
  type OwnedAgent,
} from './kild-types.ts';

/** A kild as it sits on disk. `seq` post-dates the first persisted kilds, so a file may
 *  hold messages without one — the loader numbers those by position (see
 *  {@link KildRegistry.loadArchive}). Decoding, not compatibility: the type the rest of the
 *  engine sees always has a cursor. */
interface PersistedKild extends Omit<ArchivedKild, 'log'> {
  log?: Array<MessageDraft & { seq?: number }>;
}

/**
 * Narrow one history file to the shape the engine's types promise, or refuse it.
 *
 * `JSON.parse(...) as PersistedKild` asserted a shape nothing had checked — the same
 * pattern that made a hand-edited `projects.json` 500 routes across the engine and a
 * `memory.dir: 42` throw out of a function documented as never throwing. A history file is
 * the same kind of input: written by an older engine, hand-edited, or truncated by a crash
 * mid-write. Only the fields the loader actually dereferences are checked; the rest ride
 * along, because the engine does not interpret them.
 */
function decodePersisted(
  raw: unknown,
): (Omit<PersistedKild, 'log'> & { log: Array<MessageDraft & { seq?: number }> }) | undefined {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return undefined;
  const data = raw as Record<string, unknown>;
  if (typeof data.id !== 'string' || !data.id) return undefined;
  if (data.log !== undefined && !Array.isArray(data.log)) return undefined;
  const log = (Array.isArray(data.log) ? data.log : []).filter(
    (message): message is MessageDraft & { seq?: number } =>
      typeof message === 'object' &&
      message !== null &&
      typeof (message as Message).text === 'string',
  );
  return { ...(data as unknown as Omit<PersistedKild, 'log'>), log };
}

/**
 * In-memory store of live kilds, with write-through persistence of each kild's
 * message history to `$KILD_HOME/kilds/<id>.json`. Live behaviour (delivery/broadcast)
 * lives in the manager — this only holds state and the on-disk mirror.
 *
 * Membership here IS liveness: a kild in this map is running, a kild in the archive is
 * stopped. There is no state field, because there is no third answer.
 *
 * Persistence is **history only**: on construction we load past kilds into a
 * separate `archive` map. An owned agent is a subprocess that dies with the engine,
 * so a restored kild has no live agents — it is a read-only transcript, never a
 * resumable session. We write only kilds that have at least one message, so
 * empty/never-used kilds leave no history clutter.
 */
export class KildRegistry {
  private readonly kilds = new Map<string, Kild>();
  /** Past kilds recovered from disk (read-only logs), loaded on first read. */
  private readonly archive = new Map<string, ArchivedKild>();
  private archiveLoaded = false;

  /**
   * Where history lives — read per call, never captured.
   *
   * It used to be a field initializer, which meant `$KILD_HOME` was read at the moment
   * this module was first imported and then frozen for the life of the process. That is a
   * fine assumption for the engine (one process, one env) and a wrong one for anything
   * that imports it twice with different state: the first importer silently decided where
   * every later one would look. Bun shares the module registry across test files, so the
   * archive tests passed or failed on file ORDER — green on one filesystem's readdir
   * order, red on another's, with nothing in the diff to explain it.
   */
  private get dir(): string {
    return path.join(kildHome(), 'kilds');
  }

  /** Load history the first time anything asks for it, rather than at construction, for
   *  the same reason: construction happens at import, and the answer depends on state the
   *  caller may not have set yet. */
  private ensureArchive(): Map<string, ArchivedKild> {
    if (!this.archiveLoaded) {
      this.archiveLoaded = true;
      this.loadArchive();
    }
    return this.archive;
  }

  create(kild: Kild): void {
    this.kilds.set(kild.id, kild);
  }

  get(kildId: string): Kild | undefined {
    return this.kilds.get(kildId);
  }

  /** Drop the live kild — leaving the registry IS being stopped. If it had any history,
   *  move it into the in-memory archive immediately (and return the snapshot) so it shows
   *  as read-only history without waiting for the next engine start. */
  remove(kildId: string): ArchivedKild | undefined {
    const kild = this.kilds.get(kildId);
    this.kilds.delete(kildId);
    if (!kild || kild.log.length === 0) return undefined;
    const archived = this.snapshot(kild);
    this.ensureArchive().set(kild.id, archived);
    this.saveArchived(archived);
    return archived;
  }

  /** Find which kild + agent a session belongs to — the reverse lookup that routes an
   *  agent's `send` / `spawn` back to its kild. Owned only, by construction: a session id
   *  is exactly the thing an attached agent does not have. */
  locateAgent(agentId: string): { kild: Kild; agent: OwnedAgent } | undefined {
    for (const kild of this.kilds.values()) {
      const agent = kild.agents.find(
        (a): a is OwnedAgent => a.ownership !== 'attached' && a.id === agentId,
      );
      if (agent) return { kild, agent };
    }
    return undefined;
  }

  /** Append a message to a kild's log, stamping its cursor. The registry assigns `seq`
   *  because the registry owns the log: the cursor is a property of the ORDER, and nothing
   *  else is in a position to guarantee it. Returns the stamped message so the caller
   *  broadcasts exactly what was recorded (a WS `{message}` frame carries the same `seq` a
   *  later `?since=` read will report). Undefined when the kild is gone. */
  appendMessage(kildId: string, draft: MessageDraft): Message | undefined {
    const kild = this.kilds.get(kildId);
    if (!kild) return undefined;
    // Derived from the log's tail, never from a counter held beside it: the log IS the
    // record, so a reloaded log reloads its cursor with it. Append-only ⇒ strictly
    // increasing.
    const message: Message = { ...draft, seq: (kild.log.at(-1)?.seq ?? 0) + 1 };
    kild.log.push(message);
    this.save(kild); // write-through: the log (and current agent snapshot) to disk
    return message;
  }

  /** Re-persist a kild's snapshot after out-of-band metadata changes (e.g. an agent's
   *  pi session identity arriving after the last message). No-op for message-less kilds. */
  persistNow(kildId: string): void {
    const kild = this.kilds.get(kildId);
    if (kild) this.save(kild);
  }

  summaries(): KildSummary[] {
    return [...this.kilds.values()].map((k) => ({
      id: k.id,
      name: k.name,
      worktree: k.worktree,
      agents: k.agents.map(agentView),
    }));
  }

  /** Full live Kild objects (with cwd + worktree) — the manager needs these to compute
   *  per-kild git status, which the ArchivedKild snapshot deliberately drops. */
  liveKildObjects(): Kild[] {
    return [...this.kilds.values()];
  }

  /** Live kilds as identity + structure only: no git, no cost, no log. The cheap listing. */
  liveIdentities(): KildIdentity[] {
    return [...this.kilds.values()].map(kildIdentity);
  }

  /** One kild's message log — live or archived, since an archived kild's log is exactly
   *  what it still is: the read-only record. Undefined for an id that is neither. */
  log(kildId: string): Message[] | undefined {
    return this.kilds.get(kildId)?.log ?? this.ensureArchive().get(kildId)?.log;
  }

  /** Live kilds with their full logs — lets a client joining mid-kild (or after a
   *  refresh) load the conversation so far. Same shape as an archived snapshot. */
  liveWithLogs(): ArchivedKild[] {
    return [...this.kilds.values()].map((k) => ({
      id: k.id,
      name: k.name,
      worktree: k.worktree,
      agents: k.agents.map(agentView),
      log: k.log,
      cwd: k.cwd,
      base: k.base,
      landedSha: k.landedSha,
    }));
  }

  /** Past kilds (read-only logs) recovered from disk at startup. */
  archived(): ArchivedKild[] {
    return [...this.ensureArchive().values()];
  }

  /** Serialise a kild's history. Kilds with no messages are not worth persisting. */
  private save(kild: Kild): void {
    if (kild.log.length === 0) return;
    this.persist(kild.id, this.snapshot(kild));
  }

  private snapshot(kild: Kild): ArchivedKild {
    return {
      id: kild.id,
      name: kild.name,
      worktree: kild.worktree,
      agents: kild.agents.map(agentView),
      log: kild.log,
      cwd: kild.cwd,
      base: kild.base,
      landedSha: kild.landedSha,
    };
  }

  private saveArchived(kild: ArchivedKild): void {
    this.persist(kild.id, kild);
  }

  private persist(kildId: string, data: ArchivedKild): void {
    fs.mkdirSync(this.dir, { recursive: true });
    const target = path.join(this.dir, `${kildId}.json`);
    const temp = `${target}.${process.pid}.${Date.now()}.tmp`;
    try {
      fs.writeFileSync(temp, JSON.stringify(data));
      fs.renameSync(temp, target);
    } catch (err) {
      try {
        if (fs.existsSync(temp)) fs.rmSync(temp);
      } catch {
        // Prefer the original persistence failure if temp cleanup also fails.
      }
      throw new Error(
        `kild: failed to persist kild ${kildId}: ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  private loadArchive(): void {
    let files: string[];
    try {
      files = fs.readdirSync(this.dir);
    } catch {
      return; // no history dir yet → no history
    }
    for (const file of files) {
      if (!file.endsWith('.json')) continue;
      const path_ = path.join(this.dir, file);
      try {
        const data = decodePersisted(JSON.parse(fs.readFileSync(path_, 'utf8')));
        if (!data) {
          // Named, not swallowed. A dropped archive and "this kild never existed" are the
          // same answer to every reader downstream, so the only place the difference can
          // still be told is here.
          console.error(`kild: skipping unreadable history file ${path_}`);
          continue;
        }
        // Stored order IS arrival order, so position reconstructs the cursor for any
        // message written before `seq` existed. Everything downstream can then treat the
        // cursor as always-present, including `?since=` on an old archived kild.
        const log: Message[] = data.log.map((message, index) => ({
          ...message,
          seq: message.seq ?? index + 1,
        }));
        this.archive.set(data.id, { ...data, log } as ArchivedKild);
      } catch (err) {
        // A corrupt/partial history file must not crash startup — but it says so.
        console.error(
          `kild: skipping unreadable history file ${path_}: ${err instanceof Error ? err.message : err}`,
        );
      }
    }
  }
}
