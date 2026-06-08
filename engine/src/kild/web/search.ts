import { searxngUrl } from '../config.ts';

/** One web-search result. The model-facing tool renders these as a list. */
export interface SearchHit {
  title: string;
  url: string;
  snippet: string;
}

/** The search backend seam. v1 ships one impl (SearXNG); DDG / fastCRW / Tavily are
 *  user-selectable later by adding an impl + a `webSearchProvider()` switch arm —
 *  additive, no tool/worker rework. */
export interface SearchProvider {
  search(query: string, limit: number): Promise<SearchHit[]>;
}

/** A SearXNG-backed provider. kild talks to a self-hosted instance over its keyless
 *  JSON API (`/search?q=…&format=json`) — it does NOT run or manage the container. */
export function searxng(baseUrl: string): SearchProvider {
  return {
    async search(query, limit) {
      const u = new URL('/search', baseUrl);
      u.searchParams.set('q', query);
      u.searchParams.set('format', 'json');
      const r = await fetch(u, { signal: AbortSignal.timeout(25_000) });
      if (!r.ok) {
        throw new Error(
          `searxng ${r.status} — is it up and is the JSON format enabled? (see infra/searxng)`,
        );
      }
      const body = (await r.json()) as {
        results?: { title?: string; url?: string; content?: string }[];
      };
      return (body.results ?? [])
        .filter(
          (h): h is { url: string; title?: string; content?: string } => typeof h.url === 'string',
        )
        .slice(0, limit)
        .map((h) => ({ title: h.title ?? h.url, url: h.url, snippet: h.content ?? '' }));
    },
  };
}

/** The configured search provider, or null when web search is unavailable (no
 *  `KILD_SEARXNG_URL`). Callers register `web_search` only when this is non-null. */
export function webSearchProvider(): SearchProvider | null {
  const url = searxngUrl();
  return url ? searxng(url) : null;
}
