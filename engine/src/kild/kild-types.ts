import type { UiEvent } from './events.ts';
import type { KildCloseEvent } from './hooks.ts';
import type { Inbox } from './inbox.ts';
import type { KildGitStatus } from './worktree-status.ts';

/**
 * Kild domain — the operator-facing primitive: a set of agents exchanging directed
 * Messages. A single-agent "session" is just a 1-agent Kild. Pure types; no behaviour
 * (state lives in the registry, delivery in the manager).
 *
 * Every message names its recipients. There is no privileged sender, no privileged
 * recipient, and no rank: a human-driven harness attaches and gets an ordinary handle
 * like any other agent.
 */

/** How kild reaches an agent.
 *
 *  - `owned` (the default, and everything that existed before): kild owns the process,
 *    so delivery is a PUSH — a prompt written to its stdin the moment a message is routed.
 *  - `attached`: a harness the human drives (a Claude Code session), which kild registers
 *    and addresses but never spawns. There is nothing to push to, so delivery inverts —
 *    kild queues into an {@link Inbox} and the harness PULLS at its own turn boundary.
 *
 *  Absent means `owned`: kilds persisted before attached agents existed decode
 *  unchanged (see {@link ArchivedKild}). */
export type Ownership = 'owned' | 'attached';

/** Everything both ownerships carry — identity plus the idle state observers read. */
interface AgentBase {
  /** The `@mention` handle (e.g. `orchestrator`). */
  handle: string;
  /** The persona (`.pi/agents/<name>.md` — the dir name is upstream pi convention) it
   *  runs as. */
  persona?: string;
  /** The model this agent runs on — the requested ref at spawn, upgraded to the
   *  provider-resolved `provider/id` once the session reports it. Lets an observer see
   *  which model each agent used in a run. */
  model?: string;
  /** Who spawned this agent — the spawning agent's handle, absent for the creator's
   *  initial roster. Ground-truth spawn edge (vs inferring it from the log). */
  invitedBy?: string;
  /** True when the agent has finished a turn and is waiting. Set on `agent_end` for an
   *  owned agent and by an EMPTY drain for an attached one, cleared when work arrives.
   *  Rides {@link AgentView} so observers can rank finished-and-waiting kilds without
   *  parsing logs. */
  idle?: boolean;
  /** Latest cumulative token count for this agent's session, captured from its
   *  `stats` UiEvents (emitted at each turn end). */
  tokens?: number;
  /** Latest cumulative cost (USD) for this agent's session, from `stats` UiEvents. */
  cost?: number;
  /** True once this agent's session has been stopped on its own (via
   *  `DELETE /api/kilds/:id/agents/:handle`). It STAYS on the roster — a handle is
   *  unique for the kild's lifetime and never rebinds — so observers need this to tell
   *  a working agent from a stopped one. */
  stopped?: boolean;
}

/** An agent kild spawned and owns: an agent session addressable by its `@handle`,
 *  delivered to by writing a prompt to its stdin. */
export interface OwnedAgent extends AgentBase {
  /** Absent (the persisted default) or explicit — both mean kild owns the process. */
  ownership?: 'owned';
  /** The kild session id running this agent. */
  id: string;
  /** The underlying pi session id — the durable terminal-resume handle
   *  (`pi --session <piSessionFile ?? piSessionId>`). Captured from the agent's
   *  `pi_session` event and persisted with the kild, so any agent in any kild —
   *  live or archived — can be reopened in the pi CLI. */
  piSessionId?: string;
  /** Absolute pi session file path (resume works from any cwd). */
  piSessionFile?: string;
}

/** An agent kild registered but did not spawn — a harness the human drives. It has
 *  no kild session and no pi session (nothing to resume): its whole transport is the
 *  inbox it drains at its own turn boundary. */
export interface AttachedAgent extends AgentBase {
  ownership: 'attached';
  /** Unread messages addressed to this agent, waiting for the next drain. Live state
   *  only — never persisted (an inbox outlives nothing; the kild log is the record). */
  inbox: Inbox;
}

/** An agent in a kild, of either ownership. This list is the complete roster: an
 *  address that is not in it does not exist. */
