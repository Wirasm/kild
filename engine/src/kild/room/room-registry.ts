import fs from 'node:fs';
import path from 'node:path';

import { kildHome } from '../config.ts';
import type {
  ArchivedRoom,
  Room,
  RoomMessage,
  RoomParticipant,
  RoomSummary,
} from './room-types.ts';

/**
 * In-memory store of live rooms, with write-through persistence of each room's
 * conversation log to `$KILD_HOME/rooms/<id>.json`. Live behaviour (delivery/
 * broadcast) lives in the router/manager — this only holds state and the on-disk
 * mirror.
 *
 * Persistence is **history only**: on construction we load past rooms into a
 * separate `archive` map. A participant is a worker subprocess that dies with the
 * engine, so a restored room has no live participants — it is a read-only transcript,
 * never a resumable session. We write only rooms that have at least one message, so
 * empty/never-used rooms leave no history clutter.
 */
export class RoomRegistry {
  private readonly rooms = new Map<string, Room>();
  /** Past rooms recovered from disk at startup (read-only logs). */
  private readonly archive = new Map<string, ArchivedRoom>();
  private readonly dir = path.join(kildHome(), 'rooms');

  constructor() {
    this.loadArchive();
  }

  create(room: Room): void {
    this.rooms.set(room.id, room);
  }

  get(roomId: string): Room | undefined {
    return this.rooms.get(roomId);
  }

  /** Drop the live room. If it had any history, move it into the in-memory archive
   *  immediately (and return the snapshot) so it shows as read-only history without
   *  waiting for the next engine start. Persist the final archived snapshot too, so a
   *  halted→closed transition survives restart/reload. */
  remove(roomId: string): ArchivedRoom | undefined {
    const room = this.rooms.get(roomId);
    this.rooms.delete(roomId);
    if (!room || room.log.length === 0) return undefined;
    const archived = this.snapshot(room, room.state);
    this.archive.set(room.id, archived);
    this.saveArchived(archived);
    return archived;
  }

  /** Find which room + participant a session belongs to — the reverse lookup that
   *  routes a participant's `message_out` / `invite` back to its room. */
  locateSession(sessionId: string): { room: Room; participant: RoomParticipant } | undefined {
    for (const room of this.rooms.values()) {
      const participant = room.participants.find((p) => p.sessionId === sessionId);
      if (participant) return { room, participant };
    }
    return undefined;
  }

  appendMessage(roomId: string, message: RoomMessage): void {
    const room = this.rooms.get(roomId);
    if (!room) return;
    room.log.push(message);
    this.save(room); // write-through: the log (and current participant snapshot) to disk
  }

  summaries(): RoomSummary[] {
    return [...this.rooms.values()].map((r) => ({
      id: r.id,
      name: r.name,
      worktree: r.worktree,
      participants: r.participants.map((p) => ({ name: p.name, agent: p.agent, model: p.model })),
      state: r.state,
      stopped: r.state === 'halted',
    }));
  }

  /** Full live Room objects (with cwd + worktree) — the manager needs these to compute
   *  per-workstream git status, which the ArchivedRoom snapshot deliberately drops. */
  liveRoomObjects(): Room[] {
    return [...this.rooms.values()];
  }

  /** Live rooms with their full logs — lets a client joining mid-room (or after a
   *  refresh) load the conversation so far. Same shape as an archived snapshot. */
  liveWithLogs(): ArchivedRoom[] {
    return [...this.rooms.values()].map((r) => ({
      id: r.id,
      name: r.name,
      worktree: r.worktree,
      participants: r.participants.map((p) => ({ name: p.name, agent: p.agent, model: p.model })),
      state: r.state,
      log: r.log,
    }));
  }

  /** Past rooms (read-only logs) recovered from disk at startup. */
  archived(): ArchivedRoom[] {
    return [...this.archive.values()];
  }

  /** Serialise a room's history. Rooms with no messages are not worth persisting. */
  private save(room: Room): void {
    if (room.log.length === 0) return;
    this.persist(room.id, this.snapshot(room, room.state));
  }

  private snapshot(room: Room, state: ArchivedRoom['state']): ArchivedRoom {
    return {
      id: room.id,
      name: room.name,
      worktree: room.worktree,
      participants: room.participants.map((p) => ({
        name: p.name,
        agent: p.agent,
        model: p.model,
      })),
      state,
      log: room.log,
    };
  }

  private saveArchived(room: ArchivedRoom): void {
    this.persist(room.id, room);
  }

  private persist(roomId: string, data: ArchivedRoom): void {
    fs.mkdirSync(this.dir, { recursive: true });
    const target = path.join(this.dir, `${roomId}.json`);
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
        `kild: failed to persist room ${roomId}: ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  private loadArchive(): void {
    let files: string[];
    try {
      files = fs.readdirSync(this.dir);
    } catch {
      return; // no rooms dir yet → no history
    }
    for (const file of files) {
      if (!file.endsWith('.json')) continue;
      try {
        const data = JSON.parse(fs.readFileSync(path.join(this.dir, file), 'utf8')) as ArchivedRoom;
        if (data?.id) this.archive.set(data.id, { ...data, state: data.state ?? 'closed' });
      } catch {
        // a corrupt/partial history file must not crash startup; skip it
      }
    }
  }
}
