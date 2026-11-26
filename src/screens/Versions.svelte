<script>
  import Sidebar from '../components/Sidebar.svelte';
  import PackageDrop from '../components/PackageDrop.svelte';
  import Notes from '../components/Notes.svelte';
  import { inTauri } from '../lib/tauri.js';
  import { versions, activeVersion, activateVersion, importVersions, pickVersion, removeVersion } from '../lib/versions.svelte.js';
  import { mods } from '../lib/mods.svelte.js';
  import { t, lang, local } from '../lib/i18n.svelte.js';

  let busy = $state(false);
  let added = $state([]);
  let refused = $state([]);

  const notes = $derived([
    ...[...refused, ...(versions.error ? [versions.error] : [])].map((message) => ({ level: 'error', message })),
    ...mods.problems.filter((p) => p.level === 'error'),
    ...(added.length ? [{ level: 'ok', message: t('Installée : {names}', { names: added.join(', ') }) }] : []),
  ]);

  const size = (bytes) => (bytes < 1024 * 1024 ? t('{n} Ko', { n: Math.ceil(bytes / 1024) }) : t('{n} Mo', { n: Math.round(bytes / 1024 / 1024).toLocaleString(lang()) }));
  const details = (v) => [t('Version {v}', { v: v.version }), ...(v.authors.length ? [v.authors.join(', ')] : []), v.games.join(', '), size(v.size), ...(v.shipped ? [t('Fournie avec le launcher')] : [])].join(' · ');

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
</script>

{#snippet card(name, details, description, active, id, shipped, icon)}
  <li style="flex-shrink: 0; display: flex; align-items: center; gap: 14px; padding: 14px 16px; border-radius: 20px; background: {active ? 'var(--tint)' : 'var(--surface-soft)'}; transition: background-color 0.25s ease;">
    {#if icon}
      <img src={icon} alt="" style="flex-shrink: 0; width: 56px; height: 56px; border-radius: 16px; object-fit: cover; background: var(--card);">
    {/if}
    <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;">
      <span style="font-size: 18px; font-weight: 800; color: {active ? 'var(--label)' : 'var(--text)'};">{name}</span>
      <span style="font-size: 12px; font-weight: 700; color: var(--muted);">{details}</span>
      <span style="margin-top: 2px; font-size: 13px; line-height: 19px; font-weight: 500;">{description}</span>
    </span>
    {#if id}
      <a href="#/ModDetail/{encodeURIComponent(id)}" style="flex-shrink: 0; display: inline-flex; align-items: center; height: 34px; padding: 0 14px; border-radius: 999px; text-decoration: none; background: var(--card); font-size: 13px; font-weight: 800; color: var(--text);">{t('Détails')}</a>
    {/if}
    {#if id && !shipped}
      <button type="button" onclick={() => attempt(() => removeVersion(id))} disabled={busy} style="flex-shrink: 0; height: 34px; padding: 0 14px; border: 0; border-radius: 999px; cursor: pointer; background: var(--card); font-size: 13px; font-weight: 800; color: var(--text);">{t('Retirer')}</button>
    {/if}
    {#if active}
      <span style="flex-shrink: 0; display: inline-flex; align-items: center; height: 40px; padding: 0 18px; border-radius: 999px; background: var(--card); font-size: 14px; font-weight: 800; color: var(--label);">{t('Active')}</span>
    {:else}
      <button type="button" onclick={() => attempt(() => activateVersion(id))} disabled={busy} style="flex-shrink: 0; height: 40px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 3px 0 var(--accent-deep); font-size: 15px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px var(--accent-deep); paint-order: stroke fill;">{t('Utiliser')}</button>
    {/if}
  </li>
{/snippet}

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; left: 980px; top: -240px; width: 540px; height: 540px; border-radius: 999px; border: 56px solid var(--decor); box-sizing: border-box;"></div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="versions" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 18px; padding: 22px 40px 24px 44px; min-width: 0;">
    <header data-tauri-drag-region>
      <h1 style="margin: 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Versions')}</h1>
      <p style="margin: 4px 0 0; max-width: 680px; font-size: 15px; line-height: 23px; font-weight: 500; letter-spacing: 0.02em;">{t('Une version transforme tout le jeu et habille le launcher. Il y en a une seule à la fois, et tes mods s’appliquent par-dessus.')}</p>
    </header>

    <section aria-label={t('Versions installées')} style="flex-grow: 1; min-height: 0; background: var(--card); border-radius: 28px; padding: 20px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06); display: flex; flex-direction: column; gap: 12px;">
      <PackageDrop title={t('Dépose une version ici')} hint={t('Fichier .wpver')} {busy} ondrop={(paths) => attempt(() => importVersions(paths))} onbrowse={() => attempt(pickVersion)} />

      <Notes {notes} />

      <ul style="flex: 1 1 0; min-height: 0; overflow-y: auto; margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 10px;">
        {#each versions.list as v (v.id)}
          {@render card(local(v.name), details(v), local(v.description), v.active, v.id, v.shipped, v.icon)}
        {/each}
        {@render card(t('Wii Party d’origine'), t('Ton ISO, sans version'), t('Le jeu tel qu’il est sur ton disque, et le launcher en rose.'), !activeVersion(), null, false, null)}
      </ul>
    </section>
  </main>
</div>
