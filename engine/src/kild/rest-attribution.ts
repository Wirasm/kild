import type { AttachIdentity } from './attach-token.ts';
import type { CommandResult } from './kild-types.ts';

/**
 * Who a REST call is *from*. The engine derives it — a caller may not assert a `from`,
 * because a name it chose is not an identity.
 *
 * The answer is always a **handle**, never a persona: the handle is what recipients
 * address and what `to[]` names, so anything else makes the log lie about who spoke. Two
 * agents running one persona have two handles, and an agent whose handle is `reviewer` and
 * whose persona is `coder` is `reviewer` on the log.
 *
 * Two credentials produce one, and a caller presents at most one of them:
 * - a **kild session id** — the handle of the agent kild runs in that session, which is
 *   kild-level knowledge and is therefore answered by the kild manager;
 * - a **bearer token minted at `attach`** — the handle a harness kild does not own claimed.
 *   An attached agent has no kild session by definition, which is why it needs this: it is
 *   the only way it can name itself as the thing it is already addressed as.
 *
 * A caller with neither gets {@link UNATTRIBUTED}: a label written on the message, not a
 * participant in the kild. Nothing in delivery treats it specially — if you want to
 * *receive* messages you attach a handle like any other agent. The CLI, curl and helm
 * present no credential and land here, unchanged.
 */
export const UNATTRIBUTED = 'human';

/** What a route hands attribution: the credentials presented, and the kild addressed. */
interface AttributionInput {
  /** The kild the request addresses, when it addresses one. A token is scoped to a single
   *  kild, so this is what the scope is checked against. */
  kildId?: string;
  /** A kild session id (`KILD_SESSION_ID`) — the agent-side credential. */
  sessionId?: string;
  /** A bearer token from `Authorization: Bearer <token>` — the attached-harness credential. */
  token?: string;
  /** A caller-asserted sender. Always a rejection; kept in the shape so the rejection is
   *  explicit rather than a silently ignored field. */
  from?: string;
}

export interface NewKildAttributionInput {
  openedBy?: string;
  token?: string;
  from?: string;
}

export interface SendAttributionInput {
  kildId: string;
  sessionId?: string;
  token?: string;
  from?: string;
}

export interface StopAttributionInput {
  kildId: string;
  sessionId?: string;
  token?: string;
  from?: string;
}

interface RestAttributionDeps {
  /** The handle the agent running this kild session speaks as. Kild-level knowledge: a
   *  handle belongs to an agent's membership of a kild, not to the session substrate. */
  handleForSession(sessionId: string): CommandResult<string>;
  /** The (kild, handle) a bearer token stands for; undefined when nothing minted it. */
  identifyToken(token: string): AttachIdentity | undefined;
}

function reject(message: string): CommandResult<string> {
  return { ok: false, code: 'rejected', message };
}

/** Resolve a bearer token to the handle it was minted for.
 *
 *  A token the engine cannot place is REJECTED, never downgraded to the unattributed
 *  label: a caller that presented a credential meant to be identified, and silently
 *  writing someone else's name on its message would be the worse failure. Same for a token
 *  from another kild — a handle is unique within one kild and means nothing outside it. */
function resolveTokenActor(
  token: string,
  kildId: string | undefined,
  deps: RestAttributionDeps,
): CommandResult<string> {
  const identity = deps.identifyToken(token);
  if (!identity) return reject('unknown attach token');
  // `kildId` is absent only when creating a kild — there is no kild yet to be scoped to,
  // and the kickoff records who opened it exactly as `openedBy` already does across kilds.
  if (kildId !== undefined && identity.kildId !== kildId) {
    return reject(`attach token is not for kild ${kildId}`);
  }
  return { ok: true, value: identity.handle };
}

function resolveActor(input: AttributionInput, deps: RestAttributionDeps): CommandResult<string> {
  if (input.from !== undefined) {
    return reject('from is not allowed; actor identity is engine-derived');
  }
  // The token wins when both are presented. It is scoped to the very kild being addressed,
  // while a session id names its bearer's handle in whatever kild that session belongs to —
  // possibly a different one. The nearer identity is the truer one.
  if (input.token !== undefined) return resolveTokenActor(input.token, input.kildId, deps);
  if (input.sessionId === undefined) return { ok: true, value: UNATTRIBUTED };
  return deps.handleForSession(input.sessionId);
}

export function resolveNewKildActor(
  input: NewKildAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveActor({ sessionId: input.openedBy, token: input.token, from: input.from }, deps);
}

export function resolveSendActor(
  input: SendAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveActor(input, deps);
}

export function resolveStopActor(
  input: StopAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveActor(input, deps);
}
