<script>
  // A package's "mii" setting (src-tauri/src/mods.rs): a Mii of the game or of the player's library,
  // picked like the profile's (Welcome); its value is the Mii's `key` (mii::setting_key), 0 the game's choice.
  import MiiHead from './MiiHead.svelte';
  import MiiPicker from './MiiPicker.svelte';
  import { settings } from '../lib/settings.svelte.js';
  import { miis, discMiis, loadDiscMiis } from '../lib/miis.svelte.js';
  import { t, local } from '../lib/i18n.svelte.js';

  let { setting: s, busy = false, onchange } = $props();

  let open = $state(false);
  let tab = $state('disc');

  const all = $derived([...miis.list, ...discMiis.list]);
  const chosen = $derived(s.value ? all.find((m) => m.key === s.value) : null);
  // The game's Miis are read from the disc the first time they are needed.
  $effect(() => {
    if (s.value && s.value <= 100 && !discMiis.list.length && !discMiis.error) loadDiscMiis();
  });

  const show = () => {
    tab = miis.list.some((m) => m.key === s.value) ? 'mine' : 'disc';
    if (!discMiis.list.length) loadDiscMiis();
    open = true;
  };

  const pick = (key) => {
    open = false;
    if (key !== s.value) onchange(s, key);
  };

  const items = (list) => list.map((m) => ({ key: m.key, mii: m, label: m.name }));
</script>

<button type="button" class="current" disabled={busy} onclick={show} aria-label={local(s.label)}>
  <span class="face" style="box-shadow: inset 0 -3px 0 {chosen?.color ?? '#e3e0e2'}55;">
    {#if chosen}
      <MiiHead mii={chosen} size={34} />
    {:else}
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--muted)" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M9 9.5a3 3 0 1 1 4.2 2.75c-.75.35-1.2 1-1.2 1.85V15"></path><circle cx="12" cy="19" r="0.6" fill="var(--muted)"></circle></svg>
    {/if}
  </span>
  <span class="name">{s.value === 0 ? t('Au choix du jeu') : chosen ? chosen.name : discMiis.loading ? '…' : t('Mii introuvable')}</span>
</button>

{#if open}
  <div class="scrim">
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <section role="dialog" aria-modal="true" aria-labelledby="mii-setting-title" class="fade-in dialog">
      <header style="display: flex; align-items: center; justify-content: space-between; gap: 14px;">
        <div style="min-width: 0;">
          <h2 id="mii-setting-title" style="margin: 0; font-size: 26px; font-weight: 800; color: var(--accent);">{t('Choisis un Mii')}</h2>
          <p style="margin: 2px 0 0; font-size: 14px; font-weight: 700; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{local(s.label)}</p>
        </div>
        <nav aria-label={t('Sources')} style="flex-shrink: 0; display: flex; gap: 6px; padding: 5px; border-radius: 999px; background: var(--surface);">
          {#each [['disc', t('Mii de Wii Party')], ['mine', miis.list.length ? t('Mes Mii ({n})', { n: miis.list.length }) : t('Mes Mii')]] as [key, label]}
            <button type="button" aria-current={tab === key ? 'page' : undefined} onclick={() => (tab = key)} class="tab" class:on={tab === key}>{label}</button>
          {/each}
        </nav>
      </header>

      <div style="flex-grow: 1; min-height: 0; overflow-y: auto; border-radius: 24px; background: var(--surface-soft); padding: 10px;">
        {#if tab === 'mine'}
          {#if miis.list.length}
            <MiiPicker items={items(miis.list)} selected={s.value} onpick={pick} label={t('Mes Mii')} />
          {:else}
            <p class="empty">{t('Pas encore de Mii à toi : ajoute-les dans Mes Mii.')}</p>
          {/if}
        {:else if !settings.iso}
          <p class="empty">{t('Les Mii de Wii Party sont sur ton disque')}</p>
        {:else if discMiis.loading && !discMiis.list.length}
          <p role="status" class="empty">{t('Le launcher lit les Mii de ton disque…')}</p>
        {:else if discMiis.error}
          <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: var(--danger-bg); font-size: 13px; font-weight: 700; color: var(--danger);">{discMiis.error}</p>
        {:else}
          <MiiPicker items={items(discMiis.list)} selected={s.value} onpick={pick} size={58} label={t('Mii de Wii Party')} />
        {/if}
      </div>

      <footer style="display: flex; align-items: center; gap: 12px;">
        <button type="button" class="act" onclick={() => (open = false)}>{t('Annuler')}</button>
        <span style="flex-grow: 1;"></span>
        <button type="button" class="act" class:main={s.value !== 0} onclick={() => pick(0)}>{t('Au choix du jeu')}</button>
      </footer>
    </section>
  </div>
{/if}

<style>
  .current {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 220px;
    height: 42px;
    padding: 0 16px 0 4px;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    background: var(--surface);
  }
  .current:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .face {
    flex-shrink: 0;
    width: 34px;
    height: 34px;
    border-radius: 999px;
    background: var(--card);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .name {
    min-width: 0;
    font-size: 14px;
    font-weight: 800;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgba(var(--scrim), 0.52);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dialog {
    width: 860px;
    max-width: calc(100% - 32px);
    height: 600px;
    max-height: calc(100% - 32px);
    box-sizing: border-box;
    background: var(--card);
    border-radius: 40px;
    padding: 26px 30px 22px;
    box-shadow: 0 12px 40px rgba(var(--shadow), 0.24);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .tab {
    height: 34px;
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 800;
    background: transparent;
    color: var(--muted);
  }
  .tab.on {
    background: linear-gradient(180deg, var(--grad-top) 0%, var(--grad-bottom) 62%);
    box-shadow: inset 0 3px 0 rgba(255, 255, 255, 0.45);
    color: #ffffff;
    -webkit-text-stroke: 3px var(--accent-deep);
    paint-order: stroke fill;
  }
  .empty {
    height: 100%;
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    text-align: center;
    font-size: 16px;
    font-weight: 800;
    color: var(--muted);
  }
  .act {
    height: 44px;
    padding: 0 22px;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    background: var(--surface);
    font-size: 15px;
    font-weight: 800;
    color: var(--text);
  }
  .act.main {
    background: linear-gradient(180deg, var(--grad-top) 0%, var(--grad-bottom) 62%);
    box-shadow: inset 0 3px 0 rgba(255, 255, 255, 0.45), 0 4px 0 var(--accent-deep);
    color: #ffffff;
    -webkit-text-stroke: 4px var(--accent-deep);
    paint-order: stroke fill;
  }
</style>
