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
      participants: r.participants.map((p) => ({ name: p.name, agent: p.agent })),
      state: r.state,
      stopped: r.state === 'halted',
    }));
  }

  /** Live rooms with their full logs — lets a client joining mid-room (or after a
   *  refresh) load the conversation so far. Same shape as an archived snapshot. */
  liveWithLogs(): ArchivedRoom[] {
    return [...this.rooms.values()].map((r) => ({
      id: r.id,
      name: r.name,
      worktree: r.worktree,
      participants: r.participants.map((p) => ({ name: p.name, agent: p.agent })),
      state: r.state,
      log: r.log,
    }));
  }

  /** Past rooms (read-only logs) recovered from disk at startup. */
  archived(): ArchivedRoom[] {
    return [...this.archive.values()];
  }

  /** Serialise a room's history. Best-effort: a write failure must never break a
   *  live room, and a room with no messages is not worth persisting. */
  private save(room: Room): void {
    if (room.log.length === 0) return;
    try {
      fs.mkdirSync(this.dir, { recursive: true });
      const data = this.snapshot(room, room.state);
      fs.writeFileSync(path.join(this.dir, `${room.id}.json`), JSON.stringify(data));
    } catch (err) {
      console.warn(
        `kild: failed to persist room ${room.id}: ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  private snapshot(room: Room, state: ArchivedRoom['state']): ArchivedRoom {
    return {
      id: room.id,
      name: room.name,
      worktree: room.worktree,
      participants: room.participants.map((p) => ({ name: p.name, agent: p.agent })),
      state,
      log: room.log,
    };
  }

  private saveArchived(room: ArchivedRoom): void {
    try {
      fs.mkdirSync(this.dir, { recursive: true });
      fs.writeFileSync(path.join(this.dir, `${room.id}.json`), JSON.stringify(room));
    } catch (err) {
      console.warn(
        `kild: failed to persist room ${room.id}: ${err instanceof Error ? err.message : err}`,
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
