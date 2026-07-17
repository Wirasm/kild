/**
 * Extract `@mention` handles from a message body. A mention is `@` followed by a
 * name token (letters, digits, `-`, `_`). Deduped, order-preserving. Pure — the
 * one place that decides who a post addresses, so it stays testable in isolation.
 */
export function parseMentions(text: string): string[] {
  const seen = new Set<string>();
  const handles: string[] = [];
  // A mention starts a token (not `user@host`) and is not an npm scope (`@scope/pkg`).
  // The trailing guard forbids handle chars too, so backtracking can't shave the
  // handle to dodge the `/` check (`@scope/…` must not match as `@scop`).
  for (const match of text.matchAll(/(?<![\w.])@([A-Za-z0-9_-]+)(?![A-Za-z0-9_/-])/g)) {
    const handle = match[1];
    if (handle && !seen.has(handle)) {
      seen.add(handle);
      handles.push(handle);
    }
  }
  return handles;
}
