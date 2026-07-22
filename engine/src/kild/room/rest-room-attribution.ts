import type { CommandResult } from './room-types.ts';
import { HUMAN } from './room-types.ts';

export interface OpenRoomAttributionInput {
  openedBy?: string;
  from?: string;
}

export interface PostRoomAttributionInput {
  sessionId?: string;
  from?: string;
}

export interface CloseRoomAttributionInput {
  sessionId?: string;
  from?: string;
}

export interface RestRoomAttributionSuccess {
  actor: string;
  human: boolean;
}

interface RestRoomAttributionDeps {
  resolveActor(sessionId: string): CommandResult<string>;
}

function ok(actor: string): CommandResult<RestRoomAttributionSuccess> {
  return { ok: true, value: { actor, human: actor === HUMAN } };
}

function reject(): CommandResult<RestRoomAttributionSuccess> {
  return {
    ok: false,
    code: 'rejected',
    message: 'from is not allowed; actor identity is engine-derived',
  };
}

function resolveSessionActor(
  sessionId: string | undefined,
  from: string | undefined,
  deps: RestRoomAttributionDeps,
): CommandResult<RestRoomAttributionSuccess> {
  if (from !== undefined) return reject();
  if (sessionId === undefined) return ok(HUMAN);
  const actor = deps.resolveActor(sessionId);
  return actor.ok ? ok(actor.value) : actor;
}

export function resolveOpenRoomActor(
  input: OpenRoomAttributionInput,
  deps: RestRoomAttributionDeps,
): CommandResult<RestRoomAttributionSuccess> {
  return resolveSessionActor(input.openedBy, input.from, deps);
}

export function resolvePostRoomActor(
  input: PostRoomAttributionInput,
  deps: RestRoomAttributionDeps,
): CommandResult<RestRoomAttributionSuccess> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}

export function resolveCloseRoomActor(
  input: CloseRoomAttributionInput,
  deps: RestRoomAttributionDeps,
): CommandResult<RestRoomAttributionSuccess> {
  return resolveSessionActor(input.sessionId, input.from, deps);
}
