<script>
  // One setting of a package (src-tauri/src/mods.rs, SettingView): a switch, a list, a number or a Mii.
  // `onchange(setting, value)` saves it: the page keeps the package's values (ModDetail, Debug).
  import Select from './Select.svelte';
  import MiiSetting from './MiiSetting.svelte';
  import { t, local } from '../lib/i18n.svelte.js';

  let { setting: s, last = false, busy = false, onchange } = $props();

  /** A preset's "custom" choice (no `sets`) is only shown while it is the one. */
  const offered = (s) => {
    const preset = s.choices.some((c) => c.sets);
    return s.choices.filter((c) => !preset || c.sets || c.value === s.value).map((c) => [c.value, local(c.label)]);
  };

  const step = (direction) => {
    const next = +(s.value + direction * (s.step || 1)).toFixed(10);
    onchange(s, Math.min(s.max ?? Infinity, Math.max(s.min ?? -Infinity, next)));
  };

  /** A number typed in its field: kept within its bounds, on its steps; anything else puts the value back. */
  const typed = (input) => {
    const raw = Number(input.value.replace(/[\s  ]/g, '').replace(',', '.'));
    if (input.value.trim() === '' || !Number.isFinite(raw)) {
      input.value = s.value;
      return;
    }
    const min = s.min ?? 0;
    const unit = s.step || 1;
    const value = Math.min(s.max ?? Infinity, Math.max(min, +(min + Math.round((raw - min) / unit) * unit).toFixed(10)));
    input.value = value;
    if (value !== s.value) onchange(s, value);
  };

  /** The field's width: the widest value it can hold. */
  const digits = $derived(Math.max(String(s.min ?? 0).length, String(s.max ?? s.value).length, 2));
</script>

<div style="display: flex; align-items: center; gap: 16px; padding: 11px 0; box-shadow: {last ? 'none' : 'inset 0 -2px 0 var(--surface)'};">
  <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column;">
    <span style="font-size: 15px; font-weight: 800;">{local(s.label)}</span>
    {#if s.description}
      <span style="font-size: 12px; font-weight: 700; color: var(--muted); overflow-wrap: anywhere;">{local(s.description)}</span>
    {/if}
  </span>
  {#if s.type === 'toggle'}
    <button type="button" role="switch" aria-checked={!!s.value} aria-label={local(s.label)} disabled={busy} onclick={() => onchange(s, s.value ? 0 : 1)} style="position: relative; flex-shrink: 0; width: 52px; height: 30px; border: 0; border-radius: 999px; cursor: pointer; background: {s.value ? '#25a720' : 'var(--off)'};"><span style="position: absolute; top: 4px; left: {s.value ? 26 : 4}px; width: 22px; height: 22px; border-radius: 999px; background: #ffffff; box-shadow: 0 2px 0 rgba(0,0,0,0.15);"></span></button>
  {:else if s.type === 'choice'}
    <Select label={local(s.label)} value={s.value} options={offered(s)} onchange={(v) => onchange(s, v)} />
  {:else if s.type === 'mii'}
    <MiiSetting setting={s} {busy} {onchange} />
  {:else}
    <span role="group" aria-label={local(s.label)} style="flex-shrink: 0; display: flex; align-items: center; gap: 4px; height: 38px; padding: 0 4px; border-radius: 12px; background: var(--surface);">
      <button type="button" class="step" aria-label={t('Moins')} disabled={busy || s.value <= (s.min ?? -Infinity)} onclick={() => step(-1)}>−</button>
      <input class="number" type="text" inputmode="numeric" aria-label={local(s.label)} disabled={busy} value={s.value} onchange={(e) => typed(e.currentTarget)} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} style="width: calc({digits}ch + 16px);" />
      <button type="button" class="step" aria-label={t('Plus')} disabled={busy || s.value >= (s.max ?? Infinity)} onclick={() => step(1)}>+</button>
    </span>
  {/if}
</div>

<style>
  .step {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 9px;
    cursor: pointer;
    background: var(--card);
    font-size: 18px;
    font-weight: 800;
    color: var(--text);
  }
  .step:disabled {
    cursor: default;
    opacity: 0.4;
  }
  .number {
    box-sizing: border-box;
    min-width: 48px;
    height: 30px;
    padding: 0 8px;
    border: 2px solid transparent;
    border-radius: 9px;
    background: transparent;
    text-align: center;
    font: inherit;
    font-size: 14px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .number:hover:not(:disabled) {
    background: var(--card);
  }
  .number:focus {
    outline: none;
    border-color: var(--accent);
    background: var(--card);
  }
</style>
