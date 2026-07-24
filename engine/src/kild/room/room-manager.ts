import { randomUUID } from 'node:crypto';

import { listAgents } from '../agents.ts';
import { configuredMemoryDir, configuredMemorySynthesis } from '../config.ts';
import { appendRoomLog, roomTranscriptPath, synthesisPrompt } from '../memory.ts';
import { type SessionCallbacks, type SpawnRequest, sessionManager } from '../sessions.ts';
import { resolveBaseBranch, worktreePath } from '../worktree.ts';
import { workstreamGitStatus } from '../worktree-status.ts';
import { applyDecisionMarkers, formatOpenDecisions, openDecisions } from './room-decisions.ts';
import {
  finalNonSystemPost,
  formatOperatorNotification,
  humanPostEvent,
  openerNotificationTarget,
} from './room-events.ts';
import {
  ensureRoomCanAddParticipant,
  ensureRoomCanCloseFromOperator,
  ensureRoomCanCloseFromParticipant,
  ensureRoomCanHalt,
  ensureRoomCanPost,
  transitionRoomState,
} from './room-lifecycle.ts';
import { RoomRegistry } from './room-registry.ts';
import { type RoomDelivery, routeRoomMessage, unknownRecipients } from './room-router.ts';
import {
  type ArchivedRoom,
  type CloseRoomOut,
  type CommandResult,
  HUMAN,
  type InviteOut,
  type LiveRoomStatus,
  type MessageOut,
  type OpenRoomSpec,
  type OpenRoomSuccess,
  type ParticipantSpec,
  participantView,
  type Room,
  type RoomActionSuccess,
  type RoomMessage,
  type RoomOutbound,
  type RoomParticipant,
} from './room-types.ts';

/** Soft cap on room size — a cheap loop/scale guard in v1 (loop control is otherwise
 *  just the human kill switch). */
const MAX_PARTICIPANTS = 8;

/** The single chokepoint every room post flows through: one structured trace line so a
 *  whole room reads back as an ordered log (grep `room.post`). Kept dead simple — swap
 *  the body for a real logger later without touching call sites. Programmatic tracers
 *  should instead subscribe to the manager (every post is broadcast as a `roomMessage`). */
function traceRoomPost(roomName: string, message: RoomMessage): void {
  console.error(
    JSON.stringify({
      t: 'room.post',
      room: roomName,
      from: message.from,
      to: message.to,
      system: message.system ?? false,
      implicit: message.implicit ?? false,
      chars: message.text.length,
    }),
  );
}

interface SessionRuntime {
  subscribe(
    fn: (msg: { session: string; event: unknown } | { sessions: unknown[] }) => void,
  ): () => void;
  spawn(id: string, req: SpawnRequest, origin?: 'ui' | 'cli', callbacks?: SessionCallbacks): void;
  prompt(id: string, text: string, from?: string): boolean;
  stop(id: string): void;
}

interface RoomManagerDeps {
  registry?: RoomRegistry;
  sessions?: SessionRuntime;
  listAgents?: typeof listAgents;
  createId?: () => string;
}

interface ValidatedParticipantSpec extends ParticipantSpec {
  resolvedAgent?: string;
}

function ok<T>(value: T): CommandResult<T> {
  return { ok: true, value };
}

function fail<T>(
  code: 'not_found' | 'invalid_state' | 'rejected',
  message: string,
): CommandResult<T> {
  return { ok: false, code, message };
}

/**
 * Owns live rooms: opens them (one session per participant), routes every post
 * (participant→participant as turns, everything to the human as a broadcast),
 * grows them (the human invite + an agent's `invite_agent`), and closes them (the
 * kill switch). Sits beside the SessionManager — participants ARE sessions, so the
 * SessionManager stays room-agnostic; it only forwards a participant's control
 * lines (`message_out` / `invite`) to the callbacks we hand it.
 */
export class RoomManager {
  private readonly registry: RoomRegistry;
  private readonly sessions: SessionRuntime;
  private readonly resolveAgents: typeof listAgents;
  private readonly createId: () => string;
  private readonly subscribers = new Set<(msg: RoomOutbound) => void>();

