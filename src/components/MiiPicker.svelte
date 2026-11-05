<script>
  import MiiHead from './MiiHead.svelte';

  /** items: [{ key, mii, label? }]; `selected`: a key. */
  let { items, selected = null, onpick, size = 64, label } = $props();
</script>

<div role="radiogroup" aria-label={label} style="display: grid; grid-template-columns: repeat(auto-fill, minmax({size + 20}px, 1fr)); gap: 10px; padding: 4px;">
  {#each items as item (item.key)}
    {@const on = item.key === selected}
    <button type="button" role="radio" aria-checked={on} aria-label={item.label ?? item.mii?.name} title={item.label ?? item.mii?.name} onclick={() => onpick(item.key)} style="position: relative; display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 6px 4px {item.label ? 6 : 8}px; border: 0; border-radius: 22px; cursor: pointer; background: {on ? 'var(--tint)' : 'var(--surface-soft)'}; box-shadow: {on ? 'inset 0 0 0 4px var(--accent), 0 4px 0 rgba(var(--shadow),0.10)' : '0 3px 0 rgba(var(--shadow),0.06)'};">
      <span style="width: {size}px; height: {size}px; border-radius: 999px; background: #ffffff; box-shadow: inset 0 -4px 0 {item.mii?.color ?? '#e3e0e2'}33; display: flex; align-items: center; justify-content: center; overflow: hidden;">
        <MiiHead mii={item.mii} size={size} />
      </span>
      {#if item.label}
        <span style="max-width: 100%; font-size: 12px; font-weight: 800; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{item.label}</span>
      {/if}
      {#if on}
        <span aria-hidden="true" style="position: absolute; top: 2px; right: 2px; width: 22px; height: 22px; border-radius: 999px; background: var(--accent); color: #ffffff; display: flex; align-items: center; justify-content: center; box-shadow: 0 2px 0 var(--accent-deep);">
          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 6.2l2.3 2.3 4.7-5" stroke="currentColor" stroke-width="2.2" fill="none" stroke-linecap="round" stroke-linejoin="round"></path></svg>
        </span>
      {/if}
    </button>
  {/each}
</div>
