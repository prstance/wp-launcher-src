<script>
  import Sidebar from '../components/Sidebar.svelte';
  import { wiimotes, PLAYERS, hasPlayer } from '../lib/wiimotes.svelte.js';
  import { toolsStatus, dolphinOf } from '../lib/tools.js';
  import { settings } from '../lib/settings.svelte.js';
  import { game, launch, stop, dismiss } from '../lib/game.svelte.js';
  import { t, plural } from '../lib/i18n.svelte.js';

  let dolphin = $state(null);
  let error = $state(null);
  $effect(() => {
    toolsStatus().then((tools) => (dolphin = dolphinOf(tools)));
  });

  const ok = (state = t('OK')) => ({ done: true, bg: '#25a720', border: '0', fg: 'var(--text)', state, stateFg: 'var(--ok)' });
  const now = (state = t('En cours')) => ({ done: false, bg: 'var(--card)', border: '5px solid var(--accent)', fg: 'var(--text)', state, stateFg: 'var(--accent-deep)' });
  const wait = (state = t('En attente')) => ({ done: false, bg: 'var(--card)', border: '4px solid var(--line)', fg: 'var(--muted)', state, stateFg: 'var(--muted)' });
  const bad = (state) => ({ done: false, bg: 'var(--card)', border: '5px solid #a32a3c', fg: 'var(--text)', state, stateFg: 'var(--danger)' });

  const connected = $derived(wiimotes.slots.filter(Boolean).length);
  const running = $derived(game.phase === 'running');

  const steps = $derived([
    {
      label: t('Vérification de l’ISO'),
      meta: settings.iso?.name ?? t('Aucun ISO choisi'),
      ...(game.phase === 'verifying'
        ? now()
        : game.phase === 'preparing'
          ? ok(settings.verifyIso ? t('OK') : t('Passée'))
        : game.phase === 'problems'
          ? bad(t('Anomalies'))
          : !settings.iso
            ? bad(t('Manquant'))
            : running
              ? ok(settings.verifyIso ? t('OK') : t('Passée'))
              : wait()),
    },
    dolphin
      ? { ...ok(), label: t('Dolphin prêt'), meta: t('Version {v}, intégrée au launcher', { v: dolphin.installedVersion }) }
      : { ...bad(t('Manquant')), label: 'Dolphin', meta: t('Absent de ce build du launcher') },
    running
      ? connected
        ? { ...ok(), label: t('Manettes'), meta: t('Transmises au jeu, dans le même ordre') }
        : { ...now(t('À toi')), label: t('Manettes'), meta: t('Appuie sur 1 et 2 de chaque Wiimote') }
      : hasPlayer()
        ? { ...wait(), label: t('Manettes'), meta: plural(connected, '{n} connectée, gardée pour le jeu', '{n} connectées, gardées pour le jeu') }
        : { ...bad(t('Aucune')), label: t('Manettes'), meta: t('Connecte au moins une Wiimote pour lancer le jeu') },
    { ...(running ? ok(t('En cours')) : game.phase === 'error' ? bad(t('Échec')) : wait()), label: t('Partie'), meta: running ? t('Le jeu tourne dans Dolphin') : t('Démarrage dans Dolphin') },
  ]);

  const attempt = async (work) => {
    error = null;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
  };
  const again = (force = false) => attempt(() => launch(force));
  const otherIso = async () => {
    await dismiss();
    location.replace('#/LaunchIso');
  };
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; inset: 0;">
    <div style="position: absolute; left: 300px; top: -260px; width: 760px; height: 760px; border-radius: 999px; border: 72px solid var(--decor); box-sizing: border-box;"></div>
    <div style="position: absolute; left: 940px; top: 420px; width: 400px; height: 400px; border-radius: 999px; border: 44px solid var(--decor); box-sizing: border-box;"></div>
  </div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="home" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; align-items: center; justify-content: center; gap: 28px; padding: 40px;">
    <section aria-labelledby="ld-title" style="width: 600px; box-sizing: border-box; background: var(--card); border: 6px solid var(--accent-soft); border-radius: 44px; padding: 34px 38px; box-shadow: 0 12px 40px rgba(var(--shadow),0.14); display: flex; flex-direction: column; gap: 22px;">
      <div>
        <p style="margin: 0; font-size: 13px; font-weight: 800; color: var(--label);">{t('Étape {n} sur {total} · {step}', { n: 3, total: 3, step: t('Lancement') })}</p>
        <h1 id="ld-title" style="margin: 4px 0 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Préparation de la partie')}</h1>
      </div>

      <ol style="margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 12px;">
        {#each steps as s}
          <li style="display: flex; align-items: center; gap: 14px;">
            <span aria-hidden="true" style="width: 34px; height: 34px; border-radius: 999px; flex-shrink: 0; box-sizing: border-box; display: flex; align-items: center; justify-content: center; background: {s.bg}; border: {s.border};">
              {#if s.done}
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"></path></svg>
              {/if}
            </span>
            <span style="flex-grow: 1; display: flex; flex-direction: column;">
              <span style="font-size: 16px; font-weight: 800; color: {s.fg};">{s.label}</span>
              <span style="font-size: 12px; font-weight: 700; color: var(--muted);">{s.meta}</span>
            </span>
            <span style="font-size: 12px; font-weight: 800; color: {s.stateFg};">{s.state}</span>
          </li>
        {/each}
      </ol>

      {#if game.phase === 'verifying'}
        <div role="progressbar" aria-label={t('Vérification de l’ISO')} class="bar"><div></div></div>
      {:else if game.phase === 'preparing'}
        <div role="progressbar" aria-label={t('Préparation des fichiers')} class="bar"><div></div></div>
        <p role="status" style="margin: 0; font-size: 13px; font-weight: 700; color: var(--muted);">{t('Première fois avec cette version : ses fichiers sont fabriqués à partir de ton ISO.')}</p>
      {:else if game.message || error}
        <p role="alert" style="margin: 0; padding: 12px 16px; border-radius: 20px; background: var(--danger-bg); font-size: 14px; line-height: 21px; font-weight: 700; color: var(--danger); overflow-wrap: anywhere;">{game.message ?? error}</p>
      {:else if running}
        <p role="status" style="margin: 0; padding: 12px 16px; border-radius: 20px; background: var(--surface-soft); font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{t('Le jeu est lancé. Pour le quitter, ferme la fenêtre de Dolphin ou appuie sur « {stop} ».', { stop: t('Arrêter le jeu') })}</p>
      {:else}
        <p role="status" style="margin: 0; padding: 12px 16px; border-radius: 20px; background: var(--surface-soft); font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{t('Aucune partie en cours.')}</p>
      {/if}


      <div style="display: flex; align-items: center; gap: 14px;">
      <a href="#/Main" style="display: inline-flex; align-items: center; gap: 10px; height: 48px; padding: 0 20px 0 10px; border-radius: 999px; background: var(--card); text-decoration: none; box-shadow: inset 0 0 0 2px var(--line), 0 4px 0 rgba(var(--shadow),0.10);">
        <span aria-hidden="true" style="width: 28px; height: 28px; border-radius: 8px; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 800; color: var(--text);">B</span>
        <span style="font-size: 16px; font-weight: 800; color: var(--text);">{t('Retour')}</span>
      </a>

      {#snippet primary(text, onclick, badge = 'A')}
        <button type="button" {onclick} style="display: inline-flex; align-items: center; gap: 10px; height: 52px; padding: 0 24px 0 12px; border: 0; border-radius: 999px; cursor: pointer; white-space: nowrap; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 5px 0 var(--accent-deep);">
          <span aria-hidden="true" style="width: 28px; height: 28px; border-radius: 999px; background: var(--card); border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 800; color: var(--text);">{badge}</span>
          <span style="font-size: 18px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px var(--accent-deep); paint-order: stroke fill;">{text}</span>
        </button>
      {/snippet}

      <div style="margin-left: auto; display: flex; align-items: center; gap: 14px;">
        {#if running}
          {@render primary(t('Arrêter le jeu'), stop)}
        {:else if game.phase !== 'verifying' && game.phase !== 'preparing'}
          {#if game.phase === 'problems'}
            <button type="button" onclick={otherIso} style="height: 48px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; white-space: nowrap; background: var(--surface); font-size: 15px; font-weight: 800; color: var(--text);">{t('Autre ISO')}</button>
          {/if}
          {#if !settings.iso}
            {@render primary(t('Choisir un ISO'), () => location.replace('#/LaunchIso'))}
          {:else if !hasPlayer()}
            {@render primary(t('Connecter une manette'), () => (location.hash = '#/Controllers'))}
          {:else if game.phase === 'problems'}
            {@render primary(t('Lancer quand même'), () => again(true))}
          {:else}
            {@render primary(game.phase === 'error' ? t('Réessayer') : t('Jouer'), () => again())}
          {/if}
        {/if}
      </div>
      </div>
    </section>

    <aside style="width: 340px; display: flex; flex-direction: column; gap: 18px;">
      <div style="background: var(--sky); border-radius: 28px; padding: 22px; display: flex; flex-direction: column; gap: 12px;">
        <h2 style="margin: 0; font-size: 17px; font-weight: 800; color: var(--sky-ink);">{t('Manettes')}</h2>
        <div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px;">
          {#each PLAYERS as p, i}
            {@const r = wiimotes.slots[i]}
            <span style="display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: 16px; background: {r ? 'var(--card)' : 'var(--surface-soft)'}; font-size: 13px; font-weight: 800; color: {r ? 'var(--text)' : 'var(--sky-ink)'};">
              <span style="width: 30px; height: 30px; border-radius: 999px; background: {r ? p.color : 'var(--line)'}; color: {r ? '#ffffff' : 'var(--muted)'}; display: flex; align-items: center; justify-content: center; font-size: 12px;">{t('J{n}', { n: i + 1 })}</span>{r ? t('Connectée') : t('Libre')}
            </span>
          {/each}
        </div>
        {#if wiimotes.paused && !connected}
          <p style="margin: 0; font-size: 13px; line-height: 20px; font-weight: 700; color: var(--sky-ink);">{t('Appuie en même temps sur 1 et 2 de chaque Wiimote : leurs LED clignotent, puis le jeu les reconnaît.')}</p>
        {:else if !wiimotes.paused}
          <p style="margin: 0; font-size: 13px; line-height: 20px; font-weight: 700; color: var(--sky-ink);">{t('Elles restent connectées au lancement, chacune à sa place.')}</p>
          <a href="#/Controllers" style="font-size: 13px; font-weight: 800; color: var(--sky-ink);">{t('Gérer les manettes')}</a>
        {/if}
      </div>
    </aside>
  </main>
</div>

<style>
  .bar {
    height: 18px;
    border-radius: 999px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    width: 40%;
    height: 100%;
    border-radius: 999px;
    background: var(--grad-bottom);
    animation: slide 1.1s ease-in-out infinite alternate;
  }
  @keyframes slide {
    to {
      transform: translateX(150%);
    }
  }
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
