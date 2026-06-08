import type { Room, RoomMessage, RoomParticipant, RoomSummary } from './room-types.ts';

/**
 * In-memory store of live rooms and their message logs — runtime state only, no
 * behaviour (delivery/broadcast live in the router/manager). We prove the model in
 * memory first; a JSON-file backing comes later, behind this same interface.
 */
export class RoomRegistry {
  private readonly rooms = new Map<string, Room>();

  create(room: Room): void {
    this.rooms.set(room.id, room);
  }

  get(roomId: string): Room | undefined {
    return this.rooms.get(roomId);
  }

  remove(roomId: string): void {
    this.rooms.delete(roomId);
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
    this.rooms.get(roomId)?.log.push(message);
  }

  summaries(): RoomSummary[] {
    return [...this.rooms.values()].map((r) => ({
      id: r.id,
      name: r.name,
      worktree: r.worktree,
      participants: r.participants.map((p) => ({ name: p.name, agent: p.agent })),
    }));
  }
}
