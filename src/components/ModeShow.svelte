<script>
  /** The home screen's frame: the game's modes one after the other (lib/modes.svelte.js). */
  import { shownModes } from '../lib/modes.svelte.js';
  import { game } from '../lib/game.svelte.js';

  /** The colors of the main menu's families. */
  const GROUPS = { party: '#dc2814', pair: '#c4681a', house: '#2b8d45', version: 'var(--accent)' };
  const STEP = 4200;

  const list = $derived(shownModes());
  let shown = $state(0);
  const still = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  $effect(() => {
    const count = list.length;
    shown = 0;
    // It waits while Dolphin plays, and stays put for those who asked for less motion.
    if (count < 2 || still || game.phase === 'running') return;
    const timer = setInterval(() => (shown = (shown + 1) % count), STEP);
    return () => clearInterval(timer);
  });

  const current = $derived(list[shown]);
</script>

<div class="motion-float" style="position: absolute; left: 90px; top: 66px; width: 392px; height: 306px; border-radius: 40px; background: #ffffff; transform: rotate(-6deg); box-shadow: 0 8px 0 rgba(var(--shadow),0.10), 0 24px 50px rgba(var(--shadow),0.16);">
  <div style="position: absolute; inset: 15px; border-radius: 27px; overflow: hidden; background: var(--surface-soft);">
    {#each list as mode, i (mode.id)}
      <img src={mode.image} alt="" style="position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: {i === shown ? 1 : 0}; transition: opacity 0.7s ease;">
    {/each}
  </div>
  {#if current}
    {#key current.id}
      <span class="fade-in" style="position: absolute; left: 34px; top: -16px; display: inline-flex; align-items: center; gap: 8px; max-width: 320px; padding: 8px 18px 8px 10px; border-radius: 999px; background: #ffffff; box-shadow: 0 4px 0 rgba(var(--shadow),0.12), 0 10px 20px rgba(var(--shadow),0.12);">
        <span style="flex-shrink: 0; width: 16px; height: 16px; border-radius: 999px; background: {GROUPS[current.group] ?? GROUPS.version}; box-shadow: inset 0 2px 0 rgba(255,255,255,0.45);"></span>
        <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 17px; font-weight: 800; color: {GROUPS[current.group] ?? GROUPS.version};">{current.name}</span>
      </span>
    {/key}
  {/if}
</div>
