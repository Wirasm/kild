import { expect, test } from 'bun:test';

import {
  finalNonSystemPost,
  formatOperatorNotification,
  NO_FINAL_POST,
  openerNotificationTarget,
} from './room-events.ts';
import type { Room, RoomMessage } from './room-types.ts';

function room(overrides: Partial<Room> = {}): Room {
  return {
    id: 'room-1',
    name: 'ops',
    cwd: '/tmp',
    openedBy: 'brain-session',
    participants: [{ name: 'worker', sessionId: 'worker-session', agent: 'worker' }],
    log: [],
    state: 'running',
    ...overrides,
  };
}

function message(text: string, system = false): RoomMessage {
  return { id: text, roomId: 'room-1', from: 'worker', to: [], text, ts: 0, system };
}

test('formats a clearly labeled participant post to @human', () => {
  expect(
    formatOperatorNotification('ops', {
      kind: 'human_post',
      from: 'worker',
      text: '@human need a gate decision',
    }),
  ).toBe(
    "[kild operator notification] Room 'ops': @worker posted to @human: @human need a gate decision",
  );
});

test('formats halt and close with the final non-system post', () => {
  expect(
    formatOperatorNotification('ops', { kind: 'halted', finalPost: 'implementation committed' }),
  ).toBe(
    "[kild operator notification] Room 'ops' was halted. Final non-system post: implementation committed",
  );
  expect(
    formatOperatorNotification('ops', { kind: 'closed', finalPost: 'implementation committed' }),
  ).toBe(
    "[kild operator notification] Room 'ops' was closed and archived. Final non-system post: implementation committed",
  );
});

test('uses the final non-system post and the exact sentinel when none exists', () => {
  expect(
    finalNonSystemPost(room({ log: [message('work complete'), message('Room halted', true)] })),
  ).toBe('work complete');
  expect(finalNonSystemPost(room({ log: [message('Room halted', true)] }))).toBe(NO_FINAL_POST);
  expect(NO_FINAL_POST).toBe('(no non-system posts recorded)');
});

test('targets only a non-participant opener', () => {
  expect(openerNotificationTarget(room())).toBe('brain-session');
  expect(openerNotificationTarget(room({ openedBy: undefined }))).toBeUndefined();
  expect(openerNotificationTarget(room({ openedBy: 'worker-session' }))).toBeUndefined();
});
