<script>
  import { t } from '../lib/i18n.svelte.js';

  // notes: [{ level: "error" | "warning" | "ok", message }]. One at a time, the most serious first.
  let { notes } = $props();

  const rank = { error: 0, warning: 1, ok: 2 };
  const sorted = $derived([...notes].sort((a, b) => rank[a.level] - rank[b.level]));
  let index = $state(0);
  const shown = $derived(sorted[index % sorted.length]);

  $effect(() => {
    sorted;
    index = 0;
  });
</script>

{#if sorted.length}
  <div role={shown.level === 'error' ? 'alert' : 'status'} class="note {shown.level}">
    <span style="flex-grow: 1; min-width: 0; overflow-wrap: anywhere;">{shown.message}</span>
    {#if sorted.length > 1}
      <button type="button" aria-label={t('Notification suivante')} onclick={() => (index += 1)}>{index % sorted.length + 1}/{sorted.length} ›</button>
    {/if}
  </div>
{/if}

<style>
  .note {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
    padding: 8px 14px;
    border-radius: 14px;
    font-size: 13px;
    font-weight: 700;
  }
  .error {
    background: var(--danger-bg);
    color: var(--danger);
  }
  .warning {
    background: var(--warn-bg);
    color: var(--warn);
  }
  .ok {
    background: var(--ok-bg);
    color: var(--ok);
  }
  button {
    flex-shrink: 0;
    border: none;
    border-radius: 10px;
    padding: 3px 9px;
    background: rgba(0, 0, 0, 0.08);
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
</style>
