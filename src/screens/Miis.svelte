<script>
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import Sidebar from '../components/Sidebar.svelte';
  import MiiHead from '../components/MiiHead.svelte';
  import MiiPicker from '../components/MiiPicker.svelte';
  import MiiChannel from '../components/MiiChannel.svelte';
  import { inTauri } from '../lib/tauri.js';
  import { settings, setSettings } from '../lib/settings.svelte.js';
  import { miis, discMiis, loadDiscMiis, myMii, adoptMii, importMiis, pickMiis, renameMii, removeMii, exportMii, channelButton } from '../lib/miis.svelte.js';
  import { t, plural } from '../lib/i18n.svelte.js';

  let selected = $state(settings.player?.mii ? `lib:${settings.player.mii}` : null);
  let pseudo = $state(settings.player?.name ?? '');
  let miiName = $state('');
  let confirming = $state(false);
  let busy = $state(false);
  let error = $state(null);
  let notice = $state(null);
  let dragging = $state(false);
  /** The game's Miis, to add one: { pick: "disc:<i>", name, named }; the name follows
   *  the pick until the player types one. */
  let adding = $state(null);

  const items = $derived(miis.list.map((m) => ({ key: `lib:${m.id}`, mii: m, label: m.name })));
  const current = $derived(miis.list.find((m) => `lib:${m.id}` === selected) ?? null);
  const mine = $derived(current && current.id === settings.player?.mii);
  const full = $derived(miis.list.length >= miis.max);
  const channel = $derived(channelButton());

  $effect(() => {
    miiName = current?.name ?? '';
    confirming = false;
  });

  // A Mii that has just arrived is shown at once.
  let known = null;
  $effect(() => {
    const ids = miis.list.map((m) => m.id);
    const fresh = known && ids.find((id) => !known.includes(id));
    if (fresh) selected = `lib:${fresh}`;
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

  const savePseudo = () =>
    pseudo.trim() && pseudo.trim() !== settings.player?.name && attempt(() => setSettings({ player: { ...settings.player, name: pseudo.trim() } }));
  const rename = () => current && miiName.trim() && miiName.trim() !== current.name && attempt(() => renameMii(current.id, miiName.trim()));
  const itsMe = () => attempt(() => setSettings({ player: { ...settings.player, mii: current.id } }));
  const exportIt = () => attempt(async () => (await exportMii(current.id)) && (notice = t('Mii enregistré.')));
  const remove = () =>
    attempt(async () => {
      await removeMii(current.id);
      selected = null;
    });

  const showDisc = () => {
    adding = { pick: null, name: '', named: false };
    loadDiscMiis();
  };
  const pickDisc = (key) => {
    adding.pick = key;
    if (!adding.named) adding.name = discMiis.list.find((m) => `disc:${m.id}` === key)?.name ?? '';
  };
  const adopt = () =>
    attempt(async () => {
      const id = await adoptMii(Number(adding.pick.slice(5)), adding.name.trim());
      adding = null;
      selected = `lib:${id}`;
    });

  onMount(() => {
    if (!inTauri) return;
    const stop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths.length) attempt(async () => importDone(await importMiis(payload.paths)));
    });
    return () => stop.then((f) => f());
  });
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; left: 960px; top: -260px; width: 560px; height: 560px; border-radius: 999px; border: 56px solid var(--tint); box-sizing: border-box;"></div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="miis" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 14px; padding: 18px 40px 24px 44px; min-width: 0;">
    <header data-tauri-drag-region style="display: flex; align-items: center; gap: 16px; height: 52px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Mes Mii')}</h1>
      <span style="padding: 4px 12px; border-radius: 999px; background: #ffffff; font-size: 13px; font-weight: 800; color: var(--label); box-shadow: 0 3px 0 rgba(var(--shadow),0.08);">{miis.list.length} / {miis.max}</span>
    </header>

    <div style="display: flex; gap: 24px; flex-grow: 1; min-height: 0;">
      <aside style="width: 290px; flex-shrink: 0; background: #ffffff; border-radius: 28px; padding: 22px 20px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.08); display: flex; flex-direction: column; align-items: center; gap: 12px;">
        <p style="margin: 0; align-self: flex-start; font-size: 13px; font-weight: 800; color: var(--label);">{t('Mon compte')}</p>
        <div style="width: 150px; height: 150px; border-radius: 999px; background: var(--surface-soft); box-shadow: inset 0 -8px 0 {myMii()?.color ?? 'var(--accent)'}40; display: flex; align-items: center; justify-content: center;">
          <MiiHead mii={myMii()} size={128} />
        </div>
        <label style="width: 100%; display: flex; flex-direction: column; gap: 4px;">
          <span style="padding-left: 12px; font-size: 12px; font-weight: 800; color: var(--muted);">{t('Pseudo')}</span>
          <input bind:value={pseudo} onchange={savePseudo} maxlength="10" spellcheck="false" style="height: 46px; padding: 0 18px; border: 0; border-radius: 999px; background: var(--surface-soft); box-shadow: inset 0 0 0 2px var(--line); font-size: 18px; font-weight: 800; color: var(--text); outline: none; text-align: center;">
        </label>
        <p style="margin: 0; font-size: 13px; line-height: 19px; font-weight: 700; color: var(--muted); text-align: center;">{myMii() ? t('Ton Mii : {name}. Il est le premier de la liste dans le jeu.', { name: myMii().name }) : t('Pas encore de Mii : choisis-en un dans ta bibliothèque.')}</p>

        <div style="margin-top: auto; width: 100%; display: flex; flex-direction: column; gap: 8px;">
          <p style="margin: 0; font-size: 13px; font-weight: 800; color: var(--label);">{t('Ajouter des Mii')}</p>
          <button type="button" class="add" onclick={() => attempt(async () => importDone(await pickMiis()))} disabled={busy || full}>{t('Importer un fichier Mii')}</button>
          <button type="button" class="add" onclick={showDisc} disabled={busy || full || !settings.iso} title={settings.iso ? undefined : t('Choisis d’abord ton ISO de Wii Party.')}>{t('Partir d’un Mii de Wii Party')}</button>
          <button type="button" class="add" onclick={() => attempt(channel.run)} disabled={busy || !channel.enabled} title={channel.hint}>{channel.label}</button>
          {#if channel.waiting}
            <p style="margin: -2px 0 0 4px; font-size: 12px; line-height: 17px; font-weight: 700; color: var(--muted);">{channel.hint}</p>
          {/if}
        </div>
      </aside>

      <section aria-label={t('Bibliothèque de Mii')} style="flex-grow: 1; min-width: 0; background: {dragging ? 'var(--tint)' : '#ffffff'}; border-radius: 28px; padding: 18px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06); display: flex; flex-direction: column; gap: 12px;">
        <div style="flex-grow: 1; min-height: 0; overflow-y: auto;">
          {#if miis.list.length}
            <MiiPicker {items} {selected} onpick={(k) => (selected = k)} size={72} label={t('Bibliothèque de Mii')} />
          {:else}
            <div style="height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; text-align: center;">
              <p style="margin: 0; font-size: 18px; font-weight: 800;">{t('Ta bibliothèque est vide')}</p>
              <p style="margin: 0; max-width: 420px; font-size: 14px; line-height: 21px; font-weight: 600; color: var(--muted);">{t('Glisse des fichiers Mii ici, pars d’un Mii de Wii Party, ou crée-les dans la Chaîne Mii.')}</p>
            </div>
          {/if}
        </div>

        {#if error}
          <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{error}</p>
        {:else if notice}
          <p role="status" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #d9f0dd; font-size: 13px; font-weight: 700; color: #17532a;">{notice}</p>
        {/if}

        {#if current}
          <footer class="fade-in" style="display: flex; align-items: center; gap: 14px; padding: 10px 12px 10px 10px; border-radius: 24px; background: var(--surface-soft);">
            <span style="width: 56px; height: 56px; border-radius: 999px; background: #ffffff; display: flex; align-items: center; justify-content: center; flex-shrink: 0;"><MiiHead mii={current} size={50} /></span>
            <label style="display: flex; flex-direction: column; gap: 2px; min-width: 0;">
              <span style="font-size: 11px; font-weight: 800; color: var(--muted);">{t('Nom du Mii')}{current.creator ? ` · ${t('créé par {name}', { name: current.creator })}` : ''}</span>
              <input bind:value={miiName} onchange={rename} maxlength="10" spellcheck="false" style="width: 170px; height: 36px; padding: 0 12px; border: 0; border-radius: 12px; background: #ffffff; box-shadow: inset 0 0 0 2px var(--line); font-size: 16px; font-weight: 800; color: var(--text); outline: none;">
            </label>
            <span style="flex-grow: 1;"></span>
            {#if mine}
              <span style="padding: 6px 14px; border-radius: 999px; background: var(--tint); font-size: 13px; font-weight: 800; color: var(--label);">{t('C’est toi !')}</span>
            {:else}
              <button type="button" class="act main" onclick={itsMe} disabled={busy}>{t('C’est moi')}</button>
            {/if}
            <button type="button" class="act" onclick={exportIt} disabled={busy}>{t('Exporter')}</button>
            {#if confirming}
              <button type="button" class="act danger" onclick={remove} disabled={busy}>{t('Oui, supprimer')}</button>
            {:else}
              <button type="button" class="act" onclick={() => (confirming = true)} disabled={busy}>{t('Supprimer')}</button>
            {/if}
          </footer>
        {/if}
      </section>
    </div>
  </main>

  {#if adding}
    <div style="position: absolute; inset: 0; z-index: 30; background: rgba(var(--scrim),0.52); display: flex; align-items: center; justify-content: center;">
      <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
      <section role="dialog" aria-modal="true" aria-labelledby="disc-title" class="fade-in" style="width: 860px; height: 600px; box-sizing: border-box; background: #ffffff; border-radius: 40px; padding: 26px 30px 22px; box-shadow: 0 12px 40px rgba(var(--shadow),0.24); display: flex; flex-direction: column; gap: 14px;">
        <div>
          <h2 id="disc-title" style="margin: 0; font-size: 26px; font-weight: 800; color: var(--accent);">{t('Les Mii de Wii Party')}</h2>
          <p style="margin: 4px 0 0; font-size: 14px; font-weight: 600;">{t('Choisis un visage : il devient un Mii à toi, avec le nom de ton choix.')}</p>
        </div>
        <div style="flex-grow: 1; min-height: 0; overflow-y: auto; border-radius: 24px; background: var(--surface-soft); padding: 10px;">
          {#if discMiis.loading && !discMiis.list.length}
            <p role="status" style="height: 100%; margin: 0; display: flex; align-items: center; justify-content: center; font-size: 16px; font-weight: 800; color: var(--muted);">{t('Le launcher lit les Mii de ton disque…')}</p>
          {:else if discMiis.error}
            <p role="alert" style="margin: 0; padding: 10px 16px; border-radius: 16px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{discMiis.error}</p>
          {:else}
            <MiiPicker items={discMiis.list.map((m) => ({ key: `disc:${m.id}`, mii: m, label: '' }))} selected={adding.pick} onpick={pickDisc} size={58} label={t('Mii de Wii Party')} />
          {/if}
        </div>
        <footer style="display: flex; align-items: center; gap: 12px;">
          <button type="button" class="act" onclick={() => (adding = null)}>{t('Annuler')}</button>
          <span style="flex-grow: 1;"></span>
          <label style="display: flex; align-items: center; gap: 10px; font-size: 13px; font-weight: 800; color: var(--label);">
            {t('Nom')}
            <input bind:value={adding.name} oninput={() => (adding.named = true)} maxlength="10" spellcheck="false" style="width: 170px; height: 40px; padding: 0 14px; border: 0; border-radius: 999px; background: var(--surface-soft); box-shadow: inset 0 0 0 2px var(--line); font-size: 16px; font-weight: 800; color: var(--text); outline: none;">
          </label>
          <button type="button" class="act main" onclick={adopt} disabled={busy || !adding.pick || !adding.name.trim()}>{t('Ajouter')}</button>
        </footer>
      </section>
    </div>
  {/if}

  <MiiChannel />
</div>

<style>
  .add {
    height: 42px;
    padding: 0 16px;
    border: 0;
    border-radius: 16px;
    cursor: pointer;
    text-align: left;
    background: var(--surface-soft);
    box-shadow: inset 0 0 0 2px var(--line);
    font-size: 14px;
    font-weight: 800;
    color: var(--text);
  }
  .act {
    height: 40px;
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    background: #ffffff;
    box-shadow: inset 0 0 0 2px var(--line), 0 3px 0 rgba(var(--shadow), 0.08);
    font-size: 14px;
    font-weight: 800;
    color: var(--text);
    white-space: nowrap;
  }
  .act.main {
    background: linear-gradient(180deg, var(--grad-top) 0%, var(--grad-bottom) 62%);
    box-shadow: inset 0 3px 0 rgba(255, 255, 255, 0.45), 0 4px 0 var(--accent-deep);
    color: #ffffff;
    -webkit-text-stroke: 4px var(--accent-deep);
    paint-order: stroke fill;
  }
  .act.danger {
    background: #a32a3c;
    box-shadow: 0 4px 0 #6e1424;
    color: #ffffff;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
