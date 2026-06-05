import { expect, test } from 'bun:test';

import { translate } from './events.ts';

test('text_delta becomes a text event', () => {
  expect(
    translate({
      type: 'message_update',
      assistantMessageEvent: { type: 'text_delta', delta: 'hi' },
    }),
  ).toEqual({ kind: 'text', delta: 'hi' });
});

test('non-text message updates are dropped', () => {
  expect(
    translate({
      type: 'message_update',
      assistantMessageEvent: { type: 'thinking_delta', delta: 'x' },
    }),
  ).toBeNull();
});

test('tool start/end map and carry the error flag', () => {
  expect(
    translate({ type: 'tool_execution_start', toolCallId: 'c1', toolName: 'bash', args: {} }),
  ).toMatchObject({ kind: 'tool_start', id: 'c1', name: 'bash' });
  expect(
    translate({ type: 'tool_execution_end', toolCallId: 'c1', toolName: 'bash', isError: true }),
  ).toEqual({
    kind: 'tool_end',
    id: 'c1',
    name: 'bash',
    ok: false,
  });
});

test('agent_end maps; unmodeled events are dropped (pi-upgrade safe)', () => {
  expect(translate({ type: 'agent_end' })).toEqual({ kind: 'agent_end' });
  expect(translate({ type: 'turn_start' })).toBeNull();
});
