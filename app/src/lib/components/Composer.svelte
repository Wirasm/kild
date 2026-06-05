<script lang="ts">
  interface Props {
    input: string;
    status: "running" | "stopped";
    running: boolean;
    onSend: () => void;
  }

  let {
    input = $bindable(),
    status,
    running,
    onSend,
  }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      onSend();
    }
  }
</script>

<footer class="composer">
  <textarea
    bind:value={input}
    onkeydown={onKeydown}
    placeholder={status === "stopped"
      ? "Session stopped — start a new one to continue"
      : "Message the agent…  (Enter to send, Shift+Enter for newline)"}
    rows="2"
    disabled={status === "stopped"}
  ></textarea>
  <button onclick={onSend} disabled={running || status === "stopped"}>
    Send
  </button>
</footer>

<style>
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
