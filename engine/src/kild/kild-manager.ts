import { randomUUID } from 'node:crypto';
import path from 'node:path';

import { type AgentCallbacks, agentManager, type SpawnRequest } from './agent-manager.ts';
import { type AttachIdentity, type AttachSuccess, AttachTokens } from './attach-token.ts';
import { configuredCloseHook, configuredMemoryDir } from './config.ts';
import { type KildCloseEvent, runCloseHook } from './hooks.ts';
import { Inbox, type InboxDrain } from './inbox.ts';
import { KildRegistry } from './kild-registry.ts';
import {
  type AgentSpec,
  type ArchivedKild,
  type ArchivedKildView,
  agentProcessId,
  agentView,
  archivedKildView,
  type CommandResult,
  type CreateMissing,
  costTotals,
  type Kild,
  type KildActionSuccess,
  type KildIdentity,
  type KildOutbound,
  type KildStatus,
  kildIdentity,
  type Message,
  type NewKildSpec,
  type NewKildSuccess,
  type SendOut,
  type SpawnContext,
  type StopOut,
} from './kild-types.ts';
import { appendKildLog, collectLedgerFacts, kildTranscriptPath } from './memory.ts';
import { GENERAL_PERSONA, listPersonas } from './personas.ts';
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
      chars: message.text.length,
    }),
  );
}

/** How a delivered message reads to the agent receiving it: who, where, what. */
export function formatDelivery(kildName: string, from: string, text: string): string {
  return `[#${kildName}] @${from}: ${text}`;
}

/** Named recipients that are not in this kild's roster. The one addressing question the
 *  engine still answers, and it answers it with "no" — the sender names recipients, so
 *  the only thing left to check is whether those names exist. */
function unknownRecipients(kild: Kild, to: string[]): string[] {
  const handles = new Set(kild.agents.map((agent) => agent.handle));
  return to.filter((recipient) => !handles.has(recipient));
}

/** Named recipients whose session is gone. A stopped agent STAYS on the roster — its handle
 *  never rebinds and its transcript stays addressable — so membership alone said "yes" to a
 *  send nothing could receive: it validated, `prompt()` returned false into a discarded
 *  return value, and the sender was told "Sent to the kild." A stopped handle is a real
 *  address with no reader, which is a rejection, not a delivery. */
