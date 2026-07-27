import { randomUUID } from 'node:crypto';
import { type AgentCallbacks, agentManager, type SpawnRequest } from './agent-manager.ts';
import { configuredMemoryDir, configuredMemorySynthesis } from './config.ts';
import { Inbox, type InboxDrain } from './inbox.ts';
import {
  finalNonSystemPost,
  formatOperatorNotification,
  humanPostEvent,
  openerNotificationTarget,
} from './kild-events.ts';
import {
  ensureKildCanHalt,
  ensureKildCanSend,
  ensureKildCanSpawnAgent,
  ensureKildCanStopFromAgent,
  ensureKildCanStopFromOperator,
  transitionKildState,
} from './kild-lifecycle.ts';
import { KildRegistry } from './kild-registry.ts';
import { type Delivery, routeMessage, unknownRecipients } from './kild-router.ts';
import {
  type AgentSpec,
  type ArchivedKild,
  agentProcessId,
  agentView,
  type CommandResult,
  costTotals,
  HUMAN,
  type Kild,
  type KildActionSuccess,
  type KildOutbound,
  type LiveKildStatus,
  type Message,
  type NewKildSpec,
  type NewKildSuccess,
  type SendOut,
  type SpawnOut,
  type StopOut,
} from './kild-types.ts';
import { appendKildLog, kildTranscriptPath, synthesisPrompt } from './memory.ts';
import { listPersonas } from './personas.ts';
import { resolveBaseBranch, worktreePath } from './worktree.ts';
import { kildGitStatus } from './worktree-status.ts';

/** Soft cap on kild size — a cheap loop/scale guard in v1 (loop control is otherwise
 *  just the human kill switch). */
const MAX_AGENTS = 8;

/** The single chokepoint every kild message flows through: one structured trace line so a
 *  whole kild reads back as an ordered log (grep `kild.send`). Kept dead simple — swap
 *  the body for a real logger later without touching call sites. Programmatic tracers
 *  should instead subscribe to the manager (every message is broadcast as a `message`). */
function traceSend(kildName: string, message: Message): void {
  console.error(
    JSON.stringify({
      t: 'kild.send',
      kild: kildName,
      from: message.from,
      to: message.to,
      system: message.system ?? false,
      chars: message.text.length,
    }),
  );
}

interface AgentRuntime {
  subscribe(
    fn: (msg: { agent: string; event: unknown } | { agents: unknown[] }) => void,
  ): () => void;
  spawn(id: string, req: SpawnRequest, origin?: 'ui' | 'cli', callbacks?: AgentCallbacks): void;
  prompt(id: string, text: string, from?: string): boolean;
  stop(id: string): void;
}

interface KildManagerDeps {
  registry?: KildRegistry;
  sessions?: AgentRuntime;
  listPersonas?: typeof listPersonas;
  createId?: () => string;
}

interface ValidatedAgentSpec extends AgentSpec {
  resolvedPersona?: string;
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
 * Owns live kilds: creates them (one session per agent), routes every message
 * (agent→agent as turns, everything to the human as a broadcast), grows them (the
 * human's spawn + an agent's `spawn` tool), and stops them (the kill switch). Sits
 * beside the AgentManager — agents ARE sessions, so the AgentManager stays
 * kild-agnostic; it only forwards an agent's control lines (`send` / `spawn`) to the
 * callbacks we hand it.
 */
export class KildManager {
  private readonly registry: KildRegistry;
  private readonly sessions: AgentRuntime;
  private readonly resolvePersonas: typeof listPersonas;
  private readonly createId: () => string;
  private readonly subscribers = new Set<(msg: KildOutbound) => void>();

