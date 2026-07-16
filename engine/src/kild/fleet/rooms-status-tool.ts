import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { getLiveRooms } from './engine-client.ts';
import { compactLiveRooms } from './rooms-status.ts';

export function createRoomsStatusTool(): ToolDefinition {
  return {
    name: 'rooms_status',
    label: 'Rooms Status',
    description:
      'List live kild rooms as compact status summaries with participants and the last ' +
      'one or two posts.',
    promptSnippet: 'rooms_status — summarize live rooms and their latest posts',
    parameters: Type.Object({}),
    async execute() {
      const liveRooms = await getLiveRooms();
      const compact = compactLiveRooms(liveRooms);
      return {
        content: [{ type: 'text' as const, text: JSON.stringify(compact) }],
        details: null,
      };
    },
  };
}
