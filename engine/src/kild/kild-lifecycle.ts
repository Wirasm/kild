import type { CommandResult, Kild, KildLifecycleState } from './kild-types.ts';

function fail<T>(message: string): CommandResult<T> {
  return { ok: false, code: 'invalid_state', message };
}

function inState(kild: Kild, states: KildLifecycleState[]): boolean {
  return states.includes(kild.state);
}

export function transitionKildState<T>(
  kild: Kild,
  next: KildLifecycleState,
): CommandResult<T | undefined> {
  const allowed =
    (kild.state === 'opening' && next === 'running') ||
    (kild.state === 'running' && (next === 'halted' || next === 'closed')) ||
    (kild.state === 'halted' && next === 'closed');
  if (!allowed) {
    return fail(`kild '${kild.name}' is ${kild.state}`);
  }
  kild.state = next;
  return { ok: true, value: undefined };
}

export function ensureKildCanSpawnAgent<T>(kild: Kild): CommandResult<T | undefined> {
  if (inState(kild, ['running'])) return { ok: true, value: undefined };
  if (kild.state === 'halted') return fail(`kild '${kild.name}' is halted`);
  return fail(`kild '${kild.name}' is ${kild.state}`);
}

export function ensureKildCanSend<T>(
  kild: Kild,
  opts: { allowHalted?: boolean; allowClosed?: boolean } = {},
): CommandResult<T | undefined> {
  if (kild.state === 'running') return { ok: true, value: undefined };
  if (kild.state === 'halted' && opts.allowHalted) return { ok: true, value: undefined };
  if (kild.state === 'closed' && opts.allowClosed) return { ok: true, value: undefined };
  if (kild.state === 'halted') return fail(`kild '${kild.name}' is halted`);
  return fail(`kild '${kild.name}' is ${kild.state}`);
}

export function ensureKildCanHalt<T>(kild: Kild): CommandResult<T | undefined> {
  if (kild.state === 'running') return { ok: true, value: undefined };
  if (kild.state === 'halted') return fail(`kild '${kild.name}' is already halted`);
  return fail(`kild '${kild.name}' is ${kild.state}`);
}

export function ensureKildCanStopFromOperator<T>(kild: Kild): CommandResult<T | undefined> {
  if (inState(kild, ['running', 'halted'])) return { ok: true, value: undefined };
  return fail(`kild '${kild.name}' is ${kild.state}`);
}

export function ensureKildCanStopFromAgent<T>(kild: Kild): CommandResult<T | undefined> {
  if (kild.state === 'running') return { ok: true, value: undefined };
  if (kild.state === 'halted') return fail(`kild '${kild.name}' is halted`);
  return fail(`kild '${kild.name}' is ${kild.state}`);
}