  constructor(deps: KildManagerDeps = {}) {
    this.registry = deps.registry ?? new KildRegistry();
    this.sessions = deps.sessions ?? agentManager;
    this.resolvePersonas = deps.listPersonas ?? listPersonas;
    this.createId = deps.createId ?? randomUUID;

    // Forward each agent's transcript (its UiEvent stream from the session substrate)
    // to kild clients, tagged by kild + agent — so UI clients can render per-agent
    // working detail. The agent bus stays internal.
    this.sessions.subscribe((msg) => {
      if (!('agent' in msg)) return;
      const located = this.registry.locateAgent(msg.agent);
      if (located) {
        // Capture the provider-resolved model so observers see what each agent actually
        // ran on (not just the requested ref — which may have been a default/alias).
        const event = msg.event as {
          kind?: string;
          provider?: string;
          id?: string;
          file?: string;
          tokens?: number;
          cost?: number;
        };
        if (event.kind === 'model' && event.provider && event.id) {
          located.agent.model = `${event.provider}/${event.id}`;
        }
        // Capture the latest cumulative cost snapshot (emitted at every turn end) so
        // observers can rank kilds by spend without parsing transcripts. No persistNow:
        // the next message's write-through snapshot — or stop() — carries it to disk.
        if (
          event.kind === 'stats' &&
          typeof event.tokens === 'number' &&
          typeof event.cost === 'number'
        ) {
          located.agent.tokens = event.tokens;
          located.agent.cost = event.cost;
        }
        // The pi session identity is the agent's durable terminal-resume handle;
        // persist it so archived kilds keep it too (there may be no later message to
        // piggyback the snapshot on).
        if (event.kind === 'pi_session' && event.id) {
          located.agent.piSessionId = event.id;
          located.agent.piSessionFile = event.file;
          this.registry.persistNow(located.kild.id);
        }
        // A finished turn means the agent is waiting — observability only (it rides
        // AgentView so a client can see who is finished), never a prompt.
        if (event.kind === 'agent_end') {
          located.agent.idle = true;
        }
        this.broadcast({
          kild: located.kild.id,
          agent: located.agent.handle,
          event: msg.event as never,
        });
      }
    });
  }

  subscribe(fn: (msg: KildOutbound) => void): () => void {
    this.subscribers.add(fn);
    fn({ kilds: this.registry.summaries() }); // catch the new client up
    return () => {
      this.subscribers.delete(fn);
    };
  }

  /** Create a kild under a caller-supplied id, spawning a session per agent. */
  async create(kildId: string, spec: NewKildSpec): Promise<CommandResult<NewKildSuccess>> {
    if (this.registry.get(kildId)) {
      return fail('rejected', `duplicate kild id: ${kildId}`);
    }
    const validated = await this.validateAgents(spec.cwd, spec.agents, []);
    if (!validated.ok) return validated;

    const kild: Kild = {
      id: kildId,
      name: spec.name,
      cwd: spec.cwd,
      worktree: spec.worktree,
      // Resolve the base once here — the single chokepoint every creator (CLI, REST, WS)
      // flows through: explicit `base` wins, else the cwd's configured `baseBranch`, else
      // its current branch, else `main`.
      base: await resolveBaseBranch(spec.cwd, spec.base),
      openedBy: spec.openedBy,
      agents: [],
      log: [],
      state: 'opening',
    };
    this.registry.create(kild);

    const spawnedAgentIds: string[] = [];
    for (const agent of validated.value) {
      const result = this.startAgent(kild, agent, HUMAN);
      if (!result.ok) {
        this.rollbackCreate(kildId, spawnedAgentIds);
        return result;
      }
      spawnedAgentIds.push(result.value.agentId);
    }

    const transitioned = transitionKildState(kild, 'running');
    if (!transitioned.ok) {
      this.rollbackCreate(kildId, spawnedAgentIds);
      return transitioned;
    }

    this.broadcast({ kilds: this.registry.summaries() });
    return ok({ kildId, message: `Kild '${spec.name}' created.` });
  }

  /** The human sends into the kild (kick-off and steering). `to` is optional — omit it
   *  to address the kild lead by default. */
  async sendFromHuman(
    kildId: string,
    text: string,
    to?: string[],
  ): Promise<CommandResult<KildActionSuccess>> {
    return this.send(kildId, HUMAN, text, { to });
  }

  /** An operator-side author (e.g. the brain) sends into a kild — the agent-driven
   *  mirror of {@link sendFromHuman}, routed identically. This is how the brain
   *  speaks into the real Kild primitive instead of a separate bus. `to` is optional —
   *  omit it to address the kild lead by default. */
  async sendAs(
    kildId: string,
    from: string,
    text: string,
    to?: string[],
  ): Promise<CommandResult<KildActionSuccess>> {
    return this.send(kildId, from, text, { to });
  }

  /** A kild's shared log (empty if the kild is unknown) — for demos / inspection. */
  messageLog(kildId: string): Message[] {
    return this.registry.get(kildId)?.log ?? [];
  }

  /** Past kilds recovered from disk (read-only logs from previous engine runs). */
  archived(): ArchivedKild[] {
    return this.registry.archived();
  }

  /** Live kilds with their logs — for a client joining a kild it didn't create (or
   *  reloading), so it can render the conversation so far. */
  liveKilds(): ArchivedKild[] {
    return this.registry.liveWithLogs();
  }

