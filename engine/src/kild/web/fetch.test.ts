import { afterEach, expect, test } from 'bun:test';

import { fetchUrl } from './fetch.ts';

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

const serve = (body: string, contentType = 'text/html'): typeof fetch =>
  (async () =>
    new Response(body, { status: 200, headers: { 'content-type': contentType } })) as typeof fetch;

test('converts HTML to markdown by default', async () => {
  globalThis.fetch = serve('<html><body><h1>Hi</h1><p>Para</p></body></html>');
  const md = await fetchUrl('https://example.com');
  expect(md).toContain('# Hi');
  expect(md).toContain('Para');
});

test('format=html returns the raw html', async () => {
  globalThis.fetch = serve('<h1>Hi</h1>');
  expect(await fetchUrl('https://example.com', 'html')).toContain('<h1>Hi</h1>');
});

test('non-html content is returned as-is (turndown would mangle it)', async () => {
  globalThis.fetch = serve('{"a":1}', 'application/json');
  expect(await fetchUrl('https://api.example.com/x')).toBe('{"a":1}');
});

test('truncates very large output', async () => {
  globalThis.fetch = serve(`<html><body>${'x'.repeat(60 * 1024)}</body></html>`);
  const out = await fetchUrl('https://example.com');
  expect(out).toContain('[truncated at');
  expect(out.length).toBeLessThan(55 * 1024);
});

test('rejects non-http(s) and private/loopback hosts before fetching', async () => {
  await expect(fetchUrl('ftp://x/y')).rejects.toThrow(/http/);
  await expect(fetchUrl('http://localhost/x')).rejects.toThrow(/private|loopback/);
  await expect(fetchUrl('http://127.0.0.1/x')).rejects.toThrow(/private|loopback/);
  await expect(fetchUrl('http://192.168.1.5/x')).rejects.toThrow(/private|loopback/);
  await expect(fetchUrl('http://10.0.0.1/x')).rejects.toThrow(/private|loopback/);
});
