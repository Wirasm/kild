import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { closeRoom } from './engine-client.ts';

export function createFleetCloseRoomTool(): ToolDefinition {
  return {
    name: 'close_room',
    label: 'Close Room',
    description:
      'Close a live kild room through the engine REST API. The engine stops every ' +
      'participant and archives the transcript.',
    promptSnippet: 'close_room — close a live room by id',
    parameters: Type.Object({
      roomId: Type.String({ description: 'The live room id.' }),
    }),
    async execute(_toolCallId, params) {
      const { roomId } = params as { roomId: string };
      const { message } = await closeRoom(roomId, process.env.KILD_SESSION_ID);
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
