# SearXNG — the search backend for kild's `web_search`

kild's `web_search` tool calls a **self-hosted SearXNG** over its keyless JSON API.
kild only *points at* it (`KILD_SEARXNG_URL`); it never runs or supervises the
container — that's you, here.

## Start it

```bash
cd infra/searxng
docker compose up -d
export KILD_SEARXNG_URL=http://localhost:8888   # add to your shell profile
```

Verify (no agent tokens spent):

```bash
curl -s "http://localhost:8888/search?q=hello&format=json" | head -c 300
cd ../../engine && bun run cli -- web search "anthropic claude opus" --json
```

## Notes

- **Without `KILD_SEARXNG_URL`**, `web_search` is simply not offered to agents (the
  worker logs a one-line notice). `webfetch` still works — it's in-process, no backend.
- Edit `settings.yml` to tune engines (it merges with SearXNG's defaults). Drop
  `google` if it gets CAPTCHA-throttled; brave / duckduckgo / startpage are reliable.
- Change `server.secret_key` for anything past local dev.
- The instance is loopback-only and the limiter is disabled because it serves a
  single local user/agent. Do **not** expose it publicly without re-enabling the
  limiter + adding auth/TLS.

## Other backends (later)

`web_search` sits behind a `SearchProvider` seam (`engine/src/kild/web/search.ts`).
Adding DuckDuckGo, fastCRW, Tavily, etc. is a new impl + a `webSearchProvider()`
switch arm — no changes to the tool or the worker.