  /** Live kilds enriched with each kild's git/worktree state — the code-state
   *  half of observability, so a driving agent can land work and spot collisions.
   *  Effective dir = the kild's worktree if set, else its cwd. Git failures are
   *  captured per-kild (never thrown), so status stays available even mid-conflict. */
  async liveKildsStatus(): Promise<LiveKildStatus[]> {
    return Promise.all(
      this.registry.liveKildObjects().map(async (kild) => ({
        id: kild.id,
        name: kild.name,
        worktree: kild.worktree,
        agents: kild.agents.map(agentView),
        state: kild.state,
        log: kild.log,
        totals: costTotals(kild.agents),
        git: await kildGitStatus(kild.worktree ? worktreePath(kild.worktree) : kild.cwd, kild.base),
      })),
    );
  }

  /** The effective kild dir (the kild's worktree if set, else its cwd) + base of
   *  ONE live kild — the same resolution {@link liveKildsStatus} uses per kild, exposed
   *  so the review endpoints can drill into a single kild without probing every kild's
   *  git state. Live kilds only: an archived kild's agents are gone and its worktree may
   *  be pruned, so there is no working dir to inspect (`invalid_state`); an id that was
   *  never a kild is `not_found`. */
  kildDir(kildId: string): CommandResult<{ dir: string; base?: string }> {
    const kild = this.registry.get(kildId);
    if (kild) {
      return ok({ dir: kild.worktree ? worktreePath(kild.worktree) : kild.cwd, base: kild.base });
    }
    if (this.registry.archived().some((archived) => archived.id === kildId)) {
      return fail('invalid_state', `kild ${kildId} is archived; its working dir is gone`);
    }
    return fail('not_found', `no such live kild: ${kildId}`);
  }

  /** Spawn an agent into a live kild. `invitedBy` is the spawner's handle (default
   *  {@link HUMAN} for the operator's manual spawn). */
  async spawnAgent(
    kildId: string,
    spec: AgentSpec,
    invitedBy: string = HUMAN,
  ): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const allowed = ensureKildCanSpawnAgent(kild);
    if (!allowed.ok) return allowed;

    const validated = await this.validateAgents(kild.cwd, [spec], kild.agents);
    if (!validated.ok) return validated;

    const result = this.startAgent(kild, validated.value[0] as ValidatedAgentSpec, invitedBy);
    if (!result.ok) return result;
    this.broadcast({ kilds: this.registry.summaries() });
    await this.send(kildId, HUMAN, `@${spec.handle} joined the kild.`, {
      system: true,
      allowStopped: true,
    });
    return ok({ message: `Spawned @${spec.handle} into the kild.` });
  }

