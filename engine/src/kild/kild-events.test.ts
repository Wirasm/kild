import { expect, test } from 'bun:test';

import {
  finalNonSystemPost,
  formatOperatorNotification,
  NO_FINAL_POST,
  openerNotificationTarget,
} from './kild-events.ts';
import type { Kild, Message } from './kild-types.ts';

function kild(overrides: Partial<Kild> = {}): Kild {
  return {
    id: 'kild-1',
    name: 'ops',
    cwd: '/tmp',
    openedBy: 'brain-session',
    agents: [{ handle: 'agent', id: 'agent-session', persona: 'agent' }],
    log: [],
    state: 'running',
    ...overrides,
  };
}

function message(text: string, system = false): Message {
  return { id: text, kildId: 'kild-1', from: 'agent', to: [], text, ts: 0, system };
}

test('formats a clearly labeled agent message to @human', () => {
  expect(
    formatOperatorNotification('ops', {
      kind: 'human_post',
      from: 'agent',
      text: '@human need a gate decision',
    }),
  ).toBe(
    "[kild operator notification] Kild 'ops': @agent sent to @human: @human need a gate decision",
  );
});

test('formats halt and stop with the final non-system post', () => {
  expect(
    formatOperatorNotification('ops', { kind: 'halted', finalPost: 'implementation committed' }),
  ).toBe(
    "[kild operator notification] Kild 'ops' was halted. Final non-system post: implementation committed",
  );
  expect(
    formatOperatorNotification('ops', { kind: 'closed', finalPost: 'implementation committed' }),
  ).toBe(
    "[kild operator notification] Kild 'ops' was stopped and archived. Final non-system post: implementation committed",
  );
});

test('uses the final non-system post and the exact sentinel when none exists', () => {
  expect(
    finalNonSystemPost(kild({ log: [message('work complete'), message('Kild halted', true)] })),
  ).toBe('work complete');
  expect(finalNonSystemPost(kild({ log: [message('Kild halted', true)] }))).toBe(NO_FINAL_POST);
  expect(NO_FINAL_POST).toBe('(no non-system posts recorded)');
});

test('targets only a non-agent opener', () => {
  expect(openerNotificationTarget(kild())).toBe('brain-session');
  expect(openerNotificationTarget(kild({ openedBy: undefined }))).toBeUndefined();
  expect(openerNotificationTarget(kild({ openedBy: 'agent-session' }))).toBeUndefined();
});
