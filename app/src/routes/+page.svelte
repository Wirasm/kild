<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  type Project = { name: string; path: string };

  type Item =
    | { type: "user"; text: string }
    | { type: "assistant"; text: string }
    | { type: "tool"; name: string; args: string; status: "running" | "ok" | "error" };

  type UiEvent =
    | { kind: "model"; provider: string; id: string }
    | { kind: "text"; delta: string }
    | { kind: "tool_start"; name: string; args: string }
    | { kind: "tool_end"; name: string; ok: boolean }
    | { kind: "retry"; attempt: number; max: number }
    | { kind: "agent_end" }
    | { kind: "stats"; tokens: number; cost: number; context_pct: number | null }
    | { kind: "session_end" };

  // Curated list for now; a `list_models` command (pi --list-models) comes later.
  const MODELS = ["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-8", "gpt-5.5", "MiniMax-M3"];

  let projects = $state<Project[]>([]);
  let active = $state<Project | null>(null);
  let adding = $state(false);
  let newName = $state("");
  let newPath = $state("");
  let addError = $state<string | null>(null);

  let model = $state(MODELS[0]);
  let items = $state<Item[]>([]);
  let input = $state("");
  let running = $state(false);
  let modelLabel = $state<string | null>(null);
  let stats = $state<{ tokens: number; cost: number; context_pct: number | null } | null>(null);
  let transcriptEl: HTMLElement | undefined = $state();
  let unlisten: UnlistenFn | null = null;

  function scrollDown() {
    queueMicrotask(() => transcriptEl?.scrollTo({ top: transcriptEl.scrollHeight }));
  }

  function appendText(delta: string) {
    const last = items[items.length - 1];
    if (last && last.type === "assistant") last.text += delta;
    else items.push({ type: "assistant", text: delta });
    scrollDown();
  }

  function handle(ev: UiEvent) {
    switch (ev.kind) {
      case "model":
        modelLabel = `${ev.provider} / ${ev.id}`;
        break;
      case "text":
        appendText(ev.delta);
        break;
      case "tool_start":
        items.push({ type: "tool", name: ev.name, args: ev.args, status: "running" });
        scrollDown();
        break;
      case "tool_end":
        for (let i = items.length - 1; i >= 0; i--) {
          const it = items[i];
          if (it.type === "tool" && it.name === ev.name && it.status === "running") {
            it.status = ev.ok ? "ok" : "error";
            break;
          }
        }
        break;
      case "stats":
        stats = { tokens: ev.tokens, cost: ev.cost, context_pct: ev.context_pct };
        break;
      case "agent_end":
      case "session_end":
        running = false;
        break;
      case "retry":
        break;
    }
  }

  async function loadProjects() {
    projects = await invoke<Project[]>("list_projects");
  }

  async function newSession() {
    if (!active) return;
    items = [];
    stats = null;
    modelLabel = null;
    running = false;
    await invoke("spawn_session", { model, cwd: active.path });
  }

  async function selectProject(p: Project) {
    active = p;
    await newSession();
  }

  async function addProject() {
    addError = null;
    try {
      const p = await invoke<Project>("add_project", { name: newName.trim(), path: newPath.trim() });
      await loadProjects();
      adding = false;
      newName = "";
      newPath = "";
      await selectProject(p);
    } catch (e) {
      addError = String(e);
    }
  }

  async function send() {
    const text = input.trim();
    if (!text || running || !active) return;
    items.push({ type: "user", text });
    input = "";
    running = true;
    scrollDown();
    await invoke("send_prompt", { text });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  onMount(() => {
    let alive = true;
    listen<UiEvent>("pi-event", (e) => handle(e.payload)).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });
    loadProjects().then(() => {
      if (alive && projects.length > 0) selectProject(projects[0]);
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  });
</script>

<div class="app">
  <aside class="sidebar">
    <div class="brand">kild</div>

    <div class="section-label">Projects</div>
    {#each projects as p}
      <button class="project" class:active={active?.name === p.name} onclick={() => selectProject(p)}>
        <span class="p-name">{p.name}</span>
        <span class="p-path">{p.path}</span>
      </button>
    {/each}

    {#if adding}
      <div class="add-form">
        <input bind:value={newName} placeholder="name" />
        <input bind:value={newPath} placeholder="~/projects/my-app" />
        {#if addError}<div class="add-error">{addError}</div>{/if}
        <div class="add-actions">
          <button class="primary" onclick={addProject}>Add</button>
          <button onclick={() => (adding = false)}>Cancel</button>
        </div>
      </div>
    {:else}
      <button class="new" onclick={() => (adding = true)}>+ add project</button>
    {/if}

    {#if active}
      <div class="section-label">Session</div>
      <button class="session active" onclick={newSession}>new session ↻</button>
    {/if}
  </aside>

  <main class="main">
    {#if !active}
      <div class="empty">
        <h2>No project yet</h2>
        <p>Add a project directory to start chatting with an agent in it.</p>
        <button class="primary" onclick={() => (adding = true)}>+ add project</button>
      </div>
    {:else}
      <header class="topbar">
        <span class="project-chip">{active.name}</span>
        <select bind:value={model} onchange={newSession}>
          {#each MODELS as m}<option value={m}>{m}</option>{/each}
        </select>
        <span class="model">{modelLabel ?? "…"}</span>
        <span class="spacer"></span>
        {#if stats}
          <span class="gauge"
            >ctx {stats.context_pct ?? "–"}% · {stats.tokens} tok · ${stats.cost.toFixed(4)}</span
          >
        {/if}
      </header>

      <section class="transcript" bind:this={transcriptEl}>
        {#each items as item}
          {#if item.type === "user"}
            <div class="msg user">{item.text}</div>
          {:else if item.type === "assistant"}
            <div class="msg assistant">{item.text}</div>
          {:else}
            <div class="tool {item.status}">
              <span class="tool-name">🔧 {item.name}</span>
              <span class="tool-args">{item.args}</span>
              <span class="tool-mark"
                >{item.status === "running" ? "…" : item.status === "ok" ? "✓" : "✗"}</span
              >
            </div>
          {/if}
        {/each}
        {#if running}<div class="thinking">▍</div>{/if}
      </section>

      <footer class="composer">
        <textarea
          bind:value={input}
          onkeydown={onKeydown}
          placeholder="Message the agent…  (Enter to send, Shift+Enter for newline)"
          rows="2"
        ></textarea>
        <button onclick={send} disabled={running}>Send</button>
      </footer>
    {/if}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 230px 1fr;
    height: 100vh;
  }
  .sidebar {
    background: var(--obsidian);
    border-right: 1px solid var(--border-subtle);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
  }
  .brand {
    font-weight: 600;
    color: var(--ice);
    letter-spacing: 0.5px;
    padding: 6px 8px 10px;
  }
  .section-label {
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    padding: 10px 8px 4px;
  }
  .sidebar button {
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-subtle);
    padding: 7px 9px;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
  }
  .project {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .project.active {
    background: var(--surface);
  }
  .project .p-name {
    color: var(--text-bright);
  }
  .project .p-path {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session.active {
    background: var(--surface);
    color: var(--text-bright);
  }
  .new {
    color: var(--text-muted);
    border: 1px dashed var(--border) !important;
  }
  .add-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    background: var(--surface);
    border-radius: 8px;
  }
  .add-form input {
    background: var(--void);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
    font-size: 12px;
  }
  .add-error {
    color: var(--ember);
    font-size: 11px;
  }
  .add-actions {
    display: flex;
    gap: 6px;
  }
  .primary {
    background: var(--ice) !important;
    color: var(--void) !important;
    font-weight: 600;
    border: none !important;
  }
  .main {
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-width: 0;
  }
  .empty {
    grid-row: 1 / -1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-subtle);
  }
  .empty h2 {
    color: var(--text-bright);
    margin: 0;
  }
  .empty .primary {
    padding: 8px 16px;
    border-radius: 8px;
    cursor: pointer;
    font: inherit;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--obsidian);
  }
  .project-chip {
    color: var(--ice);
    font-weight: 600;
  }
  .topbar select {
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
    font: inherit;
  }
  .topbar .model {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 12px;
  }
  .topbar .spacer {
    flex: 1;
  }
  .topbar .gauge {
    color: var(--text-subtle);
    font-family: var(--mono);
    font-size: 12px;
  }
  .transcript {
    overflow-y: auto;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .msg {
    max-width: 70ch;
    padding: 10px 14px;
    border-radius: 10px;
    white-space: pre-wrap;
    line-height: 1.55;
  }
  .msg.user {
    align-self: flex-end;
    background: var(--ice-dim);
    color: var(--void);
  }
  .msg.assistant {
    align-self: flex-start;
    background: var(--surface);
    color: var(--text-bright);
  }
  .tool {
    align-self: flex-start;
    display: flex;
    gap: 10px;
    align-items: center;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-subtle);
    background: var(--elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 6px 12px;
  }
  .tool .tool-name {
    color: var(--ice);
  }
  .tool .tool-args {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 48ch;
    white-space: nowrap;
  }
  .tool.ok .tool-mark {
    color: var(--aurora);
  }
  .tool.error .tool-mark {
    color: var(--ember);
  }
  .thinking {
    color: var(--ice);
    animation: blink 1s steps(2) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
  .composer {
    display: flex;
    gap: 10px;
    padding: 14px 16px;
    border-top: 1px solid var(--border-subtle);
    background: var(--obsidian);
  }
  .composer textarea {
    flex: 1;
    resize: none;
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    font: inherit;
  }
  .composer button {
    background: var(--ice);
    color: var(--void);
    border: none;
    border-radius: 8px;
    padding: 0 18px;
    font-weight: 600;
    cursor: pointer;
  }
  .composer button:disabled {
    background: var(--border);
    color: var(--text-muted);
    cursor: default;
  }
</style>