export type Agent = OwnedAgent | AttachedAgent;

/** The kild session behind an agent, or `undefined` when kild does not own its
 *  process. The ONE place callers that genuinely do not care about the ownership (stop
 *  the sessions, find a kild by session id) ask the question. */
export function agentProcessId(agent: Agent): string | undefined {
  return agent.ownership === 'attached' ? undefined : agent.id;
}

/** An agent as the CHEAP half of the listing surfaces it: who it is, what it runs as, and
 *  whether it is waiting. Every field is in-memory roster state — reading it costs nothing,
 *  which is the whole point of the split (see {@link KildIdentity}). */
export interface AgentIdentity {
  handle: string;
  /** ALWAYS present on the wire — `'owned'` when kild runs the process, `'attached'` when a
   *  harness it does not own holds the handle.
   *
   *  It was previously omitted for owned agents, on the reasoning that absent means owned.
   *  That made the one field a client switches on optional, so `agent.ownership === 'owned'`
   *  was false for every owned agent and every consumer needed a `?? 'owned'` it had to
   *  remember. An implicit default on a discriminant is the same mistake as the lead default
   *  this restructure deleted: the engine knows the answer, so it should say it. */
  ownership: Ownership;
  persona?: string;
  model?: string;
  /** Who spawned this agent (see {@link AgentBase.invitedBy}). On the CHEAP view because
   *  it is the only thing that tells five identical `reviewer` personas apart by origin:
   *  filter a roster by `invitedBy` and you have exactly the agents one delegator started.
   *  It was already recorded and already free to read — omitting it just meant nobody
   *  could see it. */
  invitedBy?: string;
  /** Attention state: finished a turn and waiting for input (see {@link AgentBase.idle}). */
  idle?: boolean;
  /** True once the agent's session was stopped individually (see {@link AgentBase.stopped}). */
  stopped?: boolean;
}

/** An agent as surfaced to observers that pay for detail (kild status, archive) — identity
 *  plus what the session cost and the PI session identity, the durable handle for reopening
 *  the agent in a terminal. No kild process id (that's an internal handle). */
export interface AgentView extends AgentIdentity {
  piSessionId?: string;
  piSessionFile?: string;
  /** Latest cumulative session token count (from `stats` UiEvents). */
  tokens?: number;
  /** Latest cumulative session cost in USD (from `stats` UiEvents). */
  cost?: number;
}

/** The one mapping from a live agent to its cheap roster view. */
export function agentIdentity(agent: Agent): AgentIdentity {
  return {
    handle: agent.handle,
    // Resolved, never passed through: the stored field is optional for older persisted
    // rosters, but the view states it outright.
    ownership: agent.ownership ?? 'owned',
    persona: agent.persona,
    model: agent.model,
    invitedBy: agent.invitedBy,
    idle: agent.idle,
    stopped: agent.stopped,
  };
}

/** The one mapping from a live agent to its detailed observer view — every status/
 *  archive producer uses this so new view fields appear everywhere at once. */
export function agentView(agent: Agent): AgentView {
  const attached = agent.ownership === 'attached';
  return {
    ...agentIdentity(agent),
    piSessionId: attached ? undefined : agent.piSessionId,
    piSessionFile: attached ? undefined : agent.piSessionFile,
    tokens: agent.tokens,
    cost: agent.cost,
  };
}

/** A kild's cost rollup, summed over the agents that have reported `stats`. */
export interface CostTotals {
  tokens: number;
  cost: number;
}

/** Sum agent costs into a kild total — undefined until at least one agent has reported
 *  stats, so payloads without cost data stay clean of zero-noise. */
export function costTotals(
  agents: Array<Pick<AgentView, 'tokens' | 'cost'>>,
): CostTotals | undefined {
  const reported = agents.filter((a) => a.tokens !== undefined || a.cost !== undefined);
  if (reported.length === 0) return undefined;
  return {
    tokens: reported.reduce((sum, a) => sum + (a.tokens ?? 0), 0),
    cost: reported.reduce((sum, a) => sum + (a.cost ?? 0), 0),
  };
}

