<script>
  import Sidebar from '../components/Sidebar.svelte';
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { wiimotes, hasPlayer } from '../lib/wiimotes.svelte.js';
  import { inTauri } from '../lib/tauri.js';
  import { settings, settingsReady, setSettings, pickIso, chooseIso } from '../lib/settings.svelte.js';
  import { game, launch } from '../lib/game.svelte.js';
  import { t, plural } from '../lib/i18n.svelte.js';

  let error = $state(null);
  let busy = $state(false);
  let dragging = $state(false);
  let autoStart = $state(false);

  const connected = $derived(wiimotes.slots.filter(Boolean).length);
  const canStart = $derived(!!settings.iso && hasPlayer());

  const attempt = async (work) => {
    error = null;
    busy = true;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  };
  const browse = () => attempt(() => (inTauri ? pickIso() : Promise.reject(t('Disponible dans l’application de bureau.'))));
  const choose = (path) => attempt(() => chooseIso(path));
  const start = () =>
    attempt(async () => {
      await launch();
      location.replace('#/Launching');
    });

  // Set to skip this screen: goes on as soon as a remote is there.
  $effect(() => {
    if (autoStart && settings.alwaysUseIso && canStart) {
      autoStart = false;
      start();
    }
  });

  onMount(() => {
    settingsReady.then(() => {
      // Already playing, or set to skip this screen.
      if (game.title === 'game' && (game.phase === 'running' || game.phase === 'verifying' || game.phase === 'preparing')) location.replace('#/Launching');
      else autoStart = settings.alwaysUseIso && !!settings.iso;
    });
    if (!inTauri) return;
    const stop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths[0]) choose(payload.paths[0]);
    });
    return () => stop.then((f) => f());
  });
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; inset: 0;">
    <div style="position: absolute; left: 640px; top: -220px; width: 640px; height: 640px; border-radius: 999px; border: 64px solid var(--tint); box-sizing: border-box;"></div>
    <div style="position: absolute; left: 148px; top: 120px; font-size: 108px; line-height: 108px; font-weight: 800; color: var(--accent); -webkit-text-stroke: 14px #ffffff; paint-order: stroke fill;">Wii Party</div>
  </div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="home" />
  </div>

  <div style="position: absolute; inset: 0; z-index: 2; background: rgba(var(--scrim),0.52); display: flex; align-items: center; justify-content: center;">
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <section role="dialog" aria-modal="true" aria-labelledby="iso-title" style="width: 700px; box-sizing: border-box; background: #ffffff; border-radius: 40px; padding: 30px 34px 26px; box-shadow: 0 12px 40px rgba(var(--shadow),0.24); display: flex; flex-direction: column; gap: 18px;">
      <ol aria-label={t('Étapes')} style="margin: 0; padding: 0; list-style: none; display: flex; gap: 8px;">
        <li style="display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 14px 0 4px; border-radius: 999px; background: var(--tint); font-size: 13px; font-weight: 800; color: var(--label);">
          <span style="width: 24px; height: 24px; border-radius: 999px; background: var(--accent); color: #ffffff; display: flex; align-items: center; justify-content: center; font-size: 12px;">1</span>{t('Choisir l’ISO')}
        </li>
        <li style="display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 14px 0 4px; border-radius: 999px; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--muted);">
          <span style="width: 24px; height: 24px; border-radius: 999px; background: var(--line); color: var(--muted); display: flex; align-items: center; justify-content: center; font-size: 12px;">2</span>{t('Vérification')}
        </li>
        <li style="display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 14px 0 4px; border-radius: 999px; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--muted);">
          <span style="width: 24px; height: 24px; border-radius: 999px; background: var(--line); color: var(--muted); display: flex; align-items: center; justify-content: center; font-size: 12px;">3</span>{t('Lancement')}
        </li>
      </ol>

      <div>
        <h1 id="iso-title" style="margin: 0; font-size: 32px; line-height: 38px; font-weight: 800; color: var(--accent);">{t('Choisis ton ISO de Wii Party')}</h1>
        <p style="margin: 6px 0 0; font-size: 14px; line-height: 21px; font-weight: 500; letter-spacing: 0.02em;">{t('Le launcher ne fournit pas le jeu : utilise une copie de ton propre disque. Formats acceptés : {formats}.', { formats: '.iso, .wbfs, .rvz, .gcz, .wia, .ciso' })}</p>
      </div>

      <div style="border-radius: 28px; background: {dragging ? '#8fe6ee' : '#b8f0f4'}; border: 3px dashed #2cb0fc; padding: 22px; display: flex; align-items: center; gap: 20px;">
        <svg width="64" height="64" viewBox="0 0 64 64" aria-hidden="true">
          <circle cx="32" cy="32" r="28" fill="#ffffff"></circle>
          <circle cx="32" cy="32" r="28" fill="none" stroke="#1a5f7a" stroke-width="3"></circle>
          <circle cx="32" cy="32" r="8" fill="#b8f0f4" stroke="#1a5f7a" stroke-width="3"></circle>
          <path d="M18 22 A18 18 0 0 1 30 14" stroke="#2cb0fc" stroke-width="4" fill="none" stroke-linecap="round"></path>
        </svg>
        <div style="flex-grow: 1;">
          <p style="margin: 0; font-size: 18px; font-weight: 800; color: #1a5f7a;">{t('Glisse ton fichier ici')}</p>
          <p style="margin: 2px 0 0; font-size: 13px; font-weight: 700; color: #1a5f7a;">{t('ou ouvre l’explorateur pour le sélectionner')}</p>
        </div>
        <button type="button" onclick={browse} disabled={busy} style="flex-shrink: 0; white-space: nowrap; height: 48px; padding: 0 22px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,#8fd6f6 0%,#3cb4e8 60%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 #244480; font-size: 16px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px #244480; paint-order: stroke fill;">{t('Parcourir mes fichiers')}</button>
      </div>

      <div style="display: flex; flex-direction: column; gap: 8px;">
        <h2 style="margin: 0; font-size: 13px; font-weight: 800; color: var(--label);">{t('Fichiers récents')}</h2>
        {#each settings.recent as r (r.path)}
          {@const on = settings.iso?.path === r.path}
          <button type="button" onclick={() => choose(r.path)} aria-pressed={on} style="display: flex; align-items: center; gap: 14px; padding: 8px 16px 8px 10px; border-radius: 20px; cursor: pointer; text-align: left; background: {on ? 'var(--tint)' : 'var(--surface-soft)'}; border: 0; box-shadow: {on ? 'inset 0 0 0 3px #1283cb' : 'none'};">
            <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column;">
              <span style="font-size: 15px; font-weight: 800; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{r.name}</span>
              <span style="font-size: 12px; font-weight: 700; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{r.path}</span>
            </span>
            <span style="flex-shrink: 0; padding: 3px 10px; border-radius: 999px; background: #d9f0dd; color: #17532a; font-size: 12px; font-weight: 800;">{r.country}</span>
          </button>
        {:else}
          <p style="margin: 0; padding: 12px 16px; border-radius: 20px; background: var(--surface-soft); font-size: 14px; font-weight: 700; color: var(--muted);">{t('Aucun fichier récent : les ISO que tu choisis apparaîtront ici.')}</p>
        {/each}
      </div>

      {#if error}
        <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{error}</p>
      {/if}

      {#if !hasPlayer()}
        <p role="status" style="margin: 0; display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 10px 16px; border-radius: 16px; background: var(--tint); font-size: 14px; font-weight: 800; color: var(--label);">
          {t('Connecte au moins une Wiimote pour lancer le jeu.')}
          <a href="#/Controllers" style="flex-shrink: 0; color: var(--label);">{t('Connecter une manette')}</a>
        </p>
      {/if}

      <label style="display: flex; align-items: center; gap: 10px; font-size: 14px; font-weight: 700;">
        <input type="checkbox" checked={settings.alwaysUseIso} onchange={(e) => setSettings({ alwaysUseIso: e.currentTarget.checked })} style="width: 20px; height: 20px; accent-color: var(--accent);">
        {t('Toujours utiliser ce fichier sans redemander')}
      </label>

      <footer style="display: flex; align-items: center; gap: 14px; padding-top: 4px;">
        <a href="#/Main" style="display: inline-flex; align-items: center; gap: 10px; height: 52px; padding: 0 22px 0 12px; border-radius: 999px; background: #ffffff; text-decoration: none; box-shadow: inset 0 0 0 2px var(--line), 0 4px 0 rgba(var(--shadow),0.10);">
          <span aria-hidden="true" style="width: 28px; height: 28px; border-radius: 8px; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 800; color: var(--text);">B</span>
          <span style="font-size: 17px; font-weight: 800; color: var(--text);">{t('Annuler')}</span>
        </a>
        <span style="flex-grow: 1; text-align: center; font-size: 13px; font-weight: 800; color: var(--label);">{t('Sans mod')} · {connected ? plural(connected, '{n} manette', '{n} manettes') : t('aucune manette')}</span>
        <button type="button" onclick={start} disabled={!canStart || busy} style="display: inline-flex; align-items: center; gap: 10px; height: 56px; padding: 0 28px 0 14px; border: 0; border-radius: 999px; cursor: {canStart ? 'pointer' : 'default'}; opacity: {canStart ? 1 : 0.5}; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 5px 0 var(--accent-deep);">
          <span aria-hidden="true" style="width: 30px; height: 30px; border-radius: 999px; background: #ffffff; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 800; color: var(--text);">A</span>
          <span style="font-size: 22px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 5px var(--accent-deep); paint-order: stroke fill;">{t('Lancer')}</span>
        </button>
      </footer>
    </section>
  </div>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
