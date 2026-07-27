import type { CommandResult } from './kild-types.ts';
import { HUMAN } from './kild-types.ts';

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

export interface RestAttributionSuccess {
  actor: string;
  human: boolean;
}

interface RestAttributionDeps {
  resolveActor(sessionId: string): CommandResult<string>;
}

function ok(actor: string): CommandResult<RestAttributionSuccess> {
  return { ok: true, value: { actor, human: actor === HUMAN } };
}

function reject(): CommandResult<RestAttributionSuccess> {
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
): CommandResult<RestAttributionSuccess> {
  if (from !== undefined) return reject();
  if (sessionId === undefined) return ok(HUMAN);
  const actor = deps.resolveActor(sessionId);
  return actor.ok ? ok(actor.value) : actor;
}

export function resolveNewKildActor(
  input: NewKildAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<RestAttributionSuccess> {
  return resolveSessionActor(input.openedBy, input.from, deps);
}

export function resolveSendActor(
  input: SendAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<RestAttributionSuccess> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}

export function resolveStopActor(
  input: StopAttributionInput,
  deps: RestAttributionDeps,
): CommandResult<RestAttributionSuccess> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}
