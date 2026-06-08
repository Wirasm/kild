import { afterEach, expect, test } from 'bun:test';

import { searxng, webSearchProvider } from './search.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const respond = (body: unknown, status = 200): typeof fetch =>
  (async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    })) as typeof fetch;

test('searxng maps results to {title,url,snippet} and caps to the limit', async () => {
  globalThis.fetch = respond({
    results: [
      { title: 'A', url: 'https://a.com', content: 'snippet a' },
      { title: 'B', url: 'https://b.com', content: 'snippet b' },
      { title: 'C', url: 'https://c.com', content: 'snippet c' },
    ],
  });
  const hits = await searxng('http://searx.local').search('q', 2);
  expect(hits).toHaveLength(2);
  expect(hits[0]).toEqual({ title: 'A', url: 'https://a.com', snippet: 'snippet a' });
});

test('searxng falls back title→url + empty snippet, and drops rows without a url', async () => {
  globalThis.fetch = respond({ results: [{ url: 'https://c.com' }, { title: 'no url here' }] });
  const hits = await searxng('http://searx.local').search('q', 10);
  expect(hits).toEqual([{ title: 'https://c.com', url: 'https://c.com', snippet: '' }]);
});

test('searxng throws a helpful error on a non-200', async () => {
  globalThis.fetch = respond({}, 403);
  await expect(searxng('http://searx.local').search('q', 5)).rejects.toThrow(/searxng 403/);
});

test('webSearchProvider is null without KILD_SEARXNG_URL, set with it', () => {
  const prev = process.env.KILD_SEARXNG_URL;
  delete process.env.KILD_SEARXNG_URL;
  expect(webSearchProvider()).toBeNull();
  process.env.KILD_SEARXNG_URL = 'http://searx.local';
  expect(webSearchProvider()).not.toBeNull();
  if (prev === undefined) delete process.env.KILD_SEARXNG_URL;
  else process.env.KILD_SEARXNG_URL = prev;
});
