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
  /** Past kilds recovered from disk at startup (read-only logs). */
  private readonly archive = new Map<string, ArchivedKild>();
  private readonly dir = path.join(kildHome(), 'kilds');

  constructor() {
    this.loadArchive();
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
    this.archive.set(kild.id, archived);
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
    return this.kilds.get(kildId)?.log ?? this.archive.get(kildId)?.log;
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
    return [...this.archive.values()];
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
      try {
        const data = JSON.parse(
          fs.readFileSync(path.join(this.dir, file), 'utf8'),
        ) as PersistedKild;
        if (!data?.id) continue;
        // Stored order IS arrival order, so position reconstructs the cursor for any
        // message written before `seq` existed. Everything downstream can then treat the
        // cursor as always-present, including `?since=` on an old archived kild.
        const log: Message[] = (data.log ?? []).map((message, index) => ({
          ...message,
          seq: message.seq ?? index + 1,
        }));
        this.archive.set(data.id, { ...data, log });
      } catch {
        // a corrupt/partial history file must not crash startup; skip it
      }
    }
  }
}
