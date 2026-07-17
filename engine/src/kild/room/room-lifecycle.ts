import type { CommandResult, Room, RoomLifecycleState } from './room-types.ts';

function fail<T>(message: string): CommandResult<T> {
  return { ok: false, code: 'invalid_state', message };
}

function inState(room: Room, states: RoomLifecycleState[]): boolean {
  return states.includes(room.state);
}

export function transitionRoomState<T>(
  room: Room,
  next: RoomLifecycleState,
): CommandResult<T | undefined> {
  const allowed =
    (room.state === 'opening' && next === 'running') ||
    (room.state === 'running' && (next === 'halted' || next === 'closed')) ||
    (room.state === 'halted' && next === 'closed');
  if (!allowed) {
    return fail(`room '${room.name}' is ${room.state}`);
  }
  room.state = next;
  return { ok: true, value: undefined };
}

export function ensureRoomCanAddParticipant<T>(room: Room): CommandResult<T | undefined> {
  if (inState(room, ['running'])) return { ok: true, value: undefined };
  if (room.state === 'halted') return fail(`room '${room.name}' is halted`);
  return fail(`room '${room.name}' is ${room.state}`);
}

export function ensureRoomCanPost<T>(
  room: Room,
  opts: { allowHalted?: boolean; allowClosed?: boolean } = {},
): CommandResult<T | undefined> {
  if (room.state === 'running') return { ok: true, value: undefined };
  if (room.state === 'halted' && opts.allowHalted) return { ok: true, value: undefined };
  if (room.state === 'closed' && opts.allowClosed) return { ok: true, value: undefined };
  if (room.state === 'halted') return fail(`room '${room.name}' is halted`);
  return fail(`room '${room.name}' is ${room.state}`);
}

export function ensureRoomCanHalt<T>(room: Room): CommandResult<T | undefined> {
  if (room.state === 'running') return { ok: true, value: undefined };
  if (room.state === 'halted') return fail(`room '${room.name}' is already halted`);
  return fail(`room '${room.name}' is ${room.state}`);
}

export function ensureRoomCanCloseFromOperator<T>(room: Room): CommandResult<T | undefined> {
  if (inState(room, ['running', 'halted'])) return { ok: true, value: undefined };
  return fail(`room '${room.name}' is ${room.state}`);
}

export function ensureRoomCanCloseFromParticipant<T>(room: Room): CommandResult<T | undefined> {
  if (room.state === 'running') return { ok: true, value: undefined };
  if (room.state === 'halted') return fail(`room '${room.name}' is halted`);
  return fail(`room '${room.name}' is ${room.state}`);
}
