import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import type { SearchProvider } from './search.ts';

/**
 * The agent-facing `web_search` tool. Backed by an injected {@link SearchProvider}
 * (SearXNG in v1) so the model — any model, including ones with no provider-native
 * search — can look things up. Registered only when a provider is configured.
 */
export function createWebSearchTool(provider: SearchProvider): ToolDefinition {
  return {
    name: 'web_search',
    label: 'Web Search',
    description:
      'Search the web for current/external information. Returns a ranked list of ' +
      '{title, url, snippet}. Follow up with `webfetch` on a result URL to read the page.',
    promptSnippet: 'web_search — query the web; then webfetch a result url to read it',
    parameters: Type.Object({
      query: Type.String({ description: 'The search query.' }),
      numResults: Type.Optional(
        Type.Number({ description: 'How many results to return (default 5, max 10).' }),
      ),
    }),
    async execute(_toolCallId, params) {
      const { query, numResults } = params as { query: string; numResults?: number };
      const limit = Math.min(10, Math.max(1, Math.floor(numResults ?? 5)));
      const hits = await provider.search(query, limit);
      const text = hits.length
        ? hits
            .map(
              (h, i) => `${i + 1}. [${h.title}](${h.url})${h.snippet ? `\n   ${h.snippet}` : ''}`,
            )
            .join('\n')
        : `No results for "${query}".`;
      return { content: [{ type: 'text' as const, text }], details: { count: hits.length } };
    },
  };
}
