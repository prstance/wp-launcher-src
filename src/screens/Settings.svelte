<script>
  import Sidebar from '../components/Sidebar.svelte';
  import Select from '../components/Select.svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';
  import { inTauri } from '../lib/tauri.js';
  import { toolsStatus } from '../lib/tools.js';
  import { settings, setSettings, pickIso } from '../lib/settings.svelte.js';
  import { activeVersion } from '../lib/versions.svelte.js';
  import { mods, modsSummary } from '../lib/mods.svelte.js';
  import { t, plural, local, LANGUAGES as LAUNCHER_LANGUAGES } from '../lib/i18n.svelte.js';

  const SECTIONS = $derived([t('Général'), t('Jeu et émulateur'), t('Graphismes'), t('Audio'), t('Mods'), t('À propos')]);
  const LANGUAGES = [[3, 'Français'], [1, 'English'], [2, 'Deutsch'], [4, 'Español'], [5, 'Italiano'], [6, 'Nederlands'], [0, '日本語']];
  // Dolphin's multipliers of the Wii's 640×528.
  const RESOLUTIONS = $derived([[1, t('1× (natif, 528p)')], [2, '2× (720p)'], [3, '3× (1080p)'], [4, '4× (1440p)'], [6, '6× (4K)']]);
  const ASPECTS = $derived([[0, t('Automatique')], [1, '16:9'], [2, '4:3'], [3, t('Étirer à la fenêtre')]]);
  const LAUNCHER_LANGUAGE = $derived([['auto', t('Automatique')], ...LAUNCHER_LANGUAGES]);

  let sec = $state(1);
  let error = $state(null);
  let version = $state(null);
  let tools = $state([]);
  let folders = $state(null);
  /** The launcher's update: "idle", "checking", "none" (up to date), "found" (newer: `newer`), "installing". */
  let updating = $state('idle');
  let newer = $state(null);

  $effect(() => {
    toolsStatus().then((t) => (tools = t));
    if (!inTauri) return;
    getVersion().then((v) => (version = v));
    invoke('folders_info').then((f) => (folders = f));
  });

  const attempt = async (work) => {
    error = null;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
  };
  const set = (key, value) => attempt(() => setSettings({ [key]: value }));
  const changeIso = () => attempt(() => (inTauri ? pickIso() : Promise.reject(t('Disponible dans l’application de bureau.'))));
  const openFolder = (which) => attempt(() => invoke('folder_open', { which }));
  const checkUpdate = () =>
    attempt(async () => {
      updating = 'checking';
      try {
        newer = inTauri ? await invoke('launcher_update_check') : null;
      } finally {
        updating = newer ? 'found' : 'none';
      }
    });
  // (it restarts once installed)
  const installUpdate = () =>
    attempt(async () => {
      updating = 'installing';
      try {
        await invoke('launcher_update_install');
      } catch (e) {
        updating = 'found';
        throw e;
      }
    });
  const launcherInfo = $derived(
    !version
      ? t('Version inconnue')
      : updating === 'found'
        ? t('Version {version} · la {newer} est disponible', { version, newer: newer.version })
        : updating === 'none'
          ? t('Version {version} · à jour', { version })
          : t('Version {version}', { version }),
  );
</script>

