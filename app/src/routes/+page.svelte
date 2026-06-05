<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import Topbar from "$lib/components/Topbar.svelte";
  import Ledger from "$lib/components/Ledger.svelte";
  import Composer from "$lib/components/Composer.svelte";

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
  let unlisten: UnlistenFn | null = null;

  let activeSession = $derived(sessions.find((s) => s.id === activeId) ?? null);

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
        break;
      }
      case "tool_start":
        s.items.push({ type: "tool", id: ev.id, name: ev.name, args: ev.args, status: "running" });
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
    try {
      await invoke("send_prompt", { session: s.id, text });
    } catch (e) {
      s.running = false;
      error = `Send failed: ${e}`;
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
  <Sidebar
    projects={projects}
    bind:active={active}
    bind:adding={adding}
    bind:newName={newName}
    bind:newPath={newPath}
    bind:addError={addError}
    agents={agents}
    bind:agentName={agentName}
    bind:model={model}
    models={MODELS}
    bind:sessions={sessions}
    bind:activeId={activeId}
    onSelectProject={selectProject}
    onAddProject={addProject}
    onStartSession={startSession}
    onSelectSession={selectSession}
    onCloseSession={closeSession}
  />

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
      <Topbar activeSession={activeSession} />

      {#if error}
        <div class="banner">
          <span>{error}</span>
          <button onclick={() => (error = null)}>✕</button>
        </div>
      {/if}

      <Ledger items={activeSession.items} running={activeSession.running} />

      <Composer bind:input={input} status={activeSession.status} running={activeSession.running} onSend={send} />
    {/if}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100vh;
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
</style>
