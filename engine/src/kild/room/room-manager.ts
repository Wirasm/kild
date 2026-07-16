import { randomUUID } from 'node:crypto';

import { sessionManager } from '../sessions.ts';
import { parseMentions } from './parse-mentions.ts';
import { RoomRegistry } from './room-registry.ts';
import { type RoomDelivery, routeRoomMessage } from './room-router.ts';
import {
  type ArchivedRoom,
  HUMAN,
  type OpenRoomSpec,
  type ParticipantSpec,
  type Room,
  type RoomMessage,
  type RoomOutbound,
} from './room-types.ts';

/** Soft cap on room size — a cheap loop/scale guard in v1 (loop control is otherwise
 *  just the human kill switch). */
const MAX_PARTICIPANTS = 8;

/**
 * Owns live rooms: opens them (one session per participant), routes every post
 * (participant→participant as turns, everything to the human as a broadcast),
 * grows them (the human invite + an agent's `invite_agent`), and closes them (the
 * kill switch). Sits beside the SessionManager — participants ARE sessions, so the
 * SessionManager stays room-agnostic; it only forwards a participant's control
 * lines (`message_out` / `invite`) to the callbacks we hand it.
 */
class RoomManager {
  private readonly registry = new RoomRegistry();
  private readonly subscribers = new Set<(msg: RoomOutbound) => void>();

  constructor() {
    // Forward each participant's transcript (its UiEvent stream from the session
    // substrate) to room clients, tagged by room + participant — so the cockpit can
    // render per-participant working detail. The session bus stays internal.
    sessionManager.subscribe((msg) => {
      if (!('session' in msg)) return;
      const located = this.registry.locateSession(msg.session);
      if (located) {
        this.broadcast({
          room: located.room.id,
          participant: located.participant.name,
          event: msg.event,
        });
      }
    });
  }

  subscribe(fn: (msg: RoomOutbound) => void): () => void {
    this.subscribers.add(fn);
    fn({ rooms: this.registry.summaries() }); // catch the new client up
    return () => {
      this.subscribers.delete(fn);
    };
  }

  /** Open a room under a caller-supplied id, spawning a session per participant. */
  open(roomId: string, spec: OpenRoomSpec): void {
    const room: Room = {
      id: roomId,
      name: spec.name,
      cwd: spec.cwd,
      worktree: spec.worktree,
      participants: [],
      log: [],
    };
    this.registry.create(room);
    for (const p of spec.participants) this.spawnParticipant(room, p);
    this.broadcast({ rooms: this.registry.summaries() });
  }

  /** The human posts into the room (kick-off and steering). */
  postFromHuman(roomId: string, text: string): void {
    this.post(roomId, HUMAN, text);
  }

  /** An operator-side author (e.g. the brain) posts into a room — the agent-driven
   *  mirror of {@link postFromHuman}, routed identically. This is how the brain
   *  speaks into the real Room primitive instead of a separate bus. */
  postAs(roomId: string, from: string, text: string): void {
    this.post(roomId, from, text);
  }

  /** A room's shared log (empty if the room is unknown) — for demos / inspection. */
  messages(roomId: string): RoomMessage[] {
    return this.registry.get(roomId)?.log ?? [];
  }

  /** Past rooms recovered from disk (read-only logs from previous engine runs). */
  archived(): ArchivedRoom[] {
    return this.registry.archived();
  }

  /** Live rooms with their logs — for a client joining a room it didn't open (or
   *  reloading), so it can render the conversation so far. */
  liveRooms(): ArchivedRoom[] {
    return this.registry.liveWithLogs();
  }

  /** Add a participant to a live room (the human "invite"). */
  addParticipant(roomId: string, spec: ParticipantSpec): void {
    const room = this.registry.get(roomId);
    if (!room || !this.spawnParticipant(room, spec)) return;
    this.broadcast({ rooms: this.registry.summaries() });
    this.post(roomId, HUMAN, `@${spec.name} joined the room.`, { system: true });
  }