/** One directed communication: a sender, its named recipients, and the text. There is
 *  no engine-generated message kind — roster changes and teardown are events, not
 *  entries on this log. */
export interface Message {
  id: string;
  kildId: string;
  /** Handle of the sender. */
  from: string;
  /** The recipients the SENDER named. Never empty, never inferred. */
  to: string[];
  text: string;
  /** Epoch millis, stamped by the engine on receipt. Wall-clock, so it can go BACKWARDS
   *  (NTP, DST, a clock the user set) — never use it as a cursor; that is {@link seq}. */
  ts: number;
  /** The message cursor: strictly increasing within a kild, assigned by the registry from
   *  the log's own tail, never reused. Persisted with the log, so it survives a reload —
   *  which is what makes `?since=<seq>` a real cursor rather than a guess. */
  seq: number;
}

/** A message before the registry stamps its cursor. `seq` belongs to the log's order, so
 *  only the thing that owns the log may assign it (see {@link Message.seq}). */
export type MessageDraft = Omit<Message, 'seq'>;

/** A live kild: agents + their message history + a workspace. Liveness is registry
 *  presence — in the registry it is live, archived it is stopped. */
export interface Kild {
  id: string;
  name: string;
  /** Project directory the agents run in (their cwd). */
  cwd: string;
  /** Optional shared worktree name — every agent attaches to `kild/<name>`. */
  worktree?: string;
  /** Base branch the worktree was created from and that git status/collisions are
   *  measured against (so ahead/behind and changed files reflect this kild's own
   *  work, not everything the base is ahead of `main`). */
  base?: string;
  agents: Agent[];
  log: Message[];
  /** Merge commit this kild's branch landed as, recorded by `POST /api/kilds/:id/land`.
   *  The ledger prefers it over inferring containment from git, so a landed run can name
   *  the sha it became instead of only asserting it is contained. */
  landedSha?: string;
  /** What the land carried, counted at the moment it merged.
   *
   *  Measuring afterwards is measuring the wrong thing: `base..HEAD` is empty once the
   *  branch is in base, so the ledger reported `code: 0 commits, 0 files changed` beside
   *  `landed: yes` — the kild that did the most work recorded the least. The land is the
   *  only moment these numbers exist, so it is the moment they are captured. */
  landed?: { commits: number; files: number };
}

/** An agent to spawn into a kild. */
export interface AgentSpec {
  handle: string;
  persona?: string;
  model?: string;
  /** Absolute path of an existing pi session file to seed this agent from. The agent COPIES
   *  that file's history into a brand-new session file, so the source is never written and
   *  two writers on one conversation are impossible — which is what makes forking a LIVE
   *  agent's session safe. A fork is a snapshot: it diverges from the moment it is taken and
   *  does not follow the original. */
  forkFrom?: string;
}

/**
 * Who is spawning an agent, and what they are spawning it to do. The two travel together
 * because the second is meaningless without the first: a task is a MESSAGE, and a message
 * has a sender.
 *
 * `task` is delivery convenience, not a new concept and not a stored field — it fuses
 * `spawn` + `send` the way `POST /api/kilds`'s `kickoff` already does for create, because a
 * spawned agent with no first message just sits there. The record of what an agent was
 * asked to do is message `seq 1` on the kild log, where a later revision of that task can
 * follow it. A copy on the roster would be a second answer that goes stale the moment the
 * spawner says "actually, do X instead".
 */
export interface SpawnContext {
  /** The spawner's handle — the ground-truth spawn edge (stored as
   *  {@link AgentBase.invitedBy}) and the `from` of `task`. Engine-derived at every call
   *  site: an agent's control line carries its own handle, and a REST caller is resolved
   *  from its credential (never asserted), because a sender a caller chose for itself is
   *  a forgery, not an identity. */
  invitedBy?: string;
  /** First message to deliver to the new agent, sent from {@link invitedBy} as an ordinary
   *  directed message. Requires a sender. */
  task?: string;
}

