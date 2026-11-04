<script>
  import { invoke } from '@tauri-apps/api/core';
  import { inTauri } from '../lib/tauri.js';
  import PackageDrop from './PackageDrop.svelte';
  import { mods, enabledMods, refreshMods, importMods, pickMods, setModEnabled, removeMod } from '../lib/mods.svelte.js';
  import { t, lang, local } from '../lib/i18n.svelte.js';

  let busy = $state(false);
  let added = $state([]);
  let refused = $state([]);

  const size = (bytes) =>
    bytes < 1024 * 1024
      ? t('{n} Ko', { n: Math.ceil(bytes / 1024) })
      : t('{n} Mo', { n: (bytes / 1024 / 1024).toLocaleString(lang(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }) });

  const attempt = async (work) => {
    if (!inTauri) {
      refused = [t('Disponible dans l’application de bureau.')];
      return;
    }
    busy = true;
    added = [];
    refused = [];
    try {
      const done = await work();
      added = done?.added ?? [];
      refused = done?.refused ?? [];
    } catch (e) {
      refused = [String(e)];
    }
    busy = false;
  };

  const browse = () => attempt(pickMods);
  const openFolder = () => attempt(() => invoke('folder_open', { which: 'mods' }));
  const toggle = (mod) => attempt(() => setModEnabled(mod.id, !mod.enabled));
  const remove = (mod) => attempt(() => removeMod(mod.id));

  $effect(() => {
    refreshMods();
  });
</script>

<div style="height: 100%; display: flex; flex-direction: column; gap: 12px;">
  <PackageDrop title={t('Dépose tes mods ici')} hint={t('Fichiers .wpmod')} {busy} ondrop={(paths) => attempt(() => importMods(paths))} onbrowse={browse} />

  {#if added.length}
    <p role="status" style="margin: 0; padding: 8px 14px; border-radius: 14px; background: #d9f0dd; font-size: 13px; font-weight: 700; color: #17532a; overflow-wrap: anywhere;">{t('Installé : {names}', { names: added.join(', ') })}</p>
  {/if}
  {#each refused as message}
    <p role="alert" style="margin: 0; padding: 8px 14px; border-radius: 14px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424; overflow-wrap: anywhere;">{message}</p>
  {/each}

  {#each mods.problems as problem}
    <p role={problem.level === 'error' ? 'alert' : 'status'} style="margin: 0; padding: 8px 14px; border-radius: 14px; background: {problem.level === 'error' ? '#fde0e0' : '#f8e6c6'}; font-size: 13px; font-weight: 700; color: {problem.level === 'error' ? '#7a1424' : '#6b4100'}; overflow-wrap: anywhere;">{problem.level === 'error' ? t('Le jeu ne se lancera pas : {message}', { message: problem.message }) : problem.message}</p>
  {:else}
    {#if enabledMods().length + (mods.version ? 1 : 0) > 1}
      <p role="status" style="margin: 0; padding: 8px 14px; border-radius: 14px; background: #d9f0dd; font-size: 13px; font-weight: 700; color: #17532a;">{t('Aucun conflit détecté entre les mods activés.')}</p>
    {/if}
  {/each}

  <ul aria-label={t('Mods installés')} style="flex: 1 1 0; min-height: 0; overflow-y: auto; margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column;">
    {#each mods.list as mod (mod.id)}
      <li style="display: flex; align-items: center; gap: 12px; padding: 10px 4px; box-shadow: inset 0 -2px 0 var(--surface);">
        <button type="button" role="switch" aria-checked={mod.enabled} aria-label={t('Activer {name}', { name: local(mod.name) })} onclick={() => toggle(mod)} disabled={busy} style="position: relative; flex-shrink: 0; width: 52px; height: 30px; border: 0; border-radius: 999px; cursor: pointer; background: {mod.enabled ? '#25a720' : 'var(--off)'};"><span style="position: absolute; top: 4px; left: {mod.enabled ? 26 : 4}px; width: 22px; height: 22px; border-radius: 999px; background: #ffffff; box-shadow: 0 2px 0 rgba(0,0,0,0.15);"></span></button>
        <a href="#/ModDetail/{encodeURIComponent(mod.id)}" class="info" title={local(mod.description)}>
          {#if mod.icon}
            <img src={mod.icon} alt="" style="flex-shrink: 0; width: 40px; height: 40px; border-radius: 12px; object-fit: cover;">
          {/if}
          <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column;">
            <span style="font-size: 15px; font-weight: 800; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{local(mod.name)}</span>
            <span style="font-size: 12px; font-weight: 700; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{[t('Version {v}', { v: mod.version }), ...(mod.authors.length ? [mod.authors.join(', ')] : []), mod.games.join(', '), size(mod.size)].join(' · ')}</span>
          </span>
        </a>
        {#if mod.settings > 0}
          <a href="#/ModDetail/{encodeURIComponent(mod.id)}" aria-label={t('Réglages de {name}', { name: local(mod.name) })} title={t('Réglages')} style="flex-shrink: 0; width: 34px; height: 34px; border-radius: 999px; background: var(--surface); display: flex; align-items: center; justify-content: center; color: var(--text);">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3.2"></circle><path d="M12 2.8v2.6M12 18.6v2.6M21.2 12h-2.6M5.4 12H2.8M18.5 5.5l-1.8 1.8M7.3 16.7l-1.8 1.8M18.5 18.5l-1.8-1.8M7.3 7.3 5.5 5.5"></path></svg>
          </a>
        {/if}
        <button type="button" onclick={() => remove(mod)} disabled={busy} style="flex-shrink: 0; height: 34px; padding: 0 14px; border: 0; border-radius: 999px; cursor: pointer; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--text);">{t('Retirer')}</button>
      </li>
    {:else}
      <li style="margin: auto; font-size: 15px; font-weight: 700; color: var(--muted);">{t('Aucun mod pour l’instant.')}</li>
    {/each}
  </ul>

  <div style="flex-shrink: 0; display: flex; justify-content: flex-end;">
    <button type="button" onclick={openFolder} style="height: 34px; padding: 0 14px; border: 0; border-radius: 999px; cursor: pointer; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--text);">{t('Ouvrir le dossier')}</button>
  </div>
</div>

<style>
  /* The whole text of a row leads to the mod's page. */
  .info {
    flex-grow: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 2px 6px;
    margin: -2px 0;
    border-radius: 12px;
    text-decoration: none;
  }
  .info:hover {
    scale: 1;
    background: var(--surface-soft);
  }
</style>
