<script>
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import MiiHead from '../components/MiiHead.svelte';
  import MiiPicker from '../components/MiiPicker.svelte';
  import MiiChannel from '../components/MiiChannel.svelte';
  import { inTauri } from '../lib/tauri.js';
  import { settings, settingsReady, setSettings, pickIso, chooseIso } from '../lib/settings.svelte.js';
  import { miis, discMiis, loadDiscMiis, adoptMii, importMiis, pickMiis, channelButton } from '../lib/miis.svelte.js';
  import { wiimotes } from '../lib/wiimotes.svelte.js';
  import { t, plural } from '../lib/i18n.svelte.js';

  // A disc image dropped here is the ISO; anything else, Mii files.
  const DISC = /\.(iso|wbfs|rvz|gcz|wia|ciso)$/i;

  let name = $state(settings.player?.name ?? '');
  /** The player has typed a name of their own. */
  let named = $state(!!settings.player?.name);
  /** "disc:<index>" or "lib:<id>" */
  let selected = $state(null);
  let tab = $state('disc');
  let busy = $state(false);
  let error = $state(null);
  let notice = $state(null);
  let dragging = $state(false);

  const discItems = $derived(discMiis.list.map((m) => ({ key: `disc:${m.id}`, mii: m, label: '' })));
  const libItems = $derived(miis.list.map((m) => ({ key: `lib:${m.id}`, mii: m, label: m.name })));
  const chosen = $derived(
    selected?.startsWith('disc:') ? discMiis.list.find((m) => `disc:${m.id}` === selected) : miis.list.find((m) => `lib:${m.id}` === selected),
  );
  const ready = $derived(name.trim().length > 0 && !busy);

  // Until the player types theirs, the name of the Mii they pick (Matt, Shinnosuke…),
  // or the system's without a Mii.
  $effect(() => {
    const suggested = chosen?.name || miis.suggestedName;
    if (!named && suggested) name = suggested;
  });

  // Never empty-handed: their own Miis first, else one of the game's at random.
  $effect(() => {
    if (selected) return;
    if (miis.list.length) {
      tab = 'mine';
      selected = `lib:${(miis.list.find((m) => m.id === settings.player?.mii) ?? miis.list[0]).id}`;
    } else if (discMiis.list.length) {
      selected = `disc:${discMiis.list[Math.floor(Math.random() * discMiis.list.length)].id}`;
    }
  });

  // What comes back from the Mii Channel or an import is picked at once.
  let known = null;
  $effect(() => {
    const ids = miis.list.map((m) => m.id);
    if (known) {
      const fresh = ids.find((id) => !known.includes(id));
      if (fresh) {
        tab = 'mine';
        selected = `lib:${fresh}`;
      }
    }
    if (miis.ready) known = ids;
  });

  const attempt = async (work) => {
    error = null;
    notice = null;
    busy = true;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  };

  const importDone = (done) => {
    if (!done) return;
    if (done.refused.length) error = done.refused.join(' ');
    if (done.added.length) notice = plural(done.added.length, '{n} Mii ajouté à ta bibliothèque', '{n} Mii ajoutés à ta bibliothèque');
  };
  const importFiles = () => attempt(async () => importDone(await pickMiis()));
  const iso = (path) =>
    attempt(async () => {
      await (path ? chooseIso(path) : pickIso());
      if (settings.iso) loadDiscMiis();
    });
  const channelStep = $derived(channelButton());
  const channel = () => attempt(channelStep.run);

  const start = () =>
    attempt(async () => {
      const player = name.trim();
      let mii = null;
      if (selected?.startsWith('disc:')) mii = await adoptMii(Number(selected.slice(5)), player);
      else if (selected?.startsWith('lib:')) mii = selected.slice(4);
      await setSettings({ player: { name: player, mii } });
      location.replace('#/Main');
    });

  onMount(() => {
    settingsReady.then(() => settings.iso && loadDiscMiis());
    if (!inTauri) return;
    const stop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type !== 'drop' || !payload.paths.length) return;
      const disc = payload.paths.find((p) => DISC.test(p));
      if (disc) iso(disc);
      else attempt(async () => importDone(await importMiis(payload.paths)));
    });
    return () => stop.then((f) => f());
  });
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; inset: 0; pointer-events: none;">
    <div style="position: absolute; left: -180px; top: 300px; width: 620px; height: 620px; border-radius: 999px; border: 64px solid var(--tint); box-sizing: border-box;"></div>
    <div style="position: absolute; left: 1040px; top: -200px; width: 420px; height: 420px; border-radius: 999px; border: 44px solid var(--tint); box-sizing: border-box;"></div>
  </div>
  <div data-tauri-drag-region style="position: absolute; left: 0; right: 0; top: 0; height: 40px; z-index: 1;"></div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; gap: 34px; padding: 44px 48px 36px 64px; min-width: 0;">
    <section aria-labelledby="welcome-title" style="width: 420px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; text-align: center;">
      <h1 id="welcome-title" style="margin: 0; font-size: 64px; line-height: 70px; font-weight: 800; color: var(--accent); -webkit-text-stroke: 10px #ffffff; paint-order: stroke fill; text-shadow: 0 6px 0 rgba(var(--shadow),0.10);">{t('Bienvenue !')}</h1>
      <p style="margin: 6px 0 0; max-width: 380px; font-size: 16px; line-height: 24px; font-weight: 600;">{t('Un pseudo et un Mii, et c’est parti. Tout se change plus tard dans « Mes Mii ».')}</p>

      <div class="motion-float" style="margin-top: 22px; position: relative; width: 210px; height: 210px; border-radius: 999px; background: #ffffff; box-shadow: inset 0 -10px 0 {chosen?.color ?? 'var(--accent)'}40, 0 8px 0 rgba(var(--shadow),0.10), 0 20px 40px rgba(var(--shadow),0.16); display: flex; align-items: center; justify-content: center;">
        {#key chosen?.id}
          <span class="pop"><MiiHead mii={chosen} size={176} /></span>
        {/key}
      </div>

      <label style="margin-top: 22px; width: 340px; display: flex; flex-direction: column; gap: 6px; text-align: left;">
        <span style="padding-left: 16px; font-size: 13px; font-weight: 800; color: var(--label);">{t('Ton pseudo')}</span>
        <input bind:value={name} oninput={() => (named = true)} maxlength="10" spellcheck="false" placeholder={t('Ton pseudo')} style="height: 56px; padding: 0 22px; border: 0; border-radius: 999px; background: #ffffff; box-shadow: inset 0 0 0 3px var(--line), 0 4px 0 rgba(var(--shadow),0.08); font-size: 22px; font-weight: 800; color: var(--text); outline: none; text-align: center;">
      </label>

      <button type="button" onclick={start} disabled={!ready} style="margin-top: 22px; display: inline-flex; align-items: center; gap: 12px; height: 64px; padding: 0 36px 0 18px; border: 0; border-radius: 999px; cursor: {ready ? 'pointer' : 'default'}; opacity: {ready ? 1 : 0.5}; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 5px 0 var(--accent-deep), 0 12px 22px rgba(var(--shadow),0.20);">
        <span aria-hidden="true" style="width: 32px; height: 32px; border-radius: 999px; background: #ffffff; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 15px; font-weight: 800; color: var(--text);">A</span>
        <span style="font-size: 26px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 6px var(--accent-deep); paint-order: stroke fill;">{t('C’est parti !')}</span>
      </button>
    </section>

    <section aria-labelledby="pick-title" style="flex-grow: 1; min-width: 0; background: {dragging ? 'var(--tint)' : '#ffffff'}; border-radius: 40px; padding: 26px 28px 22px; box-shadow: 0 12px 40px rgba(var(--shadow),0.14); display: flex; flex-direction: column; gap: 14px;">
      <header style="display: flex; align-items: center; justify-content: space-between; gap: 14px;">
        <h2 id="pick-title" style="margin: 0; font-size: 26px; font-weight: 800; color: var(--accent);">{t('Choisis ton Mii')}</h2>
        <nav aria-label={t('Sources')} style="display: flex; gap: 6px; padding: 5px; border-radius: 999px; background: var(--surface);">
          {#each [['disc', t('Mii de Wii Party')], ['mine', miis.list.length ? t('Mes Mii ({n})', { n: miis.list.length }) : t('Mes Mii')]] as [key, label]}
            <button type="button" aria-current={tab === key ? 'page' : undefined} onclick={() => (tab = key)} style="height: 34px; padding: 0 16px; border: 0; border-radius: 999px; cursor: pointer; font-size: 14px; font-weight: 800; {tab === key ? 'background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45); color: #ffffff; -webkit-text-stroke: 3px var(--accent-deep); paint-order: stroke fill;' : 'background: transparent; color: var(--muted);'}">{label}</button>
          {/each}
        </nav>
      </header>

      <div style="flex-grow: 1; min-height: 0; overflow-y: auto; border-radius: 24px; background: var(--surface-soft); padding: 10px;">
        {#if tab === 'mine'}
          {#if miis.list.length}
            <MiiPicker items={libItems} {selected} onpick={(k) => (selected = k)} label={t('Mes Mii')} />
          {:else}
            <div style="height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; text-align: center; padding: 20px;">
              <p style="margin: 0; font-size: 17px; font-weight: 800;">{t('Pas encore de Mii à toi')}</p>
              <p style="margin: 0; max-width: 420px; font-size: 14px; line-height: 21px; font-weight: 600; color: var(--muted);">{t('Importe un fichier Mii (.mii, .miigx, .rsd, ou le RFL_DB.dat d’une Wii), ou crée-le dans la Chaîne Mii. Tu peux aussi glisser les fichiers ici.')}</p>
            </div>
          {/if}
        {:else if !settings.iso}
          <div style="height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; padding: 20px;">
            <p style="margin: 0; font-size: 17px; font-weight: 800;">{t('Les Mii de Wii Party sont sur ton disque')}</p>
            <p style="margin: 0; max-width: 430px; font-size: 14px; line-height: 21px; font-weight: 600; color: var(--muted);">{t('Indique ton ISO de Wii Party : le launcher y prend les Mii du jeu et de quoi dessiner leur visage. Tu peux aussi glisser le fichier ici.')}</p>
            <button type="button" onclick={() => iso()} disabled={busy} style="height: 46px; padding: 0 22px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,#8fd6f6 0%,#3cb4e8 60%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 #244480; font-size: 16px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px #244480; paint-order: stroke fill;">{t('Choisir mon ISO')}</button>
          </div>
        {:else if discMiis.loading && !discMiis.list.length}
          <p role="status" style="height: 100%; margin: 0; display: flex; align-items: center; justify-content: center; font-size: 16px; font-weight: 800; color: var(--muted);">{t('Le launcher lit les Mii de ton disque…')}</p>
        {:else if discMiis.error}
          <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{discMiis.error}</p>
        {:else}
          <MiiPicker items={discItems} {selected} onpick={(k) => (selected = k)} size={58} label={t('Mii de Wii Party')} />
        {/if}
      </div>

      {#if error}
        <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{error}</p>
      {:else if notice}
        <p role="status" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #d9f0dd; font-size: 13px; font-weight: 700; color: #17532a;">{notice}</p>
      {/if}

      <footer style="display: flex; gap: 12px;">
        <button type="button" onclick={importFiles} disabled={busy} style="flex: 1; display: flex; align-items: center; gap: 12px; padding: 10px 16px; border: 0; border-radius: 22px; cursor: pointer; text-align: left; background: var(--surface-soft); box-shadow: inset 0 0 0 2px var(--line);">
          <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" style="flex-shrink: 0;"><path d="M12 4v11M7.5 10.5L12 15l4.5-4.5"></path><path d="M4 16v3a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-3"></path></svg>
          <span style="display: flex; flex-direction: column;">
            <span style="font-size: 15px; font-weight: 800; color: var(--text);">{t('Importer un fichier Mii')}</span>
            <span style="font-size: 12px; font-weight: 700; color: var(--muted);">.mii, .miigx, .rsd, RFL_DB.dat</span>
          </span>
        </button>
        <button type="button" onclick={channel} disabled={busy || !channelStep.enabled} style="flex: 1; display: flex; align-items: center; gap: 12px; padding: 10px 16px; border: 0; border-radius: 22px; cursor: pointer; text-align: left; background: var(--surface-soft); box-shadow: inset 0 0 0 2px var(--line); opacity: {channelStep.enabled ? 1 : 0.55};">
          <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" style="flex-shrink: 0;"><circle cx="12" cy="9" r="5"></circle><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6"></path><path d="M18.5 3.5l1 1M20 2v3M18.5 3.5h3"></path></svg>
          <span style="display: flex; flex-direction: column;">
            <span style="font-size: 15px; font-weight: 800; color: var(--text);">{channelStep.label}</span>
            <span class:motion-pulse={wiimotes.scanning} style="font-size: 12px; font-weight: 700; color: var(--muted);">{channelStep.hint}</span>
          </span>
        </button>
      </footer>
    </section>
  </main>

  <MiiChannel />
</div>

<style>
  .pop {
    display: block;
    animation: pop 0.3s cubic-bezier(0.2, 1.5, 0.4, 1) backwards;
  }
</style>