/** Spec to create a kild: who the agents are and where they run. */
export interface NewKildSpec {
  name: string;
  cwd: string;
  agents: AgentSpec[];
  /** Optional shared worktree — every agent attaches to one `kild/<name>` tree. */
  worktree?: string;
  /** Base branch for the worktree + git-status baseline (default: the checkout's current
   *  branch). Editable via `.kild/config.json` `baseBranch` or the `--base` CLI flag. */
  base?: string;
}

/** Lightweight kild descriptor for client lists. Its presence in a `{kilds}` broadcast
 *  IS the liveness signal — there is no state field to consult. */
export interface KildSummary {
  id: string;
  name: string;
  worktree?: string;
  agents: AgentView[];
}

/** A stopped kild as the wire serves it: its record, WITHOUT the log.
 *
 *  The log is `GET /api/kilds/:id/messages`, which works for an archived kild precisely
 *  because its log is the whole of what it still is. Carrying it in the listing as well is
 *  the duplication the cheap/costly split deleted for live kilds — and it is worse here,
 *  because the archive only grows: on a machine with hundreds of stopped kilds, "list the
 *  archive" meant "send me every conversation I have ever had". One rule, no exceptions:
 *  **no listing and no broadcast carries a log.** */
export type ArchivedKildView = Omit<ArchivedKild, 'log'>;

/** The one mapping from a stored archive to its wire view. */
export function archivedKildView(archived: ArchivedKild): ArchivedKildView {
  const { log: _log, ...view } = archived;
  return view;
}

/** A kild recovered from disk after an engine restart — its conversation log with no
 *  live agents (their sessions are gone). This is the STORED shape (and what the registry
 *  holds in memory); {@link ArchivedKildView} is what goes on the wire. */
export interface ArchivedKild {
  id: string;
  name: string;
  worktree?: string;
  agents: AgentView[];
  log: Message[];
  /** Project directory the kild ran in — persisted so archived kilds stay attributable
   *  to their project after the worktree is pruned. Optional: older files predate it. */
  cwd?: string;
  /** Base branch the kild measured against. Optional: older files predate it. */
  base?: string;
  /** Merge commit the kild landed as, when it was landed through the engine. */
  landedSha?: string;
  /** What that land carried, counted when it merged (see {@link Kild.landed}). */
  landed?: { commits: number; files: number };
}

/**
 * A kild's identity and structure — the CHEAP half of the listing.
 *
 * Everything here is already in memory (or falls out of the one
 * `git worktree list --porcelain` the enumeration needs anyway), so listing every kild on
 * the machine costs a constant number of git invocations rather than one batch per kild.
 * Git state, cost and the message log are the other half ({@link KildStatus},
 * `GET /api/kilds/:id/messages`) precisely because they are not free.
 */
export interface KildIdentity {
  id: string;
  name: string;
  /** Project directory the agents run in (their cwd) — the repo an orphan tree belongs to. */
  cwd: string;
  worktree?: string;
  /** Base branch this kild measures against. Absent for an orphan: its record is gone, so
   *  nothing recorded a base and only git could guess one. */
  base?: string;
  agents: AgentIdentity[];
  /** True for a kild that exists only as a `kild/*` worktree on disk: enumerated from
   *  git because its kild record is gone (an older engine run, lost state). It has no
   *  agents and no log, and is addressed by its WORKTREE NAME — without this it would
   *  have no id at all, which is how trees became permanently unreclaimable. */
  orphan?: boolean;
}

/** A kild's identity enriched with everything that costs something to know: its
 *  git/worktree state (the code-state half of observability, so a driving agent can land
 *  work and avoid collisions) and what it has spent. Git is live-only (never persisted);
 *  computed on demand when serving status. Carries NO log — that is its own cursored
 *  resource. */
export interface KildStatus extends KildIdentity {
  agents: AgentView[];
  git?: KildGitStatus;
  /** Kild cost rollup summed over agents — absent until stats have arrived. */
  totals?: CostTotals;
  /** Merge commit this kild's branch landed as (see {@link Kild.landedSha}). */
  landedSha?: string;
  /** What that land carried, counted when it merged (see {@link Kild.landed}). */
  landed?: { commits: number; files: number };
}

/** The identity half of one live kild — the single mapping every cheap-listing producer
 *  uses, mirroring {@link agentIdentity}. */
