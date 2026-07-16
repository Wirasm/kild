<script lang="ts">
  interface Props {
    input: string;
    status: "running" | "stopped";
    running: boolean;
    onSend: () => void;
    placeholder?: string;
  }

  let {
    input = $bindable(),
    status,
    running,
    onSend,
    placeholder = "Message the agent…  (Enter to send, Shift+Enter for newline)",
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
    placeholder={status === "stopped" ? "Room ended — read-only history" : placeholder}
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
    border-top: 1px solid var(--border-translucent);
    background: transparent;
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
    transition: border-color 0.2s cubic-bezier(0.4, 0, 0.2, 1), box-shadow 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .composer textarea:focus {
    border-color: var(--ice);
    outline: none;
    box-shadow: var(--glow-ice);
  }
  .composer textarea:disabled {
    opacity: 0.6;
  }
  .composer button {
    background: transparent;
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0 18px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .composer button:hover:not(:disabled) {
    border-color: var(--ice);
    color: var(--ice);
    background: rgba(124, 180, 200, 0.05);
    box-shadow: var(--glow-ice);
  }
  .composer button:active:not(:disabled) {
    background: rgba(124, 180, 200, 0.1);
  }
  .composer button:disabled {
    border-color: var(--border-subtle);
    color: var(--text-muted);
    opacity: 0.5;
    cursor: default;
  }
</style>
