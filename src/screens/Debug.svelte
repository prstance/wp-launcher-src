<script>
  // The test tools, in development only (`npm run tauri dev`): the launcher's own (wp-core
  // mods/debug), then those the version and the enabled mods give with `"debug": true`
  // (src-tauri/src/mods.rs, debug_state).
  import { invoke } from '@tauri-apps/api/core';
  import Sidebar from '../components/Sidebar.svelte';
  import SettingRow from '../components/SettingRow.svelte';
  import Notes from '../components/Notes.svelte';
  import { inTauri } from '../lib/tauri.js';
  import { t, local } from '../lib/i18n.svelte.js';
  import { refreshMods } from '../lib/mods.svelte.js';

  let state = $state(null);
  let error = $state(null);
  let busy = $state(false);

  const attempt = async (work) => {
    busy = true;
    error = null;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  };

  const load = async () => {
    state = await invoke('debug_state');
  };

  $effect(() => {
    if (inTauri) attempt(load);
  });

  /** Saves the group's values at once; {} puts its defaults back. */
  const save = (group, values) =>
    attempt(async () => {
      try {
        state = await invoke('debug_settings_set', { id: group.id, values });
      } catch (e) {
        await load();
        throw e;
      }
      await refreshMods();
    });

  const change = (group) => (setting, value) => {
    setting.value = value;
    save(group, Object.fromEntries(group.settings.map((s) => [s.id, s.value])));
  };

  const isDefault = (group) => group.settings.every((s) => s.value === s.default);
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div style="position: relative; z-index: 1;">
    <Sidebar active="debug" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 16px; padding: 22px 40px 0 44px; min-width: 0;">
    <div style="display: flex; align-items: baseline; gap: 16px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 48px; font-weight: 800; color: var(--accent);">{t('Debug')}</h1>
      <span style="font-size: 13px; font-weight: 700; color: var(--muted);">{t('Le launcher en développement seulement. Les outils s’appliquent au prochain lancement du jeu.')}</span>
    </div>

    {#if !inTauri || !state}
      <p style="margin: 0; font-size: 15px; font-weight: 700; color: var(--muted);">{!inTauri ? t('Disponible dans l’application de bureau.') : (error ?? t('Un instant…'))}</p>
    {:else}
      <div class="fade-in" style="flex-grow: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; padding-bottom: 24px;">
        {#if error}
          <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424; overflow-wrap: anywhere;">{error}</p>
        {/if}
        <Notes notes={state.problems} />

        {#each state.groups as group (group.id)}
          <section style="background: #ffffff; border-radius: 28px; padding: 18px 24px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10); display: flex; flex-direction: column; gap: 8px;">
            <div style="display: flex; align-items: center; gap: 12px;">
              <h2 style="flex-grow: 1; margin: 0; font-size: 18px; font-weight: 800;">{local(group.name)}</h2>
              <button type="button" onclick={() => save(group, {})} disabled={busy || isDefault(group)} style="flex-shrink: 0; height: 34px; padding: 0 14px; border: 0; border-radius: 999px; cursor: pointer; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--text);">{t('Tout désactiver')}</button>
            </div>
            <div style="display: flex; flex-direction: column;">
              {#each group.settings as s, i (s.id)}
                <SettingRow setting={s} last={i === group.settings.length - 1} {busy} onchange={change(group)} />
              {/each}
            </div>
          </section>
        {:else}
          <p style="margin: 0; font-size: 14px; font-weight: 700; color: var(--muted);">{t('Aucun outil de test : src-tauri/debug est vide.')}</p>
        {/each}
      </div>
    {/if}
  </main>
</div>
