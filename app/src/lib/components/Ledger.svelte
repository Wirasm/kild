<script lang="ts">
  import { marked } from "marked";
  import ToolCard from "./ToolCard.svelte";
  import type { Item } from "../types";

  interface Props {
    items: Item[];
    running: boolean;
    agentName?: string;
  }

  let { items, running, agentName = "Agent" }: Props = $props();
  let transcriptEl: HTMLElement | undefined = $state();

  // Tool calls are hidden by default — they bloat the pane and bury the agent's
  // messages. The toggle reveals them on demand.
  let showTools = $state(false);
  let toolCount = $derived(items.filter((i) => i.type === "tool").length);
  let shown = $derived(showTools ? items : items.filter((i) => i.type !== "tool"));

  function scrollDown() {
    queueMicrotask(() => transcriptEl?.scrollTo({ top: transcriptEl.scrollHeight }));
  }

  // Svelte 5 effect to automatically scroll to bottom on updates
  $effect(() => {
    if (items.length || running) {
      scrollDown();
    }
  });

  // Synchronous compile markdown helper
  function renderMarkdown(text: string): string {
    try {
      return marked.parse(text, { async: false }) as string;
    } catch {
      return text;
    }
  }
</script>

<section class="transcript" bind:this={transcriptEl}>
  {#if toolCount > 0}
    <div class="ledger-controls">
      <button class="tools-toggle" class:active={showTools} onclick={() => (showTools = !showTools)}>
        🔧 {toolCount} tool call{toolCount === 1 ? "" : "s"} · {showTools ? "hide" : "show"}
      </button>
    </div>
  {/if}
  {#each shown as item}
    <div class="message-wrapper {item.type}">
      <div class="gutter">
        {#if item.type === "user"}
          <div class="avatar user">TR</div>
        {:else if item.type === "assistant"}
          <div class="avatar assistant">AG</div>
        {:else}
          <div class="avatar tool">TL</div>
        {/if}
      </div>

      <div class="message-content">
        <div class="message-meta">
          {#if item.type === "user"}
            <span class="actor-name">Tōryō</span>
            <span class="actor-tag user">[User]</span>
          {:else if item.type === "assistant"}
            <span class="actor-name">{agentName}</span>
            <span class="actor-tag assistant">[Agent]</span>
          {:else}
            <span class="actor-name">{item.name}</span>
            <span class="actor-tag tool">[Tool]</span>
          {/if}
        </div>

        <div class="message-body">
          {#if item.type === "user"}
            <div class="plain-text">{item.text}</div>
          {:else if item.type === "assistant"}
            <div class="markdown-body">
              {@html renderMarkdown(item.text)}
            </div>
          {:else}
            <ToolCard name={item.name} args={item.args} status={item.status} />
          {/if}
        </div>
      </div>
    </div>
  {/each}

  {#if running}
    <div class="message-wrapper assistant running">
      <div class="gutter">
        <div class="avatar assistant">AG</div>
      </div>
      <div class="message-content">
        <div class="message-meta">
          <span class="actor-name">{agentName}</span>
          <span class="actor-tag assistant">[thinking]</span>
        </div>
        <div class="message-body">
          <div class="thinking-container">
            <div class="spinner"></div>
            <span class="thinking-text">thinking…</span>
          </div>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .transcript {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 24px; /* Spacious gaps between events */
  }

  .ledger-controls {
    position: sticky;
    top: -20px; /* counter the transcript's top padding so it pins to the very top */
    z-index: 2;
    display: flex;
    justify-content: flex-end;
    margin: -12px -4px -8px; /* tuck into the gap, don't add a full 24px row */
    padding: 4px 0;
    background: var(--obsidian);
  }
  .tools-toggle {
    background: transparent;
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .tools-toggle:hover {
    color: var(--copper);
    border-color: var(--copper);
  }
  .tools-toggle.active {
    color: var(--copper);
    border-color: var(--copper);
    background: rgba(196, 154, 92, 0.08);
  }

  .message-wrapper {
    display: grid;
    grid-template-columns: 36px 1fr;
    gap: 16px;
    align-items: start;
    width: 100%;
  }

  .gutter {
    display: flex;
    justify-content: center;
  }

  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    font-family: var(--mono);
    user-select: none;
    border: 1px solid transparent;
  }

  .avatar.user {
    background: rgba(124, 180, 200, 0.1);
    color: var(--ice);
    border-color: rgba(124, 180, 200, 0.2);
  }

  .avatar.assistant {
    background: rgba(107, 143, 94, 0.1);
    color: var(--aurora);
    border-color: rgba(107, 143, 94, 0.2);
  }

  .avatar.tool {
    background: rgba(196, 154, 92, 0.1);
    color: var(--copper);
    border-color: rgba(196, 154, 92, 0.2);
  }

  .message-content {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .message-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .actor-name {
    font-weight: 600;
    color: var(--text-bright);
  }

  .actor-tag {
    font-size: 10px;
    font-family: var(--mono);
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--surface);
    border: 1px solid var(--border-subtle);
  }
  .actor-tag.user {
    color: var(--ice-dim);
  }
  .actor-tag.assistant {
    color: var(--aurora);
  }
  .actor-tag.tool {
    color: var(--copper);
  }

  .message-body {
    font-size: 14px;
    line-height: 1.55;
    color: var(--text);
  }

  .plain-text {
    white-space: pre-wrap;
    background: var(--surface-translucent);
    border: 1px solid var(--border-subtle);
    padding: 10px 14px;
    border-radius: 8px;
    max-width: 80ch;
  }

  /* Markdown & Code styling */
  .markdown-body {
    max-width: 80ch;
  }
  .markdown-body :global(p) {
    margin: 0 0 10px 0;
  }
  .markdown-body :global(p:last-child) {
    margin: 0;
  }
  .markdown-body :global(pre) {
    background: var(--void);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 12px;
    margin: 12px 0;
    overflow-x: auto;
    box-shadow: var(--shadow-subtle);
  }
  .markdown-body :global(code) {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ice);
    background: rgba(124, 180, 200, 0.05);
    padding: 2px 4px;
    border-radius: 4px;
  }
  .markdown-body :global(pre code) {
    background: transparent;
    padding: 0;
    border-radius: 0;
    color: var(--text-bright);
  }
  .markdown-body :global(ul), .markdown-body :global(ol) {
    margin: 0 0 10px 0;
    padding-left: 20px;
  }
  .markdown-body :global(li) {
    margin-bottom: 4px;
  }
  .markdown-body :global(h1), .markdown-body :global(h2), .markdown-body :global(h3) {
    color: var(--text-bright);
    font-weight: 600;
    margin: 16px 0 8px 0;
  }
  .markdown-body :global(h3) {
    font-size: 14px;
  }

  /* Thinking Spinner */
  .thinking-container {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--surface);
    border: 1px solid var(--border-subtle);
    border-radius: 20px;
    color: var(--text-subtle);
    font-size: 11px;
    box-shadow: var(--shadow-subtle);
    width: fit-content;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(124, 180, 200, 0.2);
    border-top: 2px solid var(--ice);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    0% {
      transform: rotate(0deg);
    }
    100% {
      transform: rotate(360deg);
    }
  }
</style>
