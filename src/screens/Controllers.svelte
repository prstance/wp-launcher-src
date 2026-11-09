<script>
  import Sidebar from '../components/Sidebar.svelte';
  import { wiimotes, scan, disconnect, identify, moveTo, release, PLAYERS } from '../lib/wiimotes.svelte.js';
  import { camera, barPosition, setBar } from '../lib/remote-nav.svelte.js';
  import { t } from '../lib/i18n.svelte.js';

  let sel = $state(0);
  let releasing = $state(null);
  let error = $state(null);

  const ready = $derived(wiimotes.bluetooth === 'ready');
  const remote = $derived(wiimotes.slots[sel]);

  const PILL = $derived({
    loading: { bg: 'var(--surface)', fg: 'var(--muted)', text: t('Bluetooth…') },
    ready: { bg: '#d9f0dd', fg: '#17532a', text: t('Bluetooth prêt') },
    off: { bg: '#f8e6c6', fg: '#6b4100', text: t('Bluetooth désactivé : active-le sur l’ordinateur') },
    none: { bg: '#fde0e0', fg: '#7a1424', text: t('Aucun adaptateur Bluetooth trouvé') },
    unsupported: { bg: 'var(--surface)', fg: 'var(--muted)', text: t('Pas encore disponible sur ce système') },
  });
  const pill = $derived(
    ready && wiimotes.scanning
      ? { bg: '#b8f0f4', fg: '#1a5f7a', text: t('Recherche : appuie sur 1 et 2') }
      : wiimotes.paused
      ? { bg: 'var(--surface)', fg: 'var(--muted)', text: t('Partie en cours : les manettes sont transmises au jeu') }
      : wiimotes.bluetooth === 'error'
        ? { bg: '#fde0e0', fg: '#7a1424', text: wiimotes.message ?? t('Erreur Bluetooth') }
        : PILL[wiimotes.bluetooth],
  );

  // Empty slots fill up in order, so the first `connecting` empty ones are the ones in progress.
  const pending = $derived.by(() => {
    const empty = wiimotes.slots.flatMap((r, i) => (r ? [] : [i]));
    return new Set(empty.slice(0, wiimotes.connecting));
  });

  const card = (i) => {
    const r = wiimotes.slots[i];
    if (r) {
      return {
        dot: '#25a720', fg: '#17532a', status: t('Connectée'), device: r.model,
        hint: r.extension ? `+ ${r.extension}` : t('Aucune extension'),
        bars: [1, 2, 3, 4].map((k) => (r.battery != null && k <= Math.ceil(r.battery / 25) ? '#ffffff' : 'rgba(255,255,255,0.35)')),
      };
    }
    const bars = [1, 2, 3, 4].map(() => 'rgba(255,255,255,0.35)');
    if (pending.has(i)) return { dot: '#2cb0fc', fg: '#1a5f7a', status: t('Connexion…'), device: t('Wiimote trouvée'), hint: t('Garde-la près de l’ordinateur'), bars };
    if (wiimotes.scanning) return { dot: '#2cb0fc', fg: '#1a5f7a', status: t('Recherche…'), device: t('Appuie sur 1 et 2'), hint: t('En même temps, sur la Wiimote à ajouter'), bars };
    if (wiimotes.paused) return { dot: 'var(--line)', fg: 'var(--muted)', status: t('Libre'), device: t('Pendant la partie'), hint: t('Lance la recherche pour ajouter un joueur'), bars };
    return { dot: 'var(--off)', fg: 'var(--muted)', status: t('Libre'), device: t('Aucune manette'), hint: t('Lance la recherche pour en ajouter une'), bars };
  };

  const toggleScan = () => scan(!wiimotes.scanning);

  const free = async (addr) => {
    releasing = addr;
    error = null;
    try {
      await release(addr);
    } catch (e) {
      error = String(e);
    }
    releasing = null;
  };

  const move = (to) => {
    moveTo(sel, to);
    sel = to;
  };
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; left: 1000px; top: -240px; width: 520px; height: 520px; border-radius: 999px; border: 52px solid var(--tint); box-sizing: border-box;"></div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="pads" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 16px; padding: 22px 40px 24px 44px; min-width: 0;">
    <header data-tauri-drag-region style="display: flex; align-items: center; gap: 18px; height: 48px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Manettes')}</h1>
      <span role="status" style="display: inline-flex; align-items: center; gap: 8px; height: 34px; padding: 0 14px; border-radius: 999px; background: {pill.bg}; font-size: 13px; font-weight: 800; color: {pill.fg};">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 7l10 10-5 4V3l5 4L7 17"></path></svg>
        {pill.text}
      </span>
      <button type="button" onclick={toggleScan} disabled={!ready} style="margin-left: auto; margin-right: 132px; height: 42px; padding: 0 18px; border: 0; border-radius: 999px; cursor: {ready ? 'pointer' : 'default'}; opacity: {ready ? 1 : 0.5}; background: {wiimotes.scanning ? '#ffffff' : 'linear-gradient(180deg,#8fd6f6 0%,#3cb4e8 60%)'}; box-shadow: {wiimotes.scanning ? '0 4px 0 rgba(var(--shadow),0.10)' : 'inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 #244480'}; font-size: 14px; font-weight: 800; color: {wiimotes.scanning ? 'var(--text)' : '#ffffff'}; -webkit-text-stroke: {wiimotes.scanning ? '0' : '3px #244480'}; paint-order: stroke fill;">{wiimotes.scanning ? t('Arrêter la recherche') : t('Rechercher des manettes')}</button>
    </header>

    {#each wiimotes.system as s (s.addr)}
      <div role="alert" style="display: flex; align-items: center; gap: 14px; padding: 10px 12px 10px 16px; border-radius: 22px; background: #ffffff; border: 4px solid #fca20c;">
        <span style="flex-grow: 1; display: flex; flex-direction: column;">
          <span style="font-size: 15px; font-weight: 800; color: #6b4100;">{t('Une {model} est connectée au Bluetooth de l’ordinateur', { model: s.model })}</span>
          <span style="font-size: 13px; font-weight: 700;">{t('Le launcher ne peut pas s’en servir tant que l’ordinateur la garde. Libère-la, puis appuie sur 1 et 2 pendant la recherche.')}</span>
        </span>
        <button type="button" onclick={() => free(s.addr)} disabled={releasing === s.addr} style="flex-shrink: 0; height: 38px; padding: 0 16px; border: 0; border-radius: 999px; cursor: pointer; background: #fca20c; box-shadow: inset 0 3px 0 rgba(255,255,255,0.4), 0 4px 0 #a86400; font-size: 14px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 3px #a86400; paint-order: stroke fill;">{releasing === s.addr ? t('Libération…') : t('La libérer')}</button>
      </div>
    {/each}
    {#if error}
      <p role="alert" style="margin: 0; padding: 8px 14px; border-radius: 14px; background: #fde0e0; font-size: 13px; font-weight: 700; color: #7a1424;">{error}</p>
    {/if}

    <div role="radiogroup" aria-label={t('Joueurs')} style="display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 16px;">
      {#each PLAYERS as p, i}
        {@const c = card(i)}
        <button type="button" role="radio" aria-checked={sel === i} onclick={() => (sel = i)} style="padding: 0; border: 0; text-align: left; cursor: pointer; background: #ffffff; border-radius: 26px; overflow: hidden; display: flex; flex-direction: column; box-shadow: {sel === i ? '0 0 0 4px #ffffff, 0 0 0 10px #1283cb' : '0 0 0 0 transparent'}, 0 6px 0 rgba(var(--shadow),0.10);">
          <span style="height: 58px; width: 100%; box-sizing: border-box; padding: 0 16px; background: {p.color}; display: flex; align-items: center; justify-content: space-between;">
            <span style="font-size: 26px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 6px {p.deep}; paint-order: stroke fill;">{t('J{n}', { n: i + 1 })}</span>
            <span style="display: flex; gap: 3px;" aria-label={wiimotes.slots[i]?.battery != null ? t('Batterie {level} %', { level: wiimotes.slots[i].battery }) : t('Batterie inconnue')}>
              {#each c.bars as b}
                <span style="width: 7px; height: 16px; border-radius: 3px; background: {b};"></span>
              {/each}
            </span>
          </span>
          <span style="padding: 14px 16px 16px; display: flex; flex-direction: column; gap: 6px; min-height: 118px; box-sizing: border-box;">
            <span style="display: inline-flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 800; color: {c.fg};">
              <span style="width: 10px; height: 10px; border-radius: 999px; background: {c.dot};"></span>{c.status}
            </span>
            <span style="font-size: 17px; font-weight: 800; color: var(--text);">{c.device}</span>
            <span style="font-size: 12px; line-height: 17px; font-weight: 700; color: var(--muted);">{c.hint}</span>
          </span>
        </button>
      {/each}
    </div>

    <section aria-labelledby="cfg-title" style="flex-grow: 1; background: #ffffff; border-radius: 32px; padding: 22px 26px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.08); display: flex; flex-direction: column; gap: 16px;">
      <div style="display: flex; align-items: center; gap: 12px;">
        <span style="width: 36px; height: 36px; border-radius: 999px; background: {PLAYERS[sel].color}; color: #ffffff; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 800;">{t('J{n}', { n: sel + 1 })}</span>
        <h2 id="cfg-title" style="margin: 0; font-size: 22px; font-weight: 800;">{t('Joueur {n}', { n: sel + 1 })}{remote ? ` · ${remote.model}` : ''}</h2>
        {#if remote}
          <div style="margin-left: auto; display: flex; gap: 10px;">
            <button type="button" onclick={() => identify(sel)} style="height: 42px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,#8fd6f6 0%,#3cb4e8 60%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 #244480; font-size: 15px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 3px #244480; paint-order: stroke fill;">{t('Faire vibrer')}</button>
            <button type="button" onclick={() => disconnect(sel)} style="height: 42px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; background: #ffffff; box-shadow: inset 0 0 0 2px var(--line); font-size: 15px; font-weight: 800; color: var(--text);">{t('Déconnecter')}</button>
          </div>
        {/if}
      </div>

      {#if remote}
        <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px 40px;">
          <div style="display: flex; flex-direction: column; gap: 14px;">
          <dl style="margin: 0; display: grid; grid-template-columns: auto 1fr; gap: 10px 18px; font-size: 14px; align-content: start;">
            <dt style="font-weight: 800; color: var(--label);">{t('Batterie')}</dt>
            <dd style="margin: 0; font-weight: 800;">{remote.battery != null ? t('{level} %', { level: remote.battery }) : t('Lecture…')}</dd>
            <dt style="font-weight: 800; color: var(--label);">{t('Extension')}</dt>
            <dd style="margin: 0; font-weight: 800;">{remote.extension ?? t('Aucune')}</dd>
            <dt style="font-weight: 800; color: var(--label);">MotionPlus</dt>
            <dd style="margin: 0; font-weight: 800;">{remote.motionPlus ? t('Actif') : t('Absent')}</dd>
            <dt style="font-weight: 800; color: var(--label);">{t('Adresse')}</dt>
            <dd style="margin: 0; font-weight: 700; color: var(--muted); font-variant-numeric: tabular-nums;">{remote.addr}</dd>
          </dl>
          {#if sel === 0}
            <div style="display: flex; gap: 14px; align-items: flex-start;">
              <!-- What the IR camera sees, upright (its y axis points up). -->
              <svg width="112" height="84" viewBox="0 0 1024 768" role="img" aria-label={t('Vue de la caméra infrarouge')} style="flex-shrink: 0; border-radius: 14px; background: var(--text);">
                <path d="M512 0v768M0 384h1024" stroke-width="8" style="stroke: var(--muted)" />
                {#each camera.dots as [x, y]}
                  <circle cx={x} cy={767 - y} r="44" fill="#ffffff" />
                {/each}
              </svg>
              <div style="display: flex; flex-direction: column; gap: 8px; min-width: 0;">
                <span style="font-size: 13px; font-weight: 800; color: var(--label);">{t('Pointeur')}</span>
                <span style="font-size: 12px; line-height: 17px; font-weight: 700; color: var(--muted);">
                  {#if camera.source === 'bar'}
                    {camera.dots.length >= 2 ? t('Sensor bar repérée : vise l’écran.') : t('Un seul point vu : recule un peu ou vise la barre.')}
                  {:else if camera.source === 'gyro'}
                    {t('Pas de sensor bar : la main suit le gyroscope. Pousse-la contre un bord pour la recentrer.')}
                  {:else if remote.motionPlus}
                    {t('Calibrage du gyroscope : laisse la Wiimote immobile une seconde.')}
                  {:else}
                    {t('Aucun point vu et pas de MotionPlus : branche et allume une sensor bar.')}
                  {/if}
                </span>
                <div role="radiogroup" aria-label={t('Position de la sensor bar')} style="display: flex; gap: 4px; padding: 4px; border-radius: 999px; background: var(--surface);">
                  {#each [['above', t('Barre au-dessus')], ['below', t('Barre en dessous')]] as [value, label]}
                    <button type="button" role="radio" aria-checked={barPosition() === value} onclick={() => setBar(value).catch(() => {})} style="flex: 1; height: 30px; padding: 0 10px; border: 0; border-radius: 999px; cursor: pointer; white-space: nowrap; background: {barPosition() === value ? '#ffffff' : 'transparent'}; color: {barPosition() === value ? 'var(--text)' : 'var(--muted)'}; box-shadow: {barPosition() === value ? '0 3px 0 rgba(var(--shadow),0.12)' : 'none'}; font-size: 12px; font-weight: 800;">{label}</button>
                  {/each}
                </div>
              </div>
            </div>
          {/if}
          </div>
          <div style="display: flex; flex-direction: column; gap: 14px;">
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <span style="font-size: 13px; font-weight: 800; color: var(--label);">{t('Test des boutons')}</span>
              <div aria-live="polite" style="min-height: 38px; display: flex; flex-wrap: wrap; align-items: center; gap: 6px;">
                {#each remote.buttons as b}
                  <span style="min-width: 38px; height: 38px; padding: 0 10px; box-sizing: border-box; border-radius: 999px; background: var(--text); color: #ffffff; display: inline-flex; align-items: center; justify-content: center; font-size: 15px; font-weight: 800;">{b}</span>
                {:else}
                  <span style="font-size: 13px; font-weight: 700; color: var(--muted);">{t('Appuie sur un bouton de la Wiimote pour le voir ici.')}</span>
                {/each}
              </div>
            </div>
            <div style="display: flex; flex-direction: column; gap: 8px;">
              <span style="font-size: 13px; font-weight: 800; color: var(--label);">{wiimotes.paused ? t('Changer de joueur : le jeu voit la manette se reconnecter') : t('Changer de joueur')}</span>
              <div role="radiogroup" aria-label={t('Changer de joueur')} style="display: flex; gap: 4px; padding: 4px; border-radius: 999px; background: var(--surface);">
                {#each PLAYERS as _, j}
                  <button type="button" role="radio" aria-checked={sel === j} onclick={() => move(j)} style="flex: 1; height: 36px; border: 0; border-radius: 999px; cursor: pointer; background: {sel === j ? '#ffffff' : 'transparent'}; color: {sel === j ? 'var(--text)' : 'var(--muted)'}; box-shadow: {sel === j ? '0 3px 0 rgba(var(--shadow),0.12)' : 'none'}; font-size: 13px; font-weight: 800;">{t('J{n}', { n: j + 1 })}</button>
                {/each}
              </div>
            </div>
          </div>
        </div>
      {:else}
        <ol style="margin: 0; padding-left: 22px; display: flex; flex-direction: column; gap: 8px; font-size: 15px; line-height: 22px; font-weight: 700;">
          <li>{t('Clique sur « Rechercher des manettes ».')}</li>
          <li>{t('Sur la Wiimote, appuie en même temps sur 1 et 2 (ou sur le bouton rouge SYNC, sous le cache des piles).')}</li>
          <li>{t('Elle prend la première place libre : ses LED indiquent le numéro du joueur.')}</li>
        </ol>
      {/if}

      <p style="margin: auto 0 0; font-size: 12px; font-weight: 700; color: var(--muted);">{t('Les manettes restent connectées au launcher, qui les transmet au jeu dans le même ordre.')}</p>
    </section>
  </main>
</div>
