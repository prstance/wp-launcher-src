<script>
  // Drop-down list drawn by the launcher: WebKitGTK's own <select> opens a black
  // system menu that no CSS reaches. Options are buttons, so the Wii Remote's
  // D-pad and A (lib/remote-nav.svelte.js) pick them like any other control.
  let { label, value, options, onchange } = $props();

  const ROW = 36;

  let open = $state(false);
  let upwards = $state(false);
  let root;

  const current = $derived(options.find(([v]) => v === value)?.[1] ?? '');

  function toggle() {
    if (!open) {
      // Opens upwards when the window ends before the list does.
      const r = root.getBoundingClientRect();
      upwards = r.bottom + options.length * ROW + 24 > innerHeight && r.top > innerHeight - r.bottom;
    }
    open = !open;
  }

  function pick(v) {
    open = false;
    if (v !== value) onchange(v);
  }

  const outside = (e) => {
    if (open && !root.contains(e.target)) open = false;
  };
  const escape = (e) => {
    if (open && e.key === 'Escape') open = false;
  };
</script>

<svelte:window onclick={outside} onkeydown={escape} />

<div bind:this={root} style="position: relative; flex-shrink: 0;">
  <button type="button" class="trigger" aria-label={label} aria-haspopup="listbox" aria-expanded={open} onclick={toggle}>
    <span>{current}</span>
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" style="rotate: {open ? 180 : 0}deg;"><path d="M2.5 4.5 6 8l3.5-3.5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" fill="none"></path></svg>
  </button>

  {#if open}
    <div role="listbox" aria-label={label} class="list" style="{upwards ? 'bottom' : 'top'}: calc(100% + 6px);">
      {#each options as [v, text]}
        <button type="button" role="option" aria-selected={v === value} class="option" onclick={() => pick(v)}>
          <span>{text}</span>
          {#if v === value}
            <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 7.5 6 10.5l5-6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" fill="none"></path></svg>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .trigger {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 14px 0 16px;
    border: 0;
    border-radius: 12px;
    cursor: pointer;
    background: var(--surface);
    font-size: 13px;
    font-weight: 800;
    color: var(--text);
  }

  .trigger[aria-expanded="true"] {
    background: var(--tint);
    color: var(--label);
  }

  .trigger svg {
    transition: rotate 0.18s ease;
  }

  .list {
    position: absolute;
    right: 0;
    z-index: 10;
    min-width: 100%;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: 18px;
    background: #ffffff;
    box-shadow:
      0 0 0 2px var(--line),
      0 10px 24px rgba(var(--shadow), 0.16);
    animation: pop 0.18s cubic-bezier(0.2, 1.3, 0.4, 1) backwards;
  }

  .option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    height: 34px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    cursor: pointer;
    white-space: nowrap;
    text-align: left;
    background: transparent;
    font-size: 13px;
    font-weight: 800;
    color: var(--text);
  }

  /* Rows of a list light up rather than grow. */
  .option:hover,
  .option:global([data-wm-focus]) {
    scale: 1;
    background: var(--surface);
  }

  .option[aria-selected="true"] {
    background: var(--tint);
    color: var(--label);
  }

  .option:global([data-wm-focus]) {
    outline-offset: -2px;
  }
</style>
