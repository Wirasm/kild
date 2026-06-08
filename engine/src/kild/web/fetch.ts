import TurndownService from 'turndown';

export type FetchFormat = 'markdown' | 'html';

const MAX_BYTES = 5 * 1024 * 1024; // 5 MB hard cap (mirrors opencode webfetch)
const MAX_OUTPUT = 50 * 1024; // truncate returned text to ~50 KB
const DEFAULT_TIMEOUT_MS = 30_000;
const MAX_TIMEOUT_MS = 120_000;
// A realistic desktop Chrome UA gets past most basic bot filters; some walls (e.g.
// Cloudflare) instead trust a plain identifying agent, so we retry once with that.
const BROWSER_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36';
const FALLBACK_UA = 'kild';

const turndown = new TurndownService({
  headingStyle: 'atx',
  hr: '---',
  bulletListMarker: '-',
  codeBlockStyle: 'fenced',
  emDelimiter: '*',
});
// Drop non-content elements entirely — otherwise their text (CSS, JS, <head> meta)
// leaks into the markdown.
turndown.remove(['script', 'style', 'head', 'noscript', 'iframe']);

/** Block fetches to loopback / link-local / private ranges so the agent can't be
 *  steered into the local network (basic SSRF guard). Literal-host check — does not
 *  defend against DNS rebinding; acceptable for a single-user loopback tool. */
function assertPublicHost(hostname: string): void {
  const h = hostname.toLowerCase().replace(/^\[|\]$/g, ''); // strip IPv6 brackets
  const blocked =
    h === 'localhost' ||
    h.endsWith('.localhost') ||
    h === '0.0.0.0' ||
    h === '::1' ||
    h.startsWith('127.') ||
    h.startsWith('10.') ||
    h.startsWith('169.254.') ||
    h.startsWith('192.168.') ||
    /^172\.(1[6-9]|2\d|3[01])\./.test(h) ||
    h.startsWith('fc') || // IPv6 unique-local
    h.startsWith('fd') ||
    h.startsWith('fe80:'); // IPv6 link-local
  if (blocked) throw new Error(`refusing to fetch a private/loopback host: ${hostname}`);
}

async function doFetch(url: string, ua: string, timeoutMs: number): Promise<Response> {
  return fetch(url, {
    headers: { 'user-agent': ua, accept: 'text/html,application/xhtml+xml,*/*' },
    redirect: 'follow',
    signal: AbortSignal.timeout(timeoutMs),
  });
}

/** Fetch a public URL and return its content as markdown (default) or raw html/text.
 *  Plain HTTP — no headless browser. Enforces http(s), a 5 MB cap, and a timeout. */
export async function fetchUrl(
  url: string,
  format: FetchFormat = 'markdown',
  timeoutSeconds?: number,
): Promise<string> {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(`invalid url: ${url}`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`only http(s) urls are supported: ${url}`);
  }
  assertPublicHost(parsed.hostname);

  const timeoutMs = Math.min(MAX_TIMEOUT_MS, (timeoutSeconds ?? DEFAULT_TIMEOUT_MS / 1000) * 1000);

  let res = await doFetch(url, BROWSER_UA, timeoutMs);
  if ((res.status === 403 || res.status === 429 || res.status === 503) && res.body)
    res.body.cancel?.();
  if (res.status === 403 || res.status === 429 || res.status === 503) {
    res = await doFetch(url, FALLBACK_UA, timeoutMs); // some walls trust a plain UA
  }
  if (!res.ok) throw new Error(`fetch ${res.status} for ${url}`);

  const len = Number(res.headers.get('content-length') ?? '0');
  if (len > MAX_BYTES) throw new Error(`response too large (${len} bytes > ${MAX_BYTES})`);

  const contentType = res.headers.get('content-type') ?? '';
  const raw = await res.text();
  if (raw.length > MAX_BYTES) throw new Error(`response too large (> ${MAX_BYTES} bytes)`);

  // Non-HTML (json, plain text, etc.) is returned as-is — turndown would mangle it.
  const isHtml = contentType.includes('html') || /<html[\s>]/i.test(raw);
  let out: string;
  if (format === 'html' || !isHtml) out = raw;
  else out = turndown.turndown(raw);

  if (out.length > MAX_OUTPUT) {
    out = `${out.slice(0, MAX_OUTPUT)}\n\n…[truncated at ${MAX_OUTPUT} bytes]`;
  }
  return out;
}