export function kildIdentity(kild: Kild): KildIdentity {
  return {
    id: kild.id,
    name: kild.name,
    cwd: kild.cwd,
    worktree: kild.worktree,
    base: kild.base,
    agents: kild.agents.map(agentIdentity),
  };
}

/** Typed kild-domain result: every command either succeeds with a value or fails with
 *  an explicit kild error code + message. */
export type CommandResult<T> =
  | { ok: true; value: T }
  | { ok: false; code: KildErrorCode; message: string };

/** Kild-domain failure categories — transport-agnostic, mapped by REST/agent layers. */
export type KildErrorCode = 'not_found' | 'invalid_state' | 'rejected';

export interface KildActionSuccess {
  message: string;
  /** For sends: the named recipients other than the sender. Empty = the message reached
   *  no one, e.g. a self-addressed send. */
  deliveredTo?: string[];
  /** For sends that grew the kild: the handles this send brought in (see
   *  {@link CreateMissing}). Absent when every recipient already existed. */
  created?: string[];
}

export interface NewKildSuccess extends KildActionSuccess {
  kildId: string;
}

/** What the engine broadcasts to clients about kilds. */
export type KildOutbound =
  | { message: Message }
  | { kilds: KildSummary[] }
  /** A kild that just stopped — pushed so clients move it to read-only history immediately,
   *  without refetching the archive or restarting. Carries no log: a subscribed client
   *  already received every `{message}` frame, and one that did not reads `/messages`. */
  | { archivedKild: ArchivedKildView }
  /** The close lifecycle event: the facts the engine holds about a kild that just
   *  stopped, emitted alongside whatever `hooks.onClose` runs — so a subscribed client
   *  or harness can react to the same moment on the same facts. */
  | { kildClosed: KildCloseEvent }
  /** An agent's transcript event (its UiEvent stream), tagged by kild+agent. */
  | { kild: string; agent: string; event: UiEvent };

/** Agent→engine control line: an agent called `send`. Distinct from a `UiEvent` —
 *  routed to the kild, not shown as the agent's raw transcript.
 *
 *  There is no `spawn` line beside it. An agent grows its kild by ADDRESSING someone new:
 *  a recipient the kild does not have is created and then delivered to, with `persona` and
 *  `model` describing who it should be (see {@link CreateMissing}). Spawning was never the
 *  goal — every real delegation was spawn-then-send, and a spawn on its own produced an
 *  agent sitting idle. */
export interface SendOut {
  kind: 'send';
  requestId?: string;
  text: string;
  /** The recipients the sending agent named (structured, never parsed from the text).
   *  Required: the engine never infers a recipient, so an empty list is a rejection. */
  to: string[];
  /** Persona for any recipient that has to be created; defaults per handle. */
  persona?: string;
  /** Model for any recipient that has to be created. */
  model?: string;
}

/**
 * What a recipient becomes when the kild does not have it yet — passed by a caller that may
 * grow the kild, and absent for one that may not.
 *
 * Its presence is the whole difference between the two send paths: for an agent inside a
 * kild, naming someone new is how delegation starts, so `send` creates them; at the REST
 * boundary a typo'd handle must never quietly cost a process, so it stays a rejection naming
 * the roster. Nothing is read out of the message text in either case — the caller names the
 * handles as data.
 *
 * There is no per-recipient variant: when one call names several new handles they are all
 * created from this one spec (which is exactly the fan-out shape — N handles, one persona,
 * one brief). Different personas are different calls.
 */
export interface CreateMissing {
  /** Persona each created recipient runs. Omitted means "the handle is the persona name",
   *  the same resolution every other spawn path uses. */
  persona?: string;
  /** Model each created recipient runs on. */
  model?: string;
}

/** Agent→engine control line: an agent called `stop`. */
export interface StopOut {
  kind: 'stop';
  requestId?: string;
  reason?: string;
}

/** Engine→agent acknowledgement for an explicit kild command. */
export interface CommandAck {
  type: 'command_result';
  requestId: string;
  result: CommandResult<KildActionSuccess>;
}
