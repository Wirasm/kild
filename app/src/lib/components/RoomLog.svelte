<script lang="ts">
  import { marked } from "marked";
  import type { Message } from "../types";

  let { log }: { log: Message[] } = $props();

  const PALETTE = ["#7cb4c8", "#7cc8a0", "#c89cd8", "#d8c87c", "#d89c7c", "#9cb0d8"];
  function colorFor(name: string): string {
    if (name === "human") return "var(--ice)";
    let h = 0;
    for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
    return PALETTE[h % PALETTE.length];
  }
  function render(text: string): string {
    return marked.parse(text, { async: false }) as string;
  }
</script>

<div class="log">
  {#each log as m (m.id)}
    {#if m.system}
      <div class="sys">{m.text}</div>
    {:else}
      <div class="msg">
        <div class="head">
          <span class="from" style="color:{colorFor(m.from)}">
            {m.from === "human" ? "You" : `@${m.from}`}
          </span>
          {#if m.to.length}<span class="to">→ {m.to.map((t) => `@${t}`).join(" ")}</span>{/if}
          {#if m.implicit}<span class="implicit">reply</span>{/if}
        </div>
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        <div class="text">{@html render(m.text)}</div>
      </div>
    {/if}
  {/each}
  {#if log.length === 0}
    <div class="placeholder">No messages yet — post to get the room going.</div>
  {/if}
</div>

<style>
  .log {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    overflow-y: auto;
    height: 100%;
  }
  .msg {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
  }
  .from {
    font-weight: 600;
  }
  .to {
    color: var(--text-muted);
    font-family: var(--mono);
    font-size: 11px;
  }
  .implicit {
    color: var(--text-muted);
    font-size: 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    padding: 0 4px;
  }
  .text {
    color: var(--text);
    font-size: 13px;
    line-height: 1.45;
  }
  .text :global(p) {
    margin: 0 0 6px 0;
  }
  .text :global(p:last-child) {
    margin-bottom: 0;
  }
  .text :global(code) {
    font-family: var(--mono);
    font-size: 12px;
    background: rgba(255, 255, 255, 0.05);
    padding: 0 3px;
    border-radius: 3px;
  }
  .text :global(strong) {
    color: var(--text-bright);
    font-weight: 600;
  }
  .text :global(ul),
  .text :global(ol) {
    margin: 4px 0 8px;
    padding-left: 18px;
  }
  .text :global(li) {
    margin: 3px 0;
  }
  .text :global(li > ul),
  .text :global(li > ol) {
    margin: 3px 0 3px;
  }
  .text :global(h1),
  .text :global(h2),
  .text :global(h3) {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-bright);
    margin: 10px 0 4px;
  }
  .text :global(table) {
    border-collapse: collapse;
    margin: 6px 0;
    font-size: 12px;
  }
  .text :global(th),
  .text :global(td) {
    border: 1px solid var(--border-subtle);
    padding: 3px 8px;
    text-align: left;
    vertical-align: top;
  }
  .text :global(th) {
    color: var(--text-bright);
    background: rgba(255, 255, 255, 0.03);
  }
  .text :global(blockquote) {
    margin: 4px 0;
    padding-left: 10px;
    border-left: 2px solid var(--border-subtle);
    color: var(--text-subtle);
  }
  .text :global(a) {
    color: var(--ice);
  }
  .sys {
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
    font-style: italic;
  }
  .placeholder {
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