function stoppedRecipients(kild: Kild, to: string[]): string[] {
  return to.filter((recipient) =>
    kild.agents.some((agent) => agent.handle === recipient && agent.stopped),
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
 * Owns live kilds: creates them (one session per agent), delivers every message to the
 * recipients its sender named, grows them (`spawn` / `attach`), and stops them.
 *
 * **Addressing is the sender's job, never the engine's.** `send` takes a `to` and
 * delivers to exactly those handles — there is no default recipient, no rank, and no
 * rule that turns an unaddressed message into an addressed one. An empty `to` is a
 * rejection. That is the whole of what used to be a router: the question "who is this
 * for?" has one answer, given at the call site.
 *
 * Sits beside the AgentManager — agents ARE sessions, so the AgentManager stays
 * kild-agnostic; it only forwards an agent's control lines (`send` / `spawn`) to the
 * callbacks we hand it.
 */
export class KildManager {
  private readonly registry: KildRegistry;
  private readonly sessions: AgentRuntime;
  private readonly resolvePersonas: typeof listPersonas;
  private readonly createId: () => string;
  private readonly subscribers = new Set<(msg: KildOutbound) => void>();
  /** Attach credentials for the live kilds — minted here, dropped when a kild stops. */
  private readonly tokens = new AttachTokens();

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
      agents: [],
      log: [],
    };
    this.registry.create(kild);

    const spawnedAgentIds: string[] = [];
    for (const agent of validated.value) {
      const result = this.startAgent(kild, agent);
      if (!result.ok) {
        this.rollbackCreate(kildId, spawnedAgentIds);
        return result;
      }
      spawnedAgentIds.push(result.value.agentId);
    }

    this.broadcast({ kilds: this.registry.summaries() });
    return ok({ kildId, message: `Kild '${spec.name}' created.` });
  }

  /**
   * A kild's log as its own resource, cursored by {@link Message.seq}. The ONE reader of a
   * kild's messages — there is no second, log-shaped view riding a listing.
   *
   * Works for an ARCHIVED kild too: its log is the whole of what it still is, and a
   * read-only record that could not be read would be no record at all. Undefined
   * distinguishes "no such kild" from "a kild with nothing to say".
   *
   * `since` is exclusive — pass back the last `seq` you saw and get only what arrived
   * after it. Omit it for the whole log.
   */
  messages(kildId: string, since?: number): Message[] | undefined {
    const log = this.registry.log(kildId);
    if (!log) return undefined;
    return since === undefined ? log : log.filter((message) => message.seq > since);
  }

  /** Past kilds recovered from disk — their records, without their logs. A log is
   *  `messages(id)`, which answers for an archived kild too. */
  archivedViews(): ArchivedKildView[] {
    return this.registry.archived().map(archivedKildView);
  }

  /** The stored archive, log included — for the engine's own lookups (an agent's
   *  `piSessionFile` outlives its session). Never a response body. */
  archived(): ArchivedKild[] {
    return this.registry.archived();
  }

  /** Live kilds with their logs — for a client joining a kild it didn't create (or
   *  reloading), so it can render the conversation so far. */
  liveKilds(): ArchivedKild[] {
    return this.registry.liveWithLogs();
  }

  /** Live kilds as identity + structure only — no git, no cost, no log, and therefore no
   *  subprocesses. This is what a client polling a list of kilds should read. */
  liveIdentities(): KildIdentity[] {
    return this.registry.liveIdentities();
  }

  /** Live kilds enriched with each kild's git/worktree state — the code-state
   *  half of observability, so a driving agent can land work and spot collisions.
   *  COSTLY: a batch of git invocations per kild, which is why it is its own route on its
   *  own cadence rather than riding the listing. */
  async liveKildsStatus(): Promise<KildStatus[]> {
    return Promise.all(this.registry.liveKildObjects().map((kild) => this.statusOf(kild)));
  }

  /** ONE live kild's status — the same enrichment {@link liveKildsStatus} does per kild,
   *  exposed so addressing a single kild does not probe every other kild's git state.
   *  Undefined when no live kild has that id. */
  async liveKildStatus(kildId: string): Promise<KildStatus | undefined> {
    const kild = this.registry.get(kildId);
    return kild ? this.statusOf(kild) : undefined;
  }

  /** Identity + the costly half for one live kild. Effective dir = the kild's worktree if
   *  set, else its cwd. Git failures are captured per-kild (never thrown), so status stays
   *  available even mid-conflict. */
  private async statusOf(kild: Kild): Promise<KildStatus> {
    return {
      ...kildIdentity(kild),
      landedSha: kild.landedSha,
      agents: kild.agents.map(agentView),
      totals: costTotals(kild.agents),
      git: await kildGitStatus(kild.worktree ? worktreePath(kild.worktree) : kild.cwd, kild.base),
    };
  }

  /** The effective kild dir (the kild's worktree if set, else its cwd) + base of
   *  ONE live kild — the same resolution {@link liveKildsStatus} uses per kild, exposed
   *  so the review, land and disposal endpoints can drill into a single kild without
   *  probing every kild's git state. `repo` is the main checkout (its cwd), which land
   *  merges into and disposal removes the worktree from. Live kilds only: an archived
   *  kild's agents are gone and its worktree may be pruned, so there is no working dir to
   *  inspect (`invalid_state`); an id that was never a kild is `not_found`. */
  kildDir(kildId: string): CommandResult<{
    name: string;
    dir: string;
    repo: string;
    worktree?: string;
    base?: string;
  }> {
    const kild = this.registry.get(kildId);
    if (kild) {
      return ok({
        name: kild.name,
        dir: kild.worktree ? worktreePath(kild.worktree) : kild.cwd,
        repo: kild.cwd,
        worktree: kild.worktree,
        base: kild.base,
      });
    }
    if (this.registry.archived().some((archived) => archived.id === kildId)) {
      return fail('invalid_state', `kild ${kildId} is archived; its working dir is gone`);
    }
    return fail('not_found', `no such live kild: ${kildId}`);
  }

  /** Project directories the live kilds run in — the repos to ask git for `kild/*`
   *  worktrees, alongside the registered projects. */
  liveCwds(): string[] {
    return [...new Set(this.registry.liveKildObjects().map((kild) => kild.cwd))];
  }

  /** Worktree names live kilds occupy — a tree in this set has a kild record, so it is
   *  not an orphan. */
  liveWorktrees(): Set<string> {
    return new Set(
      this.registry
        .liveKildObjects()
        .map((kild) => kild.worktree)
        .filter((name): name is string => typeof name === 'string'),
    );
  }

  /** Record the merge a land produced, so the ledger can name the commit instead of
   *  inferring containment. */
  recordLand(
    kildId: string,
    sha: string,
    carried?: { commits: number; files: number },
  ): CommandResult<KildActionSuccess> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    kild.landedSha = sha;
    // Counted at the moment of the merge, because afterwards they are gone: `base..HEAD` is
    // empty once the branch is contained in base.
    kild.landed = carried;
    this.registry.persistNow(kildId);
    return ok({ message: `Kild '${kild.name}' landed as ${sha.slice(0, 7)}.` });
  }

  /** Stop ONE agent's session without stopping the kild — the rest keep working.
   *
   *  The agent stays on the roster, marked `stopped`: a handle is unique for the kild's
   *  lifetime and must never rebind, so its history stays addressable
   *  (`…/agents/:handle/transcript`) after its process is gone. An attached agent is
   *  refused — its harness belongs to the human, and kild must never assume it may end
   *  it. */
  stopAgent(kildId: string, handle: string): CommandResult<KildActionSuccess> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    const agent = kild.agents.find((candidate) => candidate.handle === handle.trim());
    if (!agent) return fail('not_found', `no such agent: @${handle}`);
    if (agent.ownership === 'attached') {
      return fail('rejected', `@${agent.handle} is attached — its harness is not kild's to stop`);
    }
    if (agent.stopped) {
      return ok({ message: `@${agent.handle} was already stopped.` });
    }
    this.sessions.stop(agent.id);
    agent.stopped = true;
    agent.idle = true;
    this.registry.persistNow(kildId);
    this.broadcast({ kilds: this.registry.summaries() });
    return ok({ message: `Stopped @${agent.handle} in kild '${kild.name}'.` });
  }

  /**
   * Spawn an agent into a live kild. {@link SpawnContext} says who is spawning and, if they
   * gave one, what the new agent is being spawned to do. Observers learn about the new
   * roster from the `{kilds}` broadcast — a join is an event, not a message anyone was sent.
   *
   * A `task` is delivered afterwards as an ordinary message from the spawner, so it lands on
   * the log and reads to the new agent exactly like every later instruction. If that send
   * fails the spawn still succeeded — the agent exists and is addressable — so it is
   * reported, not rolled back: killing a working agent because its first message did not
   * land would be the worse outcome.
   */
  async spawnAgent(
    kildId: string,
    spec: AgentSpec,
    context: SpawnContext = {},
  ): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    // Refused up front rather than defaulted: a task is a message, and inventing a sender
    // for it would put a name on the log that nobody proved.
    if (context.task !== undefined && context.invitedBy === undefined) {
      return fail('rejected', 'a task needs a sender: spawn from an identified caller');
    }

    const validated = await this.validateAgents(kild.cwd, [spec], kild.agents);
    if (!validated.ok) return validated;

    const result = this.startAgent(
      kild,
      validated.value[0] as ValidatedAgentSpec,
      context.invitedBy,
    );
    if (!result.ok) return result;
    this.broadcast({ kilds: this.registry.summaries() });

    const spawned = `Spawned @${spec.handle} into the kild.`;
    if (context.task === undefined || context.invitedBy === undefined)
      return ok({ message: spawned });
    const sent = await this.send(kildId, context.invitedBy, [spec.handle], context.task);
    return ok({
      message: sent.ok
        ? `${spawned} Tasked it as @${context.invitedBy}.`
        : `${spawned} Its task was NOT delivered (${sent.message}) — send it yourself.`,
    });
  }

  // ── Attribution ─────────────────────────────────────────────────────────────
  // A message's `from` is the sender's HANDLE, and a handle is kild-level knowledge: it is
  // an agent's membership of a kild, not a property of the session substrate. Both
  // credentials are therefore resolved here rather than by asking the agent manager for a
  // persona — which named the wrong thing (two agents on one persona were
  // indistinguishable) and asked the wrong layer.

  /** The handle the agent running this kild session speaks as.
   *
   *  Rejects a session that is in no live kild: an agent kild spawned outside a kild has no
   *  handle, and there is nothing to invent one from. If it wants a name on a kild's log it
   *  attaches for one. */
  handleForSession(sessionId: string): CommandResult<string> {
    const located = this.registry.locateAgent(sessionId);
    if (!located) return fail('rejected', `unknown session: ${sessionId}`);
    return ok(located.agent.handle);
  }

  /** The (kild, handle) a bearer token stands for — undefined when nothing minted it. */
  identifyToken(token: string): AttachIdentity | undefined {
    return this.tokens.identify(token);
  }

  /** Register an ATTACHED agent: a harness kild does not own (a Claude Code session
   *  the human is driving) that wants a `@handle` in this kild. Nothing is spawned — the
   *  agent gets an inbox and is addressable from the moment it attaches.
   *
   *  It also gets a TOKEN: the credential that makes it attributable when it sends, since
   *  it has no kild session to be recognised by (see {@link AttachTokens}). Attaching is the
   *  one moment a harness becomes addressable, so it is the one moment it can be given an
   *  identity.
   *
   *  Idempotent by handle: re-attaching an existing attached handle is a no-op returning the
   *  SAME token, so a hook or a shell alias can call it on every session start without
   *  special-casing and without invalidating the token the session already holds. Taking
   *  over an OWNED agent's handle is refused — two transports for one address would make
   *  delivery ambiguous. */
  async attach(kildId: string, handle: string): Promise<CommandResult<AttachSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);

    const trimmed = handle.trim();
    if (!trimmed) return fail('rejected', 'agent handle required');

    const existing = kild.agents.find((agent) => agent.handle === trimmed);
    if (existing) {
      if (existing.ownership !== 'attached') {
        return fail('rejected', `@${trimmed} is already an owned agent in '${kild.name}'`);
      }
      return ok({
        message: `@${trimmed} is already attached to kild '${kild.name}'.`,
        token: this.tokens.mint(kildId, trimmed),
      });
    }

    if (kild.agents.length + 1 > MAX_AGENTS) {
      return fail('rejected', `kild capacity exceeded (max ${MAX_AGENTS} agents)`);
    }

    kild.agents.push({
      handle: trimmed,
      ownership: 'attached',
      // Attached and waiting: it has taken no turn for this kild yet.
      idle: true,
      inbox: new Inbox(),
    });
    this.registry.persistNow(kildId);
    this.broadcast({ kilds: this.registry.summaries() });
    return ok({
      message: `@${trimmed} attached to kild '${kild.name}'.`,
      token: this.tokens.mint(kildId, trimmed),
    });
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

  /** Stop every agent session and archive the kild — the one teardown verb (there is no
   *  separate halt: a kild is live while it is in the registry and stopped once it is
   *  archived). A kild with history is pushed to clients as an archived snapshot, so it
   *  stays visible as a read-only transcript without an engine restart. The worktree is
   *  never touched — code outlives the agents that wrote it. */
  async stop(kildId: string): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    this.stopAgents(kild);
    // Attach tokens live with the kild and die with it: an archived kild has no addressable
    // handles, so a credential naming one would outlive the only thing it meant.
    this.tokens.forget(kildId);
    const archived = this.registry.remove(kildId);
    if (archived) this.broadcast({ archivedKild: archivedKildView(archived) });
    this.broadcast({ kilds: this.registry.summaries() });
    if (archived) await this.close(kild);
    return ok({ message: `Kild '${kild.name}' stopped.` });
  }

  /**
   * The close lifecycle: write the ledger entry, emit the close event, run the declared
   * hook. All three are mechanism — the engine states facts and fires what config
   * declares; it has no opinion about what should be made of them.
   *
   * A stop must never fail because of any of this, so every failure is logged loud and
   * swallowed here, at the one boundary where that is the right call.
   */
  private async close(kild: Kild): Promise<void> {
    const dir = kild.worktree ? worktreePath(kild.worktree) : kild.cwd;
    const memoryDir = await configuredMemoryDir(kild.cwd);
    const ledgerPath = path.join(memoryDir, 'LOG.md');
    try {
      // Facts at the moment of stopping — after this the worktree can be landed or
      // pruned and the answers change.
      appendKildLog(
        kild,
        memoryDir,
        await collectLedgerFacts(dir, kild.base, kild.landedSha, kild.landed),
      );
    } catch (err) {
      console.error(
        `kild: ledger append failed for '${kild.name}': ${err instanceof Error ? err.message : err}`,
      );
    }

    const event: KildCloseEvent = {
      kildId: kild.id,
      name: kild.name,
      cwd: kild.cwd,
      worktree: kild.worktree,
      base: kild.base,
      transcriptPath: kildTranscriptPath(kild.id),
      ledgerPath,
    };
    this.broadcast({ kildClosed: event });

    try {
      const hook = await configuredCloseHook(kild.cwd);
      if (!hook) return;
      // Not awaited: a hook is someone else's work, and a slow one must not hold a stop
      // open. Its own failures are already captured inside.
      void runCloseHook(hook, event, (spec, prompt) => {
        const id = this.createId();
        this.sessions.spawn(id, spec);
        this.sessions.prompt(id, prompt, 'kild');
      }).catch((err) => {
        console.error(
          `kild: onClose hook failed for '${kild.name}': ${err instanceof Error ? err.message : err}`,
        );
      });
    } catch (err) {
      console.error(
        `kild: onClose hook failed for '${kild.name}': ${err instanceof Error ? err.message : err}`,
      );
    }
  }

  /** Spawn one agent session, wired so its control lines come back here. `invitedBy` is
   *  the spawning agent's handle, absent for the creator's initial roster — the
   *  ground-truth spawn edge. It confers nothing: every agent in a kild is a peer. */
  private startAgent(
    kild: Kild,
    spec: ValidatedAgentSpec,
    invitedBy?: string,
  ): CommandResult<{ agentId: string }> {
    const agentId = this.createId();
    // Record the requested model now (visible immediately); the session's `model` event
    // upgrades it to the provider-resolved ref once it starts.
    kild.agents.push({
      handle: spec.handle,
      id: agentId,
      // The RESOLVED persona — what the process actually runs, which is the explicit one or
      // the handle. Storing the unresolved `spec.persona` recorded `undefined` for an agent
      // demonstrably running a persona, so the roster, the wire and the ledger each gave a
      // different answer and none of them the true one.
      persona: spec.resolvedPersona,
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
          // Seed this agent from a COPY of an existing pi session, when asked. The copy is
          // what makes forking a live agent's session safe: the source file is never written.
          forkFrom: spec.forkFrom,
          // Opaque to the AgentManager; the agent process reads these to register its
          // kild tools (`send`, `spawn`, `stop`) and tag its outbound control lines.
          env: {
            KILD_KILD_ID: kild.id,
            KILD_HANDLE: spec.handle,
          },
        },
        'cli',
        {
          onSend: (m) => this.handleSend(agentId, m),
          onStop: (s) => this.handleStop(agentId, s),
        },
      );
    } catch (err) {
      kild.agents = kild.agents.filter((agent) => agentProcessId(agent) !== agentId);
      return fail('rejected', err instanceof Error ? err.message : String(err));
    }
    return ok({ agentId });
  }

  /**
   * An agent called `send`: resolve its kild/handle and deliver — creating any recipient the
   * kild does not have yet.
   *
   * This is why an agent has no separate `spawn` tool. Spawning was never the goal: every
   * real delegation was spawn-then-send, and a spawn on its own produced an agent sitting
   * idle. An agent addresses a handle; if nobody holds that handle, naming it brings the
   * agent in. One verb, and the idle-agent state is unreachable rather than warned about.
   *
   * The creator is the calling agent, taken from the session the control line arrived on —
   * never from the line itself. The agent says who it is addressing and what it wants; the
   * engine says who asked.
   */
  private async handleSend(agentId: string, m: SendOut): Promise<CommandResult<KildActionSuccess>> {
    const located = this.registry.locateAgent(agentId);
    if (!located) return fail('not_found', `agent '${agentId}' is not in a live kild`);
    return this.send(located.kild.id, located.agent.handle, m.to, m.text, {
      persona: m.persona,
      model: m.model,
    });
  }

  /** An agent called `stop`: tear the kild down. Any agent in the kild may do it —
   *  there are no ranks, so there is no rank to check. */
  private async handleStop(
    agentId: string,
    _stopSpec: StopOut,
  ): Promise<CommandResult<KildActionSuccess>> {
    const located = this.registry.locateAgent(agentId);
    if (!located) return fail('not_found', `agent '${agentId}' is not in a live kild`);
    return this.stop(located.kild.id);
  }

  /**
   * Record + deliver one message from `from` to the handles in `to`.
   *
   * `to` is what the SENDER named. The engine adds nothing to it and infers nothing for
   * it: an empty list is a rejection, not a broadcast and not a default. The only
   * addressing question left is whether those handles exist — a typo comes back as a
   * clean error naming the roster, so the caller (an agent's tool result, a CLI user) can
   * correct itself. Such a message is never recorded, never delivered, never kild spam.
   *
   * `createMissing` flips what an unknown recipient MEANS, and only a caller with the
   * authority to grow the kild passes it: with it, a handle nobody holds is created and then
   * delivered to (see {@link createRecipients}); without it, it stays a rejection. That is
   * the one difference between an agent inside the kild — for which addressing someone new
   * IS how delegation starts — and a client at the REST boundary, where a typo must never
   * quietly cost a process. Neither reads intent from the text; both are told the handles.
   */
  async send(
    kildId: string,
    from: string,
    to: string[],
    text: string,
    createMissing?: CreateMissing,
  ): Promise<CommandResult<KildActionSuccess>> {
    const kild = this.registry.get(kildId);
    if (!kild) return fail('not_found', `no such kild: ${kildId}`);
    if (to.length === 0) {
      return fail('rejected', 'a message must name at least one recipient');
    }

    // Handles this send brought into the kild, reported back so the sender knows a name it
    // used is now an agent (and, when a persona was resolved from the handle, which one).
    let created: string[] = [];
    const unknown = unknownRecipients(kild, to);
    if (unknown.length > 0) {
      if (!createMissing) {
        const known = kild.agents.map((agent) => `@${agent.handle}`).join(', ');
        return fail(
          'rejected',
          `no such agent: ${unknown.map((recipient) => `@${recipient}`).join(', ')} ` +
            `(in the kild: ${known || 'none'})`,
        );
      }
      const result = await this.createRecipients(kild, unknown, from, createMissing);
      if (!result.ok) return result;
      created = result.value;
    }

    // Checked AFTER creation (a fresh agent is never stopped) and before the log: a message
    // nothing can receive must not be recorded as if it were read. Refused wholesale for a
    // mixed list, exactly as an unknown recipient already is — a partial send is the thing
    // the sender would not be told about.
    const stopped = stoppedRecipients(kild, to);
    if (stopped.length > 0) {
      const live = kild.agents
        .filter((agent) => !agent.stopped)
        .map((agent) => `@${agent.handle}`)
        .join(', ');
      return fail(
        'rejected',
        `stopped, nothing would read this: ${stopped.map((h) => `@${h}`).join(', ')} ` +
          `(still working: ${live || 'nobody'})`,
      );
    }

    // The registry stamps `seq` as it appends, and the STAMPED message is what gets
    // broadcast — so a WS `{message}` frame carries the same cursor a later
    // `GET …/messages?since=` will report, and a client can tell new from replay.
    const message = this.registry.appendMessage(kildId, {
      id: this.createId(),
      kildId,
      from,
      to,
      text,
      ts: Date.now(),
    });
    if (!message) return fail('not_found', `no such kild: ${kildId}`);
    traceSend(kild.name, message);
    this.broadcast({ message });
    const undelivered = this.deliver(kild, message);
    const intended = to.filter((t) => t !== from);
    return ok({
      message: undelivered.length
        ? `Sent, but ${undelivered.map((h) => `@${h}`).join(', ')} did not receive it — ` +
          'its session is gone; it is marked stopped.'
        : created.length
          ? `Brought ${created.map((h) => `@${h}`).join(', ')} into the kild and sent to them.`
          : 'Sent to the kild.',
      deliveredTo: intended.filter((t) => !undelivered.includes(t)),
      created: created.length ? created : undefined,
    });
  }

  /**
   * Create the recipients a sender named but the kild does not have yet — the creating half
   * of "addressing an agent that isn't here brings it in".
   *
   * Validated as a BATCH before anything starts, so a call naming one good and one unknown
   * persona spawns neither. That is what keeps the fused verb honest: either every named
   * recipient exists afterwards, or the send is refused and the kild is untouched. A spawn
   * that throws mid-batch rolls its own siblings back for the same reason.
   */
  private async createRecipients(
    kild: Kild,
    handles: string[],
    invitedBy: string,
    spec: CreateMissing,
  ): Promise<CommandResult<string[]>> {
    const validated = await this.validateAgents(
      kild.cwd,
      handles.map((handle) => ({ handle, persona: spec.persona, model: spec.model })),
      kild.agents,
    );
    if (!validated.ok) {
      // The sender may have meant somebody who is already here and mistyped the handle, so
      // the refusal names the roster too. Between this and the persona list, one rejection
      // tells an agent everything it needs to correct itself without a probe.
      const roster = kild.agents.map((agent) => `@${agent.handle}`).join(', ');
      return fail(validated.code, `${validated.message} (in the kild: ${roster || 'none'})`);
    }

    const started: string[] = [];
    for (const agent of validated.value) {
      const result = this.startAgent(kild, agent, invitedBy);
      if (!result.ok) {
        for (const agentId of started) this.sessions.stop(agentId);
        kild.agents = kild.agents.filter((a) => {
          const id = agentProcessId(a);
          return id === undefined || !started.includes(id);
        });
        return result;
      }
      started.push(result.value.agentId);
    }
    this.registry.persistNow(kild.id);
    this.broadcast({ kilds: this.registry.summaries() });
    return ok(handles);
  }

  /**
   * The whole of delivery: for each named recipient, push or queue. Returns the handles that
   * could NOT be reached.
   *
   * Two branches, and they differ only in transport — an `owned` agent is a process kild
   * can prompt, an `attached` one is a harness that pulls from its inbox. The single
   * exception is the sender: an agent is never delivered its own message.
   *
   * `prompt()` answers whether the process was there, and that answer used to be discarded:
   * a session that had died on its own (a crash — not a `stop`, so nothing marked the roster)
   * reported a successful send into nothing, and flipped `idle` false on an agent that would
   * never emit `agent_end` to flip it back. Delivery is the moment the engine finds out, so
   * it is the moment the roster learns: an unreachable agent is marked stopped, its idle
   * state is left honest, and the sender is told.
   */
  private deliver(kild: Kild, message: Message): string[] {
    const undelivered: string[] = [];
    for (const handle of message.to) {
      if (handle === message.from) continue;
      const agent = kild.agents.find((candidate) => candidate.handle === handle);
      if (!agent) continue;
      if (agent.ownership === 'attached') {
        // `idle` deliberately does NOT flip here — the harness really is idle until it
        // takes its next turn; the drain is what moves it.
        agent.inbox.enqueue({ from: message.from, text: message.text, ts: Date.now() });
        continue;
      }
      const delivered = this.sessions.prompt(
        agent.id,
        formatDelivery(kild.name, message.from, message.text),
        message.from,
      );
      if (delivered) {
        // A delivered turn reactivates the agent: it is no longer waiting.
        agent.idle = false;
        continue;
      }
      undelivered.push(agent.handle);
      agent.stopped = true;
      agent.idle = true;
    }
    if (undelivered.length > 0) {
      this.registry.persistNow(kild.id);
      this.broadcast({ kilds: this.registry.summaries() });
    }
    return undelivered;
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
      if (seenHandles.has(spec.handle)) {
        return fail('rejected', `duplicate agent: @${spec.handle}`);
      }
      seenHandles.add(spec.handle);

      const resolvedPersona = spec.persona ?? spec.handle;
      if (resolvedPersona !== GENERAL_PERSONA && !knownPersonas.has(resolvedPersona)) {
        // Naming what DOES exist is the whole point: a persona is also how a handle nobody
        // holds becomes an agent, so this rejection is the discovery path. Without the list
        // the caller's only recourse is another guess.
        const available = [
          GENERAL_PERSONA,
          ...[...knownPersonas].filter((p) => p !== GENERAL_PERSONA),
        ];
        return fail(
          'rejected',
          `unknown persona: ${resolvedPersona} (available: ${available.join(', ')})`,
        );
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

  private broadcast(msg: KildOutbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton, mirroring {@link agentManager}. */
export const kildManager = new KildManager();