  constructor(deps: RoomManagerDeps = {}) {
    this.registry = deps.registry ?? new RoomRegistry();
    this.sessions = deps.sessions ?? sessionManager;
    this.resolveAgents = deps.listAgents ?? listAgents;
    this.createId = deps.createId ?? randomUUID;

    // Forward each participant's transcript (its UiEvent stream from the session
    // substrate) to room clients, tagged by room + participant — so the cockpit can
    // render per-participant working detail. The session bus stays internal.
    this.sessions.subscribe((msg) => {
      if (!('session' in msg)) return;
      const located = this.registry.locateSession(msg.session);
      if (located) {
        // Capture the provider-resolved model so observers see what each agent actually
        // ran on (not just the requested ref — which may have been a default/alias).
        const event = msg.event as {
          kind?: string;
          provider?: string;
          id?: string;
          file?: string;
        };
        if (event.kind === 'model' && event.provider && event.id) {
          located.participant.model = `${event.provider}/${event.id}`;
        }
        // The pi session identity is the participant's durable terminal-resume handle;
        // persist it so archived rooms keep it too (there may be no later post to piggyback
        // the snapshot on).
        if (event.kind === 'pi_session' && event.id) {
          located.participant.piSessionId = event.id;
          located.participant.piSessionFile = event.file;
          this.registry.persistNow(located.room.id);
        }
        // Failsafe on a finished turn: if a delegate went idle WITHOUT posting, its inviter
        // is blind (an explicit post would have woken the inviter via routing). Nudge the
        // delegate to report. A delegate that DID post needs nothing — the post is the signal.
        if (event.kind === 'agent_end') {
          this.nudgeIfIdleWithoutReport(located.participant);
        }
        this.broadcast({
          room: located.room.id,
          participant: located.participant.name,
          event: msg.event as never,
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
  async open(roomId: string, spec: OpenRoomSpec): Promise<CommandResult<OpenRoomSuccess>> {
    if (this.registry.get(roomId)) {
      return fail('rejected', `duplicate room id: ${roomId}`);
    }
    const validated = await this.validateParticipants(spec.cwd, spec.participants, []);
    if (!validated.ok) return validated;

    const room: Room = {
      id: roomId,
      name: spec.name,
      cwd: spec.cwd,
      worktree: spec.worktree,
      // Resolve the base once here — the single chokepoint every opener (CLI, REST, WS,
      // fleet tool) flows through: explicit `base` wins, else the cwd's configured
      // `baseBranch`, else its current branch, else `main`.
      base: await resolveBaseBranch(spec.cwd, spec.base),
      openedBy: spec.openedBy,
      participants: [],
      log: [],
      state: 'opening',
    };
    this.registry.create(room);

    const spawnedSessionIds: string[] = [];
    for (const participant of validated.value) {
      const result = this.spawnParticipant(room, participant, HUMAN);
      if (!result.ok) {
        this.rollbackOpen(roomId, spawnedSessionIds);
        return result;
      }
      spawnedSessionIds.push(result.value.sessionId);
    }

    const transitioned = transitionRoomState(room, 'running');
    if (!transitioned.ok) {
      this.rollbackOpen(roomId, spawnedSessionIds);
      return transitioned;
    }

    this.broadcast({ rooms: this.registry.summaries() });
    return ok({ roomId, message: `Room '${spec.name}' opened.` });
  }

  /** The human posts into the room (kick-off and steering). `to` is optional — omit it
   *  to address the room lead by default. */
  async postFromHuman(
    roomId: string,
    text: string,
    to?: string[],
  ): Promise<CommandResult<RoomActionSuccess>> {
    return this.post(roomId, HUMAN, text, { to });
  }

  /** An operator-side author (e.g. the brain) posts into a room — the agent-driven
   *  mirror of {@link postFromHuman}, routed identically. This is how the brain
   *  speaks into the real Room primitive instead of a separate bus. `to` is optional —
   *  omit it to address the room lead by default. */
  async postAs(
    roomId: string,
    from: string,
    text: string,
    to?: string[],
  ): Promise<CommandResult<RoomActionSuccess>> {
    return this.post(roomId, from, text, { to });
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

  /** Live rooms enriched with each workstream's git/worktree state — the code-state
   *  half of observability, so a driving agent can land work and spot collisions.
   *  Effective dir = the room's worktree if set, else its cwd. Git failures are
   *  captured per-room (never thrown), so status stays available even mid-conflict. */
  async liveRoomsStatus(): Promise<LiveRoomStatus[]> {
    return Promise.all(
      this.registry.liveRoomObjects().map(async (room) => ({
        id: room.id,
        name: room.name,
        worktree: room.worktree,
        participants: room.participants.map(participantView),
        state: room.state,
        log: room.log,
        decisions: room.decisions,
        git: await workstreamGitStatus(
          room.worktree ? worktreePath(room.worktree) : room.cwd,
          room.base,
        ),
      })),
    );
  }

  /** The effective workstream dir (the room's worktree if set, else its cwd) + base of
   *  ONE live room — the same resolution {@link liveRoomsStatus} uses per room, exposed
   *  so the review endpoints can drill into a single room without probing every room's
   *  git state. Live rooms only: an archived room's participants are gone and its
   *  worktree may be pruned, so there is no workstream to inspect (`invalid_state`);
   *  an id that was never a room is `not_found`. */
  workstreamDir(roomId: string): CommandResult<{ dir: string; base?: string }> {
    const room = this.registry.get(roomId);
    if (room) {
      return ok({ dir: room.worktree ? worktreePath(room.worktree) : room.cwd, base: room.base });
    }
    if (this.registry.archived().some((archived) => archived.id === roomId)) {
      return fail('invalid_state', `room ${roomId} is archived; its workstream is gone`);
    }
    return fail('not_found', `no such live room: ${roomId}`);
  }

  /** Add a participant to a live room. `invitedBy` is the inviter's name (default
   *  {@link HUMAN} for the operator's manual invite). */
  async addParticipant(
    roomId: string,
    spec: ParticipantSpec,
    invitedBy: string = HUMAN,
  ): Promise<CommandResult<RoomActionSuccess>> {
    const room = this.registry.get(roomId);
    if (!room) return fail('not_found', `no such room: ${roomId}`);
    const allowed = ensureRoomCanAddParticipant(room);
    if (!allowed.ok) return allowed;

    const validated = await this.validateParticipants(room.cwd, [spec], room.participants);
    if (!validated.ok) return validated;

    const result = this.spawnParticipant(
      room,
      validated.value[0] as ValidatedParticipantSpec,
      invitedBy,
    );
    if (!result.ok) return result;
    this.broadcast({ rooms: this.registry.summaries() });
    await this.post(roomId, HUMAN, `@${spec.name} joined the room.`, {
      system: true,
      allowStopped: true,
    });
    return ok({ message: `Invited @${spec.name} to the room.` });
  }

  /** Stop every participant session — the human kill switch / room teardown. A room
   *  with history moves straight into the archive and is pushed to clients, so it stays
   *  visible as a read-only transcript without an engine restart.
   *
   *  Open decisions block the close (the fold's whole guarantee: a raised decision
   *  cannot leave the system silently). `force` is the operator's escape hatch — it is
   *  never offered to the in-room lead path ({@link handleCloseRoom}). */
  async close(
    roomId: string,
    opts: { force?: boolean } = {},
  ): Promise<CommandResult<RoomActionSuccess>> {
    const room = this.registry.get(roomId);
    if (!room) return fail('not_found', `no such room: ${roomId}`);
    const allowed = ensureRoomCanCloseFromOperator(room);
    if (!allowed.ok) return allowed;
    if (!opts.force && openDecisions(room).length > 0) {
      return fail(
        'rejected',
        `room '${room.name}' has open decisions: ${formatOpenDecisions(room)}. ` +
          `Resolve each (post 'resolved[<key>]: <how>') or close with force.`,
      );
    }
    for (const participant of room.participants) this.sessions.stop(participant.sessionId);
    const transitioned = transitionRoomState(room, 'closed');
    if (!transitioned.ok) return transitioned;
    const archived = this.registry.remove(roomId);
    this.notifyOpener(room, {
      kind: 'closed',
      finalPost: finalNonSystemPost(room),
    });
    if (archived) this.broadcast({ archivedRoom: archived });
    this.broadcast({ rooms: this.registry.summaries() });
    if (archived) await this.recordMemory(room);
    return ok({ message: `Room '${room.name}' closed.` });
  }

  /** Manual circuit breaker: stop every participant session but KEEP the room, so its
   *  transcript stays visible (read-only). The operator trips this to halt a runaway or
   *  off-track room without tearing it down (vs {@link close}). */
  async halt(roomId: string): Promise<CommandResult<RoomActionSuccess>> {
    const room = this.registry.get(roomId);
    if (!room) return fail('not_found', `no such room: ${roomId}`);
    const allowed = ensureRoomCanHalt(room);
    if (!allowed.ok) return allowed;
    for (const participant of room.participants) this.sessions.stop(participant.sessionId);
    const transitioned = transitionRoomState(room, 'halted');
    if (!transitioned.ok) return transitioned;
    await this.post(roomId, HUMAN, 'Room halted by the operator.', {
      system: true,
      allowStopped: true,
    });
    this.notifyOpener(room, {
      kind: 'halted',
      finalPost: finalNonSystemPost(room),
    });
    this.broadcast({ rooms: this.registry.summaries() });
    return ok({ message: `Room '${room.name}' halted.` });
  }

  /** Post-close memory hook: append the engine-written log entry (always), then spawn
   *  the optional synthesis session (config `memory.synthesis`) to distill the transcript
   *  into the memory dir's `MEMORY.md` (config `memory.dir`, default `.kild/`). Memory
   *  must never break a close — failures are logged loud and swallowed here, at the one
   *  boundary where that is the right call. */
  private async recordMemory(room: Room): Promise<void> {
    let memoryDir: string;
    try {
      memoryDir = await configuredMemoryDir(room.cwd);
      appendRoomLog(room, memoryDir);
    } catch (err) {
      console.error(
        `kild: room log append failed for '${room.name}': ${err instanceof Error ? err.message : err}`,
      );
      return; // no log entry → don't synthesize against a missing input
    }
    try {
      const synthesis = await configuredMemorySynthesis(room.cwd);
      if (!synthesis) return;
      const id = this.createId();
      this.sessions.spawn(id, {
        model: synthesis.model,
        cwd: room.cwd, // the MAIN checkout — memory files are gitignored, so worktrees never see them
        agent: synthesis.agent ?? 'default',
        projectName: `memory:${room.name}`,
      });
      this.sessions.prompt(
        id,
        synthesisPrompt(room, roomTranscriptPath(room.id), memoryDir),
        'kild',
      );
    } catch (err) {
      console.error(
        `kild: memory synthesis spawn failed for '${room.name}': ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  /** Spawn one participant session, wired so its control lines route back here.
   *  `invitedBy` is the inviter's name (a live participant) or {@link HUMAN} for the
   *  opener's initial roster — the ground-truth spawn edge + idle-notice target. */
  private spawnParticipant(
    room: Room,
    spec: ValidatedParticipantSpec,
    invitedBy: string,
  ): CommandResult<{ sessionId: string }> {
    const isLead = room.participants.length === 0; // first participant leads the room
    const sessionId = this.createId();
    // Record the requested model now (visible immediately); the session's `model` event
    // upgrades it to the provider-resolved ref once it starts.
    room.participants.push({
      name: spec.name,
      sessionId,
      agent: spec.agent,
      model: spec.model,
      invitedBy,
    });
    try {
      this.sessions.spawn(
        sessionId,
        {
          model: spec.model,
          cwd: room.cwd,
          agent: spec.resolvedAgent,
          projectName: room.name,
          // Every participant attaches to the room's shared worktree, if any.
          worktree: room.worktree,
          // Base branch a brand-new worktree forks from (the first participant creates it).
          base: room.base,
          // Opaque to the SessionManager; the worker reads these to register its room
          // tools (`post_message`, `invite_agent`, and — lead only — `close_room`) and
          // tag its outbound control lines.
          env: {
            KILD_ROOM: room.id,
            KILD_PARTICIPANT: spec.name,
            ...(isLead ? { KILD_ROOM_LEAD: '1' } : {}),
          },
        },
        'cli',
        {
          onMessage: (m) => this.handleParticipantMessage(sessionId, m),
          onInvite: (i) => this.handleInvite(sessionId, i),
          onCloseRoom: (c) => this.handleCloseRoom(sessionId, c),
        },
      );
    } catch (err) {
      room.participants = room.participants.filter(
        (participant) => participant.sessionId !== sessionId,
      );
      return fail('rejected', err instanceof Error ? err.message : String(err));
    }
    return ok({ sessionId });
  }

  /** A participant called `post_message`: resolve its room/name and route. */
  private async handleParticipantMessage(
    sessionId: string,
    m: MessageOut,
  ): Promise<CommandResult<RoomActionSuccess>> {
    const located = this.registry.locateSession(sessionId);
    if (!located) return fail('not_found', `session '${sessionId}' is not in a live room`);
    const result = await this.post(located.room.id, located.participant.name, m.text, {
      to: m.to,
      implicit: m.implicit,
    });
    // An explicit post counts as reporting ONLY if it actually reached someone else —
    // a rejected post (unknown recipient) or a self-addressed one leaves whoever is
    // waiting just as blind as silence, so the idle failsafe must still fire.
    if (!m.implicit && result.ok && (result.value.deliveredTo?.length ?? 0) > 0) {
      located.participant.posted = true;
    }
    return result;
  }

  /** A participant called `invite_agent`: add the named agent to its room. */
  private async handleInvite(
    sessionId: string,
    spec: InviteOut,
  ): Promise<CommandResult<RoomActionSuccess>> {
    const located = this.registry.locateSession(sessionId);
    if (!located) return fail('not_found', `session '${sessionId}' is not in a live room`);
    // The inviter is the calling participant — recorded as the new agent's `invitedBy`,
    // so its idle/done notice routes back here (hierarchical delegation signalling).
    return this.addParticipant(
      located.room.id,
      { name: spec.name, agent: spec.agent, model: spec.model },
      located.participant.name,
    );
  }

  /** The room's lead called `close_room`: notice, then teardown. Only the lead holds
   *  the tool (worker-side), but enforce it here too — a control line is just stdout,
   *  so the engine, not the subprocess, is the authority on who may end a room. */
  private async handleCloseRoom(
    sessionId: string,
    closeSpec: CloseRoomOut,
  ): Promise<CommandResult<RoomActionSuccess>> {
    const located = this.registry.locateSession(sessionId);
    if (!located) return fail('not_found', `session '${sessionId}' is not in a live room`);
    const { room, participant } = located;
    const allowed = ensureRoomCanCloseFromParticipant(room);
    if (!allowed.ok) return allowed;
    if (room.participants[0]?.sessionId !== sessionId) {
      return fail('rejected', `only the lead may close room '${room.name}'`);
    }
    // No force on the participant path: an agent may not bury a raised decision. Only
    // the operator (human/brain) can force-close past open decisions.
    if (openDecisions(room).length > 0) {
      return fail(
        'rejected',
        `room '${room.name}' has open decisions: ${formatOpenDecisions(room)}. ` +
          `Get each resolved (a 'resolved[<key>]: <how>' post) before closing; ` +
          `only the operator may force-close past them.`,
      );
    }
    await this.post(
      room.id,
      HUMAN,
      `Room closed by @${participant.name}${closeSpec.reason ? `: ${closeSpec.reason}` : '.'}`,
      {
        system: true,
        allowStopped: true,
      },
    );
    return this.close(room.id);
  }

  /** Record + route one post from `from` (a participant name or {@link HUMAN}).
   *
   * Addressing is structured, never parsed from prose — the ONE rule: a system notice
   * targets no one; otherwise an explicit `to` wins; otherwise the post goes to the room
   * lead (the orchestrator). A typo'd handle is returned as a clean error to the caller
   * (the calling agent's tool result), so it can correct itself — it is never recorded,
   * routed, or turned into room spam. */
  private async post(
    roomId: string,
    from: string,
    text: string,
    opts: { to?: string[]; implicit?: boolean; system?: boolean; allowStopped?: boolean } = {},
  ): Promise<CommandResult<RoomActionSuccess>> {
    const room = this.registry.get(roomId);
    if (!room) return fail('not_found', `no such room: ${roomId}`);
    const allowed = ensureRoomCanPost(room, { allowHalted: opts.allowStopped });
    if (!allowed.ok) return allowed;

    const lead = room.participants[0]?.name;
    const to = opts.system ? [] : opts.to?.length ? opts.to : lead ? [lead] : [];
    const message: RoomMessage = {
      id: this.createId(),
      roomId,
      from,
      to,
      text,
      ts: Date.now(),
      implicit: opts.implicit,
      system: opts.system,
    };

    const unknown = unknownRecipients(room, message);
    if (unknown.length > 0) {
      const known = room.participants.map((participant) => `@${participant.name}`).join(', ');
      return fail(
        'rejected',
        `no such participant: ${unknown.map((recipient) => `@${recipient}`).join(', ')} ` +
          `(in the room: ${known || 'none'})`,
      );
    }

    // Fold decision markers BEFORE the append — appendMessage's write-through snapshot
    // then persists the updated ledger together with the post that changed it.
    applyDecisionMarkers(room, message);
    traceRoomPost(room.name, message);
    this.registry.appendMessage(roomId, message);
    routeRoomMessage(room, message, this.delivery());
    if (
      !message.system &&
      !message.implicit &&
      message.to.includes(HUMAN) &&
      room.participants.some((participant) => participant.name === message.from)
    ) {
      this.notifyOpener(room, humanPostEvent(message));
    }
    return ok({ message: 'Posted to the room.', deliveredTo: to.filter((t) => t !== from) });
  }

  private async validateParticipants(
    cwd: string,
    participants: ParticipantSpec[],
    existing: Array<{ name: string }>,
  ): Promise<CommandResult<ValidatedParticipantSpec[]>> {
    if (existing.length + participants.length > MAX_PARTICIPANTS) {
      return fail('rejected', `room capacity exceeded (max ${MAX_PARTICIPANTS} participants)`);
    }

    const knownAgents = new Set((await this.resolveAgents(cwd)).map((agent) => agent.name));
    const seenNames = new Set(existing.map((participant) => participant.name));
    const validated: ValidatedParticipantSpec[] = [];

    for (const spec of participants) {
      if (spec.name === HUMAN) {
        return fail('rejected', `participant name '${HUMAN}' is reserved`);
      }
      if (seenNames.has(spec.name)) {
        return fail('rejected', `duplicate participant: @${spec.name}`);
      }
      seenNames.add(spec.name);

      const resolvedAgent = spec.agent ?? spec.name;
      if (resolvedAgent !== 'default' && !knownAgents.has(resolvedAgent)) {
        return fail('rejected', `unknown agent: ${resolvedAgent}`);
      }
      validated.push({ ...spec, resolvedAgent });
    }

    return ok(validated);
  }

  private rollbackOpen(roomId: string, sessionIds: string[]): void {
    for (const sessionId of sessionIds) this.sessions.stop(sessionId);
    this.registry.remove(roomId);
  }

  /** Best-effort direct notification. It deliberately bypasses room posting/routing so an
   *  operator prompt can never become a room message or trigger an agent reply loop. */
  private notifyOpener(room: Room, event: Parameters<typeof formatOperatorNotification>[1]): void {
    const target = openerNotificationTarget(room);
    if (!target) return;
    this.sessions.prompt(target, formatOperatorNotification(room.name, event), 'kild');
  }

  /** Failsafe (NOT the default path): a participant that finishes a turn without a
   *  DELIVERED explicit post has left whoever is waiting blind — an actual post would have
   *  woken them via routing (or reached the operator channel for @human). Nudge it ONCE
   *  per active→idle transition to report: delegates toward their inviter, top-level
   *  participants toward @human — no one is assumed to be watching the roster (the
   *  operator is an agent by default; the cockpit human is the special case). Delivered
   *  as a direct session prompt (bypasses room routing, so it can't become a post or
   *  loop); a participant whose post DELIVERED gets nothing (its post is the signal). */
  private nudgeIfIdleWithoutReport(participant: RoomParticipant): void {
    if (participant.idle) return; // dedup: already handled this transition
    participant.idle = true;
    if (participant.posted) return; // it reported — a delivered post is the signal
    // Deliver signals, not sights: assume NO ONE is watching the roster — the operator is
    // an agent (a pi driver, another orchestrator) by default, and the cockpit human is
    // the special case. So top-level participants are nudged too, toward @human: unposted
    // work is invisible to every operator kind except a human who happens to be looking.
    const inviter = participant.invitedBy;
    const target = !inviter || inviter === HUMAN ? HUMAN : inviter;
    this.sessions.prompt(
      participant.sessionId,
      `[kild] You finished your turn without posting. If you have a result for @${target}, ` +
        `post_message it to them now (with evidence). If you are done with nothing to add, post ` +
        `a one-line status so @${target} isn't left waiting.`,
      'kild',
    );
  }

  private delivery(): RoomDelivery {
    return {
      deliverAsTurn: (sessionId, from, text) => {
        // A delivered turn reactivates the participant: clear idle + posted so its next
        // active→idle transition is judged fresh (did it report THIS turn?).
        const located = this.registry.locateSession(sessionId);
        if (located) {
          located.participant.idle = false;
          located.participant.posted = false;
        }
        this.sessions.prompt(sessionId, text, from);
      },
      broadcast: (message) => this.broadcast({ roomMessage: message }),
    };
  }

  private broadcast(msg: RoomOutbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton, mirroring {@link sessionManager}. */
export const roomManager = new RoomManager();
