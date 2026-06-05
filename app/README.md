# kild cockpit

The kild desktop UI — SvelteKit (Svelte 5 runes) in a thin Tauri shell. It talks
to the kild **engine** (`../engine`) over HTTP + WebSocket; the engine owns all
agent sessions. The only Rust here is the window bootstrap (`src-tauri`).

## Develop

```bash
bun install
bun run tauri dev   # builds the engine sidecar, starts engine + frontend, opens the window
```

- `bun run dev` — just the Vite frontend on :1420 (needs the engine running separately).
- `bun run check` — svelte-check.
- `bun run build` — static frontend → `build/`.

Override the engine URL with `VITE_KILD_ENGINE` (default `http://localhost:4517`).
See the root `CLAUDE.md` for the architecture and `src/lib/api.ts` for the
engine client + the `{session, event}` / `{sessions}` wire protocol.