  /** Stop every participant session — the human kill switch / room teardown. A room
   *  with history moves straight into the archive and is pushed to clients, so it stays
   *  visible as a read-only transcript without an engine restart. */
  close(roomId: string): void {
    const room = this.registry.get(roomId);
    if (!room) return;
    for (const p of room.participants) sessionManager.stop(p.sessionId);
    const archived = this.registry.remove(roomId);
    if (archived) this.broadcast({ archivedRoom: archived });
    this.broadcast({ rooms: this.registry.summaries() });
  }

  /** Manual circuit breaker: stop every participant session but KEEP the room, so its
   *  transcript stays visible (read-only). The operator trips this to halt a runaway or
   *  off-track room without tearing it down (vs {@link close}). Idempotent. */
  halt(roomId: string): void {
    const room = this.registry.get(roomId);
    if (!room || room.stopped) return;
    for (const p of room.participants) sessionManager.stop(p.sessionId);
    room.stopped = true;
    this.post(roomId, HUMAN, 'Room halted by the operator.', { system: true });
    this.broadcast({ rooms: this.registry.summaries() });
  }

  /** Spawn one participant session, wired so its control lines route back here.
   *  Returns false if rejected (name taken / reserved / at capacity). */
  private spawnParticipant(room: Room, spec: ParticipantSpec): boolean {
    if (spec.name === HUMAN) return false;
    if (room.participants.some((p) => p.name === spec.name)) return false;
    if (room.participants.length >= MAX_PARTICIPANTS) return false;
    const sessionId = randomUUID();
    room.participants.push({ name: spec.name, sessionId, agent: spec.agent });
    sessionManager.spawn(
      sessionId,
      {
        model: spec.model,
        cwd: room.cwd,
        agent: spec.agent,
        projectName: room.name,
        // Every participant attaches to the room's shared worktree, if any.
        worktree: room.worktree,
        // Opaque to the SessionManager; the worker reads these to register its room
        // tools (`post_message`, `invite_agent`) and tag its outbound control lines.
        env: { KILD_ROOM: room.id, KILD_PARTICIPANT: spec.name },
      },
      'cli',
      {
        onMessage: (m) => this.handleParticipantMessage(sessionId, m),
        onInvite: (i) => this.handleInvite(sessionId, i),
      },
    );
    return true;
  }

  /** A participant called `post_message`: resolve its room/name and route. */
  private handleParticipantMessage(
    sessionId: string,
    m: { text: string; to?: string[]; implicit?: boolean },
  ): void {
    const located = this.registry.locateSession(sessionId);
    if (!located) return; // not (or no longer) a participant
    this.post(located.room.id, located.participant.name, m.text, {
      to: m.to,
      implicit: m.implicit,
    });
  }

  /** A participant called `invite_agent`: add the named agent to its room. */
  private handleInvite(sessionId: string, spec: ParticipantSpec): void {
    const located = this.registry.locateSession(sessionId);
    if (located) this.addParticipant(located.room.id, spec);
  }

  /** Record + route one post from `from` (a participant name or {@link HUMAN}). */
  private post(
    roomId: string,
    from: string,
    text: string,
    opts: { to?: string[]; implicit?: boolean; system?: boolean } = {},
  ): void {
    const room = this.registry.get(roomId);
    if (!room) return;
    const message: RoomMessage = {
      id: randomUUID(),
      roomId,
      from,
      // The one place "who is this addressed to?" is answered: a notice addresses no
      // one, otherwise an explicit `to` wins, else the @mentions in the text. The router
      // consumes this verbatim — it must never re-derive addressees from the text.
      to: opts.system ? [] : (opts.to ?? parseMentions(text)),
      text,
      ts: Date.now(),
      implicit: opts.implicit,
      system: opts.system,
    };
    this.registry.appendMessage(roomId, message);
    routeRoomMessage(room, message, this.delivery());
  }

  private delivery(): RoomDelivery {
    return {
      deliverAsTurn: (sessionId, from, text) => sessionManager.prompt(sessionId, text, from),
      broadcast: (message) => this.broadcast({ roomMessage: message }),
    };
  }

  private broadcast(msg: RoomOutbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton, mirroring {@link sessionManager}. */
export const roomManager = new RoomManager();
