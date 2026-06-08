/**
 * Extract `@mention` handles from a message body. A mention is `@` followed by a
 * name token (letters, digits, `-`, `_`). Deduped, order-preserving. Pure — the
 * one place that decides who a post addresses, so it stays testable in isolation.
 */
export function parseMentions(text: string): string[] {
  const seen = new Set<string>();
  const handles: string[] = [];
  for (const match of text.matchAll(/@([A-Za-z0-9_-]+)/g)) {
    const handle = match[1];
    if (handle && !seen.has(handle)) {
      seen.add(handle);
      handles.push(handle);
    }
  }
  return handles;
}
