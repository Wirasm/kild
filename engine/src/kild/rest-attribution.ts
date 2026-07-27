import type { CommandResult } from './kild-types.ts';

/**
 * Who a REST call is *from*. The engine derives it — a caller may not assert a `from`,
 * because a name it chose is not an identity.
 *
 * A session-aware caller passes its session id and gets that session's persona. A caller
 * with no session identity gets {@link UNATTRIBUTED}: a label written on the message, not
 * a participant in the kild. Nothing in delivery treats it specially — if you want to
 * *receive* messages you attach a handle like any other agent.
 */
export const UNATTRIBUTED = 'human';

export interface NewKildAttributionInput {
  openedBy?: string;
  from?: string;
}

export interface SendAttributionInput {
  sessionId?: string;
  from?: string;
}

export interface StopAttributionInput {
  sessionId?: string;
  from?: string;
}

interface RestAttributionDeps {
  resolveActor(sessionId: string): CommandResult<string>;
}

function reject(): CommandResult<string> {
  return {
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  };
}

function resolveSessionActor(
  sessionId: string | undefined,
  from: string | undefined,
  deps: RestAttributionDeps,
): CommandResult<string> {
  if (from !== undefined) return reject();
  if (sessionId === undefined) return { ok: true, value: UNATTRIBUTED };
  return deps.resolveActor(sessionId);
}

export function resolveNewKildActor(
  input: NewKildAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveSessionActor(input.openedBy, input.from, deps);
}

export function resolveSendActor(
  input: SendAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}

export function resolveStopActor(
  input: StopAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<string> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}