  /** Register an ATTACHED agent: a harness kild does not own (a Claude Code session
   *  the human is driving) that wants a `@handle` in this kild. Nothing is spawned — the
   *  agent gets an inbox and is addressable from the moment it attaches.
   *
   *  Idempotent by handle: re-attaching an existing attached handle is a no-op, so a hook
   *  or a shell alias can call it on every session start without special-casing. Taking
   *  over an OWNED agent's handle is refused — two transports for one address would make
   *  delivery ambiguous. */
  async attach(kildId: string, handle: string): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);

    const trimmed = handle.trim();
    if (!trimmed) return fail('rejected', 'agent handle required');
    if (trimmed === HUMAN) return fail('rejected', `agent handle '${HUMAN}' is reserved`);

    const existing = kild.agents.find((agent) => agent.handle === trimmed);
    if (existing) {
      if (existing.ownership !== 'attached') {
        return fail('rejected', `@${trimmed} is already an owned agent in '${kild.name}'`);
      }
      return ok({ message: `@${trimmed} is already attached to kild '${kild.name}'.` });
    }

    const allowed = ensureKildCanSpawnAgent(kild);
    if (!allowed.ok) return allowed;
    if (kild.agents.length + 1 > MAX_AGENTS) {
      return fail('rejected', `kild capacity exceeded (max ${MAX_AGENTS} agents)`);
    }

    kild.agents.push({
      handle: trimmed,
      ownership: 'attached',
      invitedBy: HUMAN,
      // Attached and waiting: it has taken no turn for this kild yet.
      idle: true,
      inbox: new Inbox(),
    });
    this.registry.persistNow(kildId);
    this.broadcast({ kilds: this.registry.summaries() });
    await this.send(kildId, HUMAN, `@${trimmed} joined the kild (attached).`, {
      system: true,
      allowStopped: true,
    });
    return ok({ message: `@${trimmed} attached to kild '${kild.name}'.` });
  }

  /** Destructively read an attached agent's inbox — the pull half of the inverted
   *  transport, called at the harness's own turn boundary.
   *
   *  This IS the lifecycle signal too: an empty drain marks the agent idle, a non-empty
   *  one marks it working. There is deliberately no separate status verb — the drain the
   *  harness already makes on every turn end answers the question for free. */
  drain(kildId: string, handle: string): CommandResult<InboxDrain> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const agent = kild.agents.find((candidate) => candidate.handle === handle.trim());
    if (!agent) return fail('not_found', `no such agent: @${handle}`);
    if (agent.ownership !== 'attached') {
      return fail('rejected', `@${agent.handle} is an owned agent — kild pushes to it`);
    }
    const drained = agent.inbox.drain();
    agent.idle = drained.idle;
    if (drained.capped) {
      // The loop guard tripped. One structured line so a wake loop is visible in the
      // engine log rather than only in the owner's credit bill.
      console.error(
        JSON.stringify({
          t: 'kild.wake_cap',
          kild: kild.name,
          agent: agent.handle,
          pending: agent.inbox.pending,
        }),
      );
    }
    return ok(drained);
  }

  /** Stop every agent session — the human kill switch / kild teardown. A kild with
   *  history moves straight into the archive and is pushed to clients, so it stays
   *  visible as a read-only transcript without an engine restart. */
  async stop(kildId: string): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const allowed = ensureKildCanStopFromOperator(kild);
    if (!allowed.ok) return allowed;
    this.stopAgents(kild);
    const transitioned = transitionKildState(kild, 'closed');
    if (!transitioned.ok) return transitioned;
    const archived = this.registry.remove(kildId);
    this.notifyOpener(kild, {
      kind: 'closed',
      finalPost: finalNonSystemPost(kild),
    });
    if (archived) this.broadcast({ archivedKild: archived });
    this.broadcast({ kilds: this.registry.summaries() });
    if (archived) await this.recordMemory(kild);
    return ok({ message: `Kild '${kild.name}' stopped.` });
  }

  /** Manual circuit breaker: stop every agent session but KEEP the kild, so its
   *  transcript stays visible (read-only). The operator trips this to halt a runaway or
   *  off-track kild without tearing it down (vs {@link stop}). */
  async halt(kildId: string): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const allowed = ensureKildCanHalt(kild);
    if (!allowed.ok) return allowed;
    this.stopAgents(kild);
    const transitioned = transitionKildState(kild, 'halted');
    if (!transitioned.ok) return transitioned;
    await this.send(kildId, HUMAN, 'Kild halted by the operator.', {
      system: true,
      allowStopped: true,
    });
    this.notifyOpener(kild, {
      kind: 'halted',
      finalPost: finalNonSystemPost(kild),
    });
    this.broadcast({ kilds: this.registry.summaries() });
    return ok({ message: `Kild '${kild.name}' halted.` });
  }

  /** Post-stop memory hook: append the engine-written log entry (always), then spawn
   *  the optional synthesis session (config `memory.synthesis`) to distill the transcript
   *  into the memory dir's `MEMORY.md` (config `memory.dir`, default `.kild/`). Memory
   *  must never break a stop — failures are logged loud and swallowed here, at the one
   *  boundary where that is the right call. */
  private async recordMemory(kild: Kild): Promise<void> {
    let memoryDir: string;
    try {
      memoryDir = await configuredMemoryDir(kild.cwd);
      appendKildLog(kild, memoryDir);
    } catch (err) {
      console.error(
        `kild: log append failed for '${kild.name}': ${err instanceof Error ? err.message : err}`,
      );
      return; // no log entry → don't synthesize against a missing input
    }
    try {
      const synthesis = await configuredMemorySynthesis(kild.cwd);
      if (!synthesis) return;
      const id = this.createId();
      this.sessions.spawn(id, {
        model: synthesis.model,
        cwd: kild.cwd, // the MAIN checkout — memory files are gitignored, so worktrees never see them
        persona: synthesis.persona ?? 'default',
        label: `memory:${kild.name}`,
      });
      this.sessions.prompt(
        id,
        synthesisPrompt(kild, kildTranscriptPath(kild.id), memoryDir),
        'kild',
      );
    } catch (err) {
      console.error(
        `kild: memory synthesis spawn failed for '${kild.name}': ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  /** Spawn one agent session, wired so its control lines route back here. `invitedBy`
   *  is the spawner's handle (a live agent) or {@link HUMAN} for the creator's initial
   *  roster — the ground-truth spawn edge + idle-notice target. */
  private startAgent(
    kild: Kild,
    spec: ValidatedAgentSpec,
    invitedBy: string,
  ): CommandResult<{ agentId: string }> {
    const isLead = kild.agents.length === 0; // first agent leads the kild
    const agentId = this.createId();
    // Record the requested model now (visible immediately); the session's `model` event
    // upgrades it to the provider-resolved ref once it starts.
    kild.agents.push({
      handle: spec.handle,
      id: agentId,
      persona: spec.persona,
      model: spec.model,
      invitedBy,
    });
    try {
      this.sessions.spawn(
        agentId,
        {
          model: spec.model,
          cwd: kild.cwd,
          persona: spec.resolvedPersona,
          label: kild.name,
          // Every agent attaches to the kild's shared worktree, if any.
          worktree: kild.worktree,
          // Base branch a brand-new worktree forks from (the first agent creates it).
          base: kild.base,
          // Opaque to the AgentManager; the agent process reads these to register its
          // kild tools (`send`, `spawn`, and — lead only — `stop`) and tag its outbound
          // control lines.
          env: {
            KILD_KILD_ID: kild.id,
            KILD_HANDLE: spec.handle,
            ...(isLead ? { KILD_LEAD: '1' } : {}),
          },
        },
        'cli',
        {
          onSend: (m) => this.handleSend(agentId, m),
          onSpawn: (s) => this.handleSpawn(agentId, s),
          onStop: (s) => this.handleStop(agentId, s),
        },
      );
    } catch (err) {
      kild.agents = kild.agents.filter((agent) => agentProcessId(agent) !== agentId);
      return fail('rejected', err instanceof Error ? err.message : String(err));
    }
    return ok({ agentId });
  }

  /** An agent called `send`: resolve its kild/handle and route. */
  private async handleSend(agentId: string, m: SendOut): Promise<CommandResult<KildActionSuccess>> {
    const located = this.registry.locateAgent(agentId);
    if (!located) return fail('not_found', `agent '${agentId}' is not in a live kild`);
    return this.send(located.kild.id, located.agent.handle, m.text, { to: m.to });
  }

  /** An agent called `spawn`: add the named agent to its kild. */
  private async handleSpawn(
    agentId: string,
    spec: SpawnOut,
  ): Promise<CommandResult<KildActionSuccess>> {
    const located = this.registry.locateAgent(agentId);
    if (!located) return fail('not_found', `agent '${agentId}' is not in a live kild`);
    // The spawner is the calling agent — recorded as the new agent's `invitedBy`, so its
    // idle/done notice routes back here (hierarchical delegation signalling).
    return this.spawnAgent(
      located.kild.id,
      { handle: spec.handle, persona: spec.persona, model: spec.model },
      located.agent.handle,
    );
  }

  /** The kild's lead called `stop`: notice, then teardown. Only the lead holds the tool
   *  (agent-side), but enforce it here too — a control line is just stdout, so the engine,
   *  not the subprocess, is the authority on who may end a kild. */
  private async handleStop(
    agentId: string,
    stopSpec: StopOut,
  ): Promise<CommandResult<KildActionSuccess>> {
    const located = this.registry.locateAgent(agentId);
    if (!located) return fail('not_found', `agent '${agentId}' is not in a live kild`);
    const { kild, agent } = located;
    const allowed = ensureKildCanStopFromAgent(kild);
    if (!allowed.ok) return allowed;
    const lead = kild.agents[0];
    if (!lead || agentProcessId(lead) !== agentId) {
      return fail('rejected', `only the lead may stop kild '${kild.name}'`);
    }
    await this.send(
      kild.id,
      HUMAN,
      `Kild stopped by @${agent.handle}${stopSpec.reason ? `: ${stopSpec.reason}` : '.'}`,
      {
        system: true,
        allowStopped: true,
      },
    );
    return this.stop(kild.id);
  }

  /** Record + route one message from `from` (an agent handle or {@link HUMAN}).
   *
   * Addressing is structured, never parsed from prose — the ONE rule: a system notice
   * targets no one; otherwise an explicit `to` wins; otherwise the message goes to the
   * kild lead (the orchestrator). A typo'd handle is returned as a clean error to the
   * caller (the calling agent's tool result), so it can correct itself — it is never
   * recorded, routed, or turned into kild spam. */
  private async send(
    kildId: string,
    from: string,
    text: string,
    opts: { to?: string[]; system?: boolean; allowStopped?: boolean } = {},
  ): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const allowed = ensureKildCanSend(kild, { allowHalted: opts.allowStopped });
    if (!allowed.ok) return allowed;

    const lead = kild.agents[0]?.handle;
    const to = opts.system ? [] : opts.to?.length ? opts.to : lead ? [lead] : [];
    const message: Message = {
      id: this.createId(),
      kildId,
      from,
      to,
      text,
      ts: Date.now(),
      system: opts.system,
    };

    const unknown = unknownRecipients(kild, message);
    if (unknown.length > 0) {
      const known = kild.agents.map((agent) => `@${agent.handle}`).join(', ');
      return fail(
        'rejected',
        `no such agent: ${unknown.map((recipient) => `@${recipient}`).join(', ')} ` +
          `(in the kild: ${known || 'none'})`,
      );
    }

    traceSend(kild.name, message);
    this.registry.appendMessage(kildId, message);
    routeMessage(kild, message, this.delivery());
    if (
      !message.system &&
      message.to.includes(HUMAN) &&
      kild.agents.some((agent) => agent.handle === message.from)
    ) {
      this.notifyOpener(kild, humanPostEvent(message));
    }
    return ok({ message: 'Sent to the kild.', deliveredTo: to.filter((t) => t !== from) });
  }

  private async validateAgents(
    cwd: string,
    agents: AgentSpec[],
    existing: Array<{ handle: string }>,
  ): Promise<CommandResult<ValidatedAgentSpec[]>> {
    if (existing.length + agents.length > MAX_AGENTS) {
      return fail('rejected', `kild capacity exceeded (max ${MAX_AGENTS} agents)`);
    }

    const knownPersonas = new Set((await this.resolvePersonas(cwd)).map((p) => p.name));
    const seenHandles = new Set(existing.map((agent) => agent.handle));
    const validated: ValidatedAgentSpec[] = [];

    for (const spec of agents) {
      if (spec.handle === HUMAN) {
        return fail('rejected', `agent handle '${HUMAN}' is reserved`);
      }
      if (seenHandles.has(spec.handle)) {
        return fail('rejected', `duplicate agent: @${spec.handle}`);
      }
      seenHandles.add(spec.handle);

      const resolvedPersona = spec.persona ?? spec.handle;
      if (resolvedPersona !== 'default' && !knownPersonas.has(resolvedPersona)) {
        return fail('rejected', `unknown persona: ${resolvedPersona}`);
      }
      validated.push({ ...spec, resolvedPersona });
    }

    return ok(validated);
  }

  /** Stop the sessions kild owns. An attached agent has none — its harness belongs to
   *  the human, and kild must never assume it can end it. */
  private stopAgents(kild: Kild): void {
    for (const agent of kild.agents) {
      const agentId = agentProcessId(agent);
      if (agentId) this.sessions.stop(agentId);
    }
  }

  private rollbackCreate(kildId: string, agentIds: string[]): void {
    for (const agentId of agentIds) this.sessions.stop(agentId);
    this.registry.remove(kildId);
  }

  /** Best-effort direct notification. It deliberately bypasses kild send/routing so an
   *  operator prompt can never become a kild message or trigger an agent reply loop. */
  private notifyOpener(kild: Kild, event: Parameters<typeof formatOperatorNotification>[1]): void {
    const target = openerNotificationTarget(kild);
    if (!target) return;
    this.sessions.prompt(target, formatOperatorNotification(kild.name, event), 'kild');
  }

  private delivery(): Delivery {
    return {
      deliverAsTurn: (agentId, from, text) => {
        // A delivered turn reactivates the agent: it is no longer waiting.
        const located = this.registry.locateAgent(agentId);
        if (located) located.agent.idle = false;
        this.sessions.prompt(agentId, text, from);
      },
      // The attached counterpart of the stdin push: queue it and let the agent collect
      // it. `idle` deliberately does NOT flip here — the harness really is idle until it
      // takes its next turn; the drain is what moves it.
      queueForAttached: (agent, from, text) => {
        agent.inbox.enqueue({ from, text, ts: Date.now() });
      },
      broadcast: (message) => this.broadcast({ message }),
    };
  }

  private broadcast(msg: KildOutbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton, mirroring {@link agentManager}. */
export const kildManager = new KildManager();