{#snippet row(label, desc, control, last)}
  <div style="display: flex; align-items: center; gap: 16px; padding: 11px 0; box-shadow: {last ? 'none' : 'inset 0 -2px 0 var(--surface)'};">
    <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column;">
      <span style="font-size: 15px; font-weight: 800;">{label}</span>
      {#if desc}
        <span style="font-size: 12px; font-weight: 700; color: var(--muted); overflow-wrap: anywhere;">{desc}</span>
      {/if}
    </span>
    {@render control()}
  </div>
{/snippet}

{#snippet toggle(label, key)}
  <button type="button" aria-pressed={settings[key]} aria-label={label} onclick={() => set(key, !settings[key])} style="position: relative; flex-shrink: 0; width: 52px; height: 30px; border: 0; border-radius: 999px; cursor: pointer; background: {settings[key] ? '#25a720' : 'var(--off)'};"><span style="position: absolute; top: 4px; left: {settings[key] ? 26 : 4}px; width: 22px; height: 22px; border-radius: 999px; background: #ffffff; box-shadow: 0 2px 0 rgba(0,0,0,0.15);"></span></button>
{/snippet}

{#snippet choice(label, key, options)}
  <Select {label} value={settings[key]} {options} onchange={(v) => set(key, v)} />
{/snippet}

{#snippet action(text, onclick)}
  <button type="button" {onclick} style="flex-shrink: 0; height: 38px; padding: 0 16px; border: 0; border-radius: 999px; cursor: pointer; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--text);">{text}</button>
{/snippet}

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div style="position: relative; z-index: 1;">
    <Sidebar active="settings" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 16px; padding: 22px 40px 24px 44px; min-width: 0;">
    <div style="display: flex; align-items: center; gap: 20px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 48px; font-weight: 800; color: var(--accent);">{t('Paramètres')}</h1>
    </div>

    <div style="display: flex; gap: 22px; flex-grow: 1; min-height: 0;">
      <nav aria-label={t('Sections des paramètres')} style="width: 230px; flex-shrink: 0; align-self: flex-start; background: #ffffff; border-radius: 28px; padding: 10px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10); display: flex; flex-direction: column; gap: 4px;">
        {#each SECTIONS as label, i}
          <button type="button" aria-current={sec === i ? 'page' : 'false'} onclick={() => (sec = i)} style="height: 44px; padding: 0 16px; border: 0; border-radius: 16px; cursor: pointer; text-align: left; background: {sec === i ? 'var(--tint)' : 'transparent'}; color: {sec === i ? 'var(--label)' : 'var(--text)'}; font-size: 15px; font-weight: 800;">{label}</button>
        {/each}
      </nav>

      <div style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; gap: 16px;">
        {#if error}
          <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{error}</p>
        {/if}

        {#key sec}
        <section class="fade-in" aria-label={SECTIONS[sec]} style="background: #ffffff; border-radius: 28px; padding: 6px 24px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10);">
          <h2 style="margin: 16px 0 4px; font-size: 18px; font-weight: 800;">{SECTIONS[sec]}</h2>

          {#if sec === 0}
            {#snippet launcherLanguage()}{@render choice(t('Langue du launcher'), 'language', LAUNCHER_LANGUAGE)}{/snippet}
            {#snippet versionLink()}{@render action(t('Changer'), () => (location.hash = '#/Versions'))}{/snippet}
            {#snippet autostart()}{@render toggle(t('Ouvrir au démarrage de l’ordinateur'), 'autostart')}{/snippet}
            {#snippet minimize()}{@render toggle(t('Réduire pendant la partie'), 'minimizeInGame')}{/snippet}
            {#snippet discord()}{@render toggle(t('Afficher la partie sur Discord'), 'discordPresence')}{/snippet}
            {@render row(t('Langue du launcher'), t('Automatique suit la langue de l’ordinateur'), launcherLanguage)}
            {@render row(t('Version'), t('{version} : elle décide du jeu et du thème du launcher', { version: local(activeVersion()?.name) ?? t('Wii Party d’origine') }), versionLink)}
            {@render row(t('Ouvrir au démarrage de l’ordinateur'), t('Le launcher s’ouvre quand tu allumes l’ordinateur'), autostart)}
            {@render row(t('Réduire pendant la partie'), t('Le launcher se range dans la barre des tâches et revient à la fin'), minimize)}
            {@render row(t('Afficher la partie sur Discord'), t('Tes amis voient si tu es dans le launcher, dans quel mode et à quel mini-jeu tu joues'), discord, true)}
          {:else if sec === 1}
            {#snippet iso()}{@render action(settings.iso ? t('Changer') : t('Choisir'), changeIso)}{/snippet}
            {#snippet always()}{@render toggle(t('Toujours utiliser cet ISO'), 'alwaysUseIso')}{/snippet}
            {#snippet verify()}{@render toggle(t('Vérifier l’ISO à chaque lancement'), 'verifyIso')}{/snippet}
            {#snippet full()}{@render toggle(t('Plein écran au lancement'), 'fullscreen')}{/snippet}
            {#snippet language()}{@render choice(t('Langue du jeu'), 'gameLanguage', LANGUAGES)}{/snippet}
            {@render row(t('ISO de Wii Party'), settings.iso ? `${settings.iso.path} · ${settings.iso.country}` : t('Aucun ISO choisi pour l’instant'), iso)}
            {@render row(t('Toujours utiliser cet ISO'), t('Jouer lance directement le jeu, sans repasser par le choix de l’ISO'), always)}
            {@render row(t('Vérifier l’ISO à chaque lancement'), t('Quelques secondes pour éviter de lancer une copie abîmée'), verify)}
            {@render row(t('Plein écran au lancement'), t('Le jeu s’ouvre sur tout l’écran'), full)}
            {@render row(t('Langue du jeu'), t('Si le disque la contient (ta version peut en proposer moins)'), language, true)}
          {:else if sec === 2}
            {#snippet resolution()}{@render choice(t('Résolution interne'), 'resolution', RESOLUTIONS)}{/snippet}
            {#snippet aspect()}{@render choice(t('Format d’image'), 'aspect', ASPECTS)}{/snippet}
            {#snippet vsync()}{@render toggle(t('Synchronisation verticale'), 'vsync')}{/snippet}
            {#snippet fps()}{@render toggle(t('Afficher les images par seconde'), 'showFps')}{/snippet}
            {@render row(t('Résolution interne'), t('Plus élevée = plus net, mais plus gourmand'), resolution)}
            {@render row(t('Format d’image'), t('Automatique suit le réglage du jeu'), aspect)}
            {@render row(t('Synchronisation verticale'), t('Évite que l’image se déchire'), vsync)}
            {@render row(t('Afficher les images par seconde'), t('Un compteur dans un coin de l’écran'), fps, true)}
          {:else if sec === 3}
            {#snippet volume()}
              <span style="flex-shrink: 0; display: flex; align-items: center; gap: 12px;">
                <input type="range" aria-label={t('Volume du jeu')} min="0" max="100" step="5" value={settings.volume} oninput={(e) => (settings.volume = Number(e.currentTarget.value))} onchange={(e) => set('volume', Number(e.currentTarget.value))} style="width: 220px; accent-color: var(--accent);">
                <span style="width: 46px; text-align: right; font-size: 14px; font-weight: 800; font-variant-numeric: tabular-nums;">{t('{level} %', { level: settings.volume })}</span>
              </span>
            {/snippet}
            {@render row(t('Volume du jeu'), settings.volume === 0 ? t('Coupé') : t('Le son de la partie'), volume, true)}
          {:else if sec === 4}
            {#snippet open()}{@render action(t('Ouvrir'), () => openFolder('mods'))}{/snippet}
            {@render row(t('Dossier des mods'), folders ? `${folders.mods} · ${mods.list.length ? plural(mods.list.length, '{n} mod', '{n} mods') : t('vide')}` : t('Disponible dans l’application de bureau'), open, true)}
            <p style="margin: 2px 0 12px; font-size: 13px; line-height: 20px; font-weight: 700; color: var(--muted);">{t('Installe et active tes mods dans la')} <a href="#/Library" style="color: var(--label);">{t('Bibliothèque')}</a>. {modsSummary()}</p>
          {:else}
            {#snippet nothing()}{/snippet}
            {#snippet data()}{@render action(t('Ouvrir'), () => openFolder('data'))}{/snippet}
            {#snippet update()}
              {#if updating === 'found'}
                {@render action(t('Mettre à jour'), installUpdate)}
              {:else if updating === 'checking' || updating === 'installing'}
                <span role="status" style="flex-shrink: 0; font-size: 13px; font-weight: 800; color: var(--muted);">{updating === 'checking' ? t('Recherche…') : t('Installation…')}</span>
              {:else}
                {@render action(t('Rechercher une mise à jour'), checkUpdate)}
              {/if}
            {/snippet}
            {@render row('PartyLauncher', launcherInfo, update)}
            {#each tools as tool}
              {@render row(tool.name, tool.installedVersion ? t('Version {version}, intégrée au launcher', { version: tool.installedVersion }) : t('Absent de ce build'), nothing)}
            {/each}
            {@render row(t('Données du launcher'), folders ? t('{folder} (sauvegardes du jeu comprises)', { folder: folders.data }) : t('Disponible dans l’application de bureau'), data, true)}
          {/if}
        </section>
        {/key}

        {#if sec >= 1 && sec <= 3}
          <p style="margin: 0; font-size: 12px; font-weight: 700; color: var(--muted);">{t('Ces réglages s’appliquent au prochain lancement du jeu.')}</p>
        {/if}
      </div>
    </div>
  </main>
</div>
