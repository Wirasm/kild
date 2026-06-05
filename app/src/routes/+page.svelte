<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  type Project = { name: string; path: string };
  type Agent = { name: string; system_prompt: string };

  type Item =
    | { type: "user"; text: string }
    | { type: "assistant"; text: string }
    | { type: "tool"; id: string; name: string; args: string; status: "running" | "ok" | "error" };

  type UiEvent =
    | { kind: "model"; provider: string; id: string }
    | { kind: "text"; delta: string }
    | { kind: "tool_start"; id: string; name: string; args: string }
    | { kind: "tool_end"; id: string; name: string; ok: boolean }
    | { kind: "retry"; attempt: number; max: number }
    | { kind: "agent_end" }
    | { kind: "stats"; tokens: number; cost: number; context_pct: number | null }
    | { kind: "session_end" };

  type Session = {
    id: number;
    projectName: string;
    agent: string;
    model: string;
    items: Item[];
    running: boolean;
    status: "running" | "stopped";
    modelLabel: string | null;
    stats: { tokens: number; cost: number; context_pct: number | null } | null;
  };

  const MODELS = ["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-8", "gpt-5.5", "MiniMax-M3"];

  // Projects + new-session config
  let projects = $state<Project[]>([]);
  let active = $state<Project | null>(null); // active project = context for new sessions
  let adding = $state(false);
  let newName = $state("");
  let newPath = $state("");
  let addError = $state<string | null>(null);
  let agents = $state<Agent[]>([]);
  let agentName = $state("default");
  let model = $state(MODELS[0]);

  // Sessions (runtime registry)
  let sessions = $state<Session[]>([]);
  let activeId = $state<number | null>(null);
  let input = $state("");
  let error = $state<string | null>(null);
  let transcriptEl: HTMLElement | undefined = $state();
  let unlisten: UnlistenFn | null = null;

  let activeSession = $derived(sessions.find((s) => s.id === activeId) ?? null);

  function scrollDown() {
    queueMicrotask(() => transcriptEl?.scrollTo({ top: transcriptEl.scrollHeight }));
  }

  function handle(sessionId: number, ev: UiEvent) {
    const s = sessions.find((x) => x.id === sessionId);
    if (!s) return;
    switch (ev.kind) {
      case "model":
        s.modelLabel = `${ev.provider} / ${ev.id}`;
        break;
      case "text": {
        const last = s.items[s.items.length - 1];
        if (last && last.type === "assistant") last.text += ev.delta;
        else s.items.push({ type: "assistant", text: ev.delta });
        if (s.id === activeId) scrollDown();
        break;
      }
      case "tool_start":
        s.items.push({ type: "tool", id: ev.id, name: ev.name, args: ev.args, status: "running" });
        if (s.id === activeId) scrollDown();
        break;
      case "tool_end":
        for (let i = s.items.length - 1; i >= 0; i--) {
          const it = s.items[i];
          if (it.type === "tool" && it.id === ev.id && it.status === "running") {
            it.status = ev.ok ? "ok" : "error";
            break;
          }
        }
        break;
      case "stats":
        s.stats = { tokens: ev.tokens, cost: ev.cost, context_pct: ev.context_pct };
        break;
      case "agent_end":
        s.running = false;
        break;
      case "session_end":
        for (const it of s.items) if (it.type === "tool" && it.status === "running") it.status = "error";
        s.running = false;
        s.status = "stopped";
        break;
      case "retry":
        break;
    }
  }

  async function loadProjects() {
    projects = await invoke<Project[]>("list_projects");
  }
  async function loadAgents(projectPath: string) {
    agents = await invoke<Agent[]>("list_agents", { project: projectPath });
    if (!agents.some((a) => a.name === agentName)) agentName = "default";
  }
  async function selectProject(p: Project) {
    active = p;
    try {
      await loadAgents(p.path);
    } catch (e) {
      error = `Could not load agents: ${e}`;
    }
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

  async function startSession() {
    if (!active) return;
    error = null;
    try {
      const id = await invoke<number>("spawn_session", { model, cwd: active.path, agent: agentName });
      sessions.push({
        id,
        projectName: active.name,
        agent: agentName,
        model,
        items: [],
        running: false,
        status: "running",
        modelLabel: null,
        stats: null,
      });
      activeId = id;
      input = "";
    } catch (e) {
      error = `Could not start a session: ${e}`;
    }
  }

  function selectSession(id: number) {
    activeId = id;
    input = "";
    scrollDown();
  }

  async function closeSession(id: number) {
    try {
      await invoke("stop_session", { session: id });
    } catch (e) {
      console.warn("stop_session failed", e);
    }
    sessions = sessions.filter((s) => s.id !== id);
    if (activeId === id) activeId = sessions[sessions.length - 1]?.id ?? null;
  }

  async function send() {
    const s = activeSession;
    if (!s || s.running || s.status === "stopped") return;
    const text = input.trim();
    if (!text) return;
    s.items.push({ type: "user", text });
    input = "";
    s.running = true;
    error = null;
    scrollDown();
    try {
      await invoke("send_prompt", { session: s.id, text });
    } catch (e) {
      s.running = false;
      error = `Send failed: ${e}`;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }

  onMount(() => {
    let alive = true;
    listen<{ session: number; event: UiEvent }>("pi-event", (e) =>
      handle(e.payload.session, e.payload.event)
    ).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });
    loadProjects()
      .then(() => {
        if (alive && projects.length > 0) selectProject(projects[0]);
      })
      .catch((e) => (error = `Could not load projects: ${e}`));
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
      <div class="section-label">New session · {active.name}</div>
      <div class="new-session">
        <select bind:value={agentName} title="Agent (system prompt)">
          {#each agents as a}<option value={a.name}>{a.name}</option>{/each}
        </select>
        <select bind:value={model} title="Model">
          {#each MODELS as m}<option value={m}>{m}</option>{/each}
        </select>
        <button class="primary start" onclick={startSession}>+ start session</button>
      </div>
    {/if}

    {#if sessions.length > 0}
      <div class="section-label">Sessions</div>
      {#each sessions as s}
        <div class="session-row" class:active={s.id === activeId}>
          <button class="session-pick" onclick={() => selectSession(s.id)}>
            <span class="dot {s.status}" class:busy={s.running}></span>
            <span class="s-title">{s.agent} · {s.model}</span>
            <span class="s-proj">{s.projectName}</span>
          </button>
          <button class="session-close" title="Close session" onclick={() => closeSession(s.id)}>✕</button>
        </div>
      {/each}
    {/if}
  </aside>

  <main class="main">
    {#if projects.length === 0}
      <div class="empty">
        <h2>No project yet</h2>
        <p>Add a project directory to start chatting with an agent in it.</p>
        <button class="primary" onclick={() => (adding = true)}>+ add project</button>
      </div>
    {:else if !activeSession}
      <div class="empty">
        <h2>No session</h2>
        <p>Pick a project, choose an agent + model in the sidebar, and start a session.</p>
        {#if active}<button class="primary" onclick={startSession}>+ start session</button>{/if}
      </div>
    {:else}
      <header class="topbar">
        <span class="project-chip">{activeSession.projectName}</span>
        <span class="summary">{activeSession.agent} · {activeSession.model}</span>
        <span class="model">{activeSession.modelLabel ?? "…"}</span>
        {#if activeSession.status === "stopped"}<span class="stopped-tag">stopped</span>{/if}
        <span class="spacer"></span>
        {#if activeSession.stats}
          <span class="gauge"
            >ctx {activeSession.stats.context_pct ?? "–"}% · {activeSession.stats.tokens} tok · ${activeSession.stats.cost.toFixed(
              4
            )}</span
          >
        {/if}
      </header>

      {#if error}
        <div class="banner"><span>{error}</span><button onclick={() => (error = null)}>✕</button></div>
      {/if}

      <section class="transcript" bind:this={transcriptEl}>
        {#each activeSession.items as item}
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
        {#if activeSession.running}<div class="thinking">▍</div>{/if}
      </section>

      <footer class="composer">
        <textarea
          bind:value={input}
          onkeydown={onKeydown}
          placeholder={activeSession.status === "stopped"
            ? "Session stopped — start a new one to continue"
            : "Message the agent…  (Enter to send, Shift+Enter for newline)"}
          rows="2"
          disabled={activeSession.status === "stopped"}
        ></textarea>
        <button onclick={send} disabled={activeSession.running || activeSession.status === "stopped"}
          >Send</button
        >
      </footer>
    {/if}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 240px 1fr;
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
    padding: 12px 8px 4px;
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
  .new-session {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 4px 4px;
  }
  .new-session select {
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
  }
  .new-session .start {
    text-align: center;
    padding: 7px;
  }
  .session-row {
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .session-row.active {
    background: var(--surface);
  }
  .session-pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .session-pick .s-title {
    color: var(--text-bright);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session-pick .s-proj {
    color: var(--text-muted);
    font-size: 11px;
    margin-left: auto;
    flex: none;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .dot.running {
    background: var(--aurora);
  }
  .dot.stopped {
    background: var(--text-muted);
  }
  .dot.busy {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .session-close {
    color: var(--text-muted) !important;
    padding: 4px 8px !important;
    flex: none;
  }
  .session-close:hover {
    color: var(--ember) !important;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .empty {
    flex: 1;
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
  .topbar .summary {
    color: var(--text-subtle);
    font-size: 13px;
  }
  .topbar .model {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 12px;
  }
  .topbar .stopped-tag {
    color: var(--ember);
    font-size: 11px;
    border: 1px solid var(--ember);
    border-radius: 4px;
    padding: 1px 6px;
  }
  .topbar .spacer {
    flex: 1;
  }
  .topbar .gauge {
    color: var(--text-subtle);
    font-family: var(--mono);
    font-size: 12px;
  }
  .banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    background: #2a1719;
    color: var(--text-bright);
    border-bottom: 1px solid var(--ember);
    font-size: 13px;
  }
  .banner button {
    background: transparent;
    border: none;
    color: var(--text-subtle);
    cursor: pointer;
    font: inherit;
  }
  .transcript {
    flex: 1;
    min-height: 0;
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
  .composer textarea:disabled {
    opacity: 0.6;
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
