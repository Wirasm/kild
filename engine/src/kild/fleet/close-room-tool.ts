import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { closeRoom } from './engine-client.ts';

export function createFleetCloseRoomTool(): ToolDefinition {
  return {
    name: 'close_room',
    label: 'Close Room',
    description:
      'Close a live kild room through the engine REST API. The engine stops every ' +
      'participant and archives the transcript. A room with open decisions refuses to ' +
      'close: resolve each first (post "resolved[<key>]: <how>"), or pass force ONLY ' +
      'when the human explicitly says to abandon the open decisions.',
    promptSnippet: 'close_room — close a live room by id',
    parameters: Type.Object({
      roomId: Type.String({ description: 'The live room id.' }),
      force: Type.Optional(
        Type.Boolean({
          description:
            'Close past open decisions. Only on an explicit human instruction — this buries ' +
            'unresolved decisions.',
        }),
      ),
    }),
    async execute(_toolCallId, params) {
      const { roomId, force } = params as { roomId: string; force?: boolean };
      const { message } = await closeRoom(roomId, process.env.KILD_SESSION_ID, force);
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
