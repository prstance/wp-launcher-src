<script>
  import { modsSummary } from '../lib/mods.svelte.js';
  import Sidebar from '../components/Sidebar.svelte';
  import { minimize, toggleMaximize, close } from '../lib/window.js';
  import { wiimotes, PLAYERS, hasPlayer } from '../lib/wiimotes.svelte.js';
  import { toolsStatus, dolphinOf } from '../lib/tools.js';
  import { inTauri } from '../lib/tauri.js';
  import { theme, activeVersion } from '../lib/versions.svelte.js';
  import { game } from '../lib/game.svelte.js';
  import Decor from '../components/Decor.svelte';
  import { t, plural, local } from '../lib/i18n.svelte.js';

  const spaces = [
    { x: 14, y: 168, c: '#fc6c78' }, { x: 62, y: 190, c: '#fca20c' }, { x: 112, y: 176, c: '#72cc72' },
    { x: 150, y: 138, c: '#3cb4e8' }, { x: 112, y: 104, c: '#fc6c78' }, { x: 76, y: 66, c: '#c864f8' },
    { x: 112, y: 26, c: '#72cc72' }, { x: 166, y: 14, c: '#fca20c' }, { x: 216, y: 40, c: '#fc6c78' },
    { x: 236, y: 92, c: '#3cb4e8' }, { x: 290, y: 104, c: '#72cc72' }, { x: 304, y: 156, c: '#c864f8' }
  ];

  let tools = $state(null);
  $effect(() => {
    toolsStatus().then((t) => (tools = t));
  });

  const ready = $derived(!!dolphinOf(tools) && hasPlayer());
  const connected = $derived(wiimotes.slots.filter(Boolean).length);
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">

  <div aria-hidden="true" style="position: absolute; inset: 0; pointer-events: none;">
    <div style="position: absolute; left: 640px; top: -220px; width: 640px; height: 640px; border-radius: 999px; border: 64px solid var(--tint); box-sizing: border-box;"></div>
    <div style="position: absolute; left: 1010px; top: 330px; width: 420px; height: 420px; border-radius: 999px; border: 44px solid var(--tint); box-sizing: border-box;"></div>
    <div style="position: absolute; left: 520px; top: 400px; width: 120px; height: 120px; border-radius: 999px; background: var(--tint);"></div>
    <Decor items={theme.home} />
  </div>

  <div style="position: relative; z-index: 1;">
    <Sidebar active="home" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; padding: 0 40px 28px 44px; min-width: 0;">
    <header data-tauri-drag-region style="height: 56px; display: flex; align-items: center; justify-content: space-between;">
      <span></span>
      <div style="display: flex; align-items: center; gap: 22px;">
        <div style="display: flex; align-items: center; gap: 8px;">
          <span aria-hidden="true" style="width: 28px; height: 28px; border-radius: 9px; background: var(--accent); color: #ffffff; font-size: 17px; font-weight: 800; display: flex; align-items: center; justify-content: center; box-shadow: inset 0 2px 0 rgba(255,255,255,0.4);">P</span>
          <span style="font-size: 17px; font-weight: 800; color: var(--text); letter-spacing: 0.01em;">Party<span style="color: var(--accent);">Launcher</span></span>
        </div>
        <div style="display: flex; gap: 4px; margin-right: -24px;">
          <button type="button" aria-label={t('Réduire')} onclick={minimize} style="width: 40px; height: 32px; border: 0; border-radius: 10px; background: transparent; color: var(--muted); cursor: pointer;"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M2 7h10" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg></button>
          <button type="button" aria-label={t('Agrandir')} onclick={toggleMaximize} style="width: 40px; height: 32px; border: 0; border-radius: 10px; background: transparent; color: var(--muted); cursor: pointer;"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><rect x="2" y="2" width="10" height="10" rx="2.5" stroke="currentColor" stroke-width="2" fill="none"></rect></svg></button>
          <button type="button" aria-label={t('Fermer')} onclick={close} style="width: 40px; height: 32px; border: 0; border-radius: 10px; background: transparent; color: var(--muted); cursor: pointer;"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg></button>
        </div>
      </div>
    </header>

    <section style="position: relative; height: 396px;">
      <div style="position: relative; z-index: 2; width: 600px; padding-top: 18px; display: flex; flex-direction: column; align-items: flex-start;">
        <span style="display: inline-flex; align-items: center; gap: 8px; padding: 7px 14px 7px 8px; border-radius: 999px; background: #ffffff; box-shadow: 0 4px 0 rgba(var(--shadow),0.10); font-size: 13px; font-weight: 800; color: var(--label);">
          <span class:motion-pulse={ready} style="width: 18px; height: 18px; border-radius: 999px; background: {ready ? '#25a720' : '#fca20c'}; display: inline-block; box-shadow: inset 0 2px 0 rgba(255,255,255,0.45);"></span>
          {!inTauri ? t('Aperçu dans le navigateur') : !tools ? t('Vérification…') : game.phase === 'running' ? t('Partie en cours') : ready ? t('Prêt à jouer') : dolphinOf(tools) ? t('Connecte une Wiimote') : t('Dolphin absent de ce build')}
        </span>
        <h1 style="margin: 14px 0 0; font-size: 108px; line-height: 108px; font-weight: 800; letter-spacing: -0.01em; color: var(--accent); -webkit-text-stroke: 14px #ffffff; paint-order: stroke fill; text-shadow: 0 8px 0 rgba(var(--shadow),0.10);">Wii Party</h1>
        <p style="margin: 8px 0 0; max-width: 500px; font-size: 17px; line-height: 27px; font-weight: 500; letter-spacing: 0.02em;">{local(theme.tagline) ?? t('Connecte tes Wiimotes, appuie sur Jouer, puis indique ton ISO de Wii Party.')}</p>

        <div style="margin-top: 26px; display: flex; align-items: center; gap: 14px;">
          <a href="#/LaunchIso" style="display: inline-flex; align-items: center; gap: 12px; height: 64px; padding: 0 36px 0 18px; border-radius: 999px; text-decoration: none; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 5px 0 var(--accent-deep), 0 12px 22px rgba(var(--shadow),0.20);">
            <span aria-hidden="true" style="width: 32px; height: 32px; border-radius: 999px; background: #ffffff; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 15px; font-weight: 800; color: var(--text);">A</span>
            <span style="font-size: 26px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 6px var(--accent-deep); paint-order: stroke fill;">{t('Jouer')}</span>
          </a>
          <a href="#/Settings" aria-label={t('Options de lancement')} style="width: 56px; height: 56px; border-radius: 999px; background: #ffffff; color: var(--muted); display: flex; align-items: center; justify-content: center; box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 5px 0 rgba(var(--shadow),0.12);">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="3.2"></circle><path d="M12 2.8v2.6M12 18.6v2.6M2.8 12h2.6M18.6 12h2.6M5.5 5.5l1.8 1.8M16.7 16.7l1.8 1.8M5.5 18.5l1.8-1.8M16.7 7.3l1.8-1.8"></path></svg>
          </a>
        </div>

        <div style="margin-top: 18px; display: flex; align-items: center; gap: 12px;">
          <a href="#/Controllers" style="display: inline-flex; align-items: center; gap: 6px; height: 40px; padding: 0 12px; border-radius: 999px; background: #ffffff; text-decoration: none; box-shadow: 0 4px 0 rgba(var(--shadow),0.10);">
            <span style="font-size: 13px; font-weight: 800; color: var(--label); margin-right: 2px;">{t('Manettes')}</span>
            {#each PLAYERS as p, i}
              <span style="width: 24px; height: 24px; border-radius: 999px; background: {wiimotes.slots[i] ? p.color : 'var(--line)'}; color: {wiimotes.slots[i] ? '#ffffff' : 'var(--muted)'}; font-size: 11px; font-weight: 800; display: flex; align-items: center; justify-content: center;">{t('J{n}', { n: i + 1 })}</span>
            {/each}
            <span style="margin-left: 2px; font-size: 12px; font-weight: 700; color: var(--muted);">{connected ? plural(connected, '{n} connectée', '{n} connectées') : t('aucune')}</span>
          </a>
        </div>
      </div>

      {#if theme.hero.length}
        <div aria-hidden="true" style="position: absolute; right: -30px; top: -32px; width: 550px; height: 460px; pointer-events: none;">
          <Decor items={theme.hero} />
        </div>
      {:else}
        <div aria-hidden="true" style="position: absolute; right: 10px; top: 8px; width: 470px; height: 380px;">
          <div class="motion-float" style="position: absolute; left: 60px; top: 40px; width: 390px; height: 260px; border-radius: 40px; background: #ffffff; transform: rotate(-7deg); box-shadow: 0 8px 0 rgba(var(--shadow),0.10), 0 24px 50px rgba(var(--shadow),0.16);">
            <div style="position: absolute; inset: 18px; border-radius: 26px; background: #f2e8d4;">
              {#each spaces as s}
                <span style="position: absolute; left: {s.x}px; top: {s.y}px; width: 40px; height: 40px; border-radius: 999px; background: {s.c}; box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 4px 0 rgba(60,30,40,0.18);"></span>
              {/each}
              <span style="position: absolute; left: 132px; top: 66px; width: 48px; height: 48px; border-radius: 999px; background: #1283cb; border: 4px solid #ffffff; box-sizing: border-box; color: #ffffff; font-size: 16px; font-weight: 800; display: flex; align-items: center; justify-content: center; box-shadow: 0 5px 0 rgba(0,0,0,0.18);">1</span>
              <span style="position: absolute; left: 258px; top: 22px; width: 48px; height: 48px; border-radius: 999px; background: #b33f26; border: 4px solid #ffffff; box-sizing: border-box; color: #ffffff; font-size: 16px; font-weight: 800; display: flex; align-items: center; justify-content: center; box-shadow: 0 5px 0 rgba(0,0,0,0.18);">2</span>
              <span style="position: absolute; left: 64px; top: 140px; width: 48px; height: 48px; border-radius: 999px; background: #1f7d1b; border: 4px solid #ffffff; box-sizing: border-box; color: #ffffff; font-size: 16px; font-weight: 800; display: flex; align-items: center; justify-content: center; box-shadow: 0 5px 0 rgba(0,0,0,0.18);">3</span>
              <span style="position: absolute; left: 228px; top: 150px; width: 48px; height: 48px; border-radius: 999px; background: #d9b527; border: 4px solid #ffffff; box-sizing: border-box; color: var(--text); font-size: 16px; font-weight: 800; display: flex; align-items: center; justify-content: center; box-shadow: 0 5px 0 rgba(0,0,0,0.18);">4</span>
            </div>
          </div>
          <div class="motion-wobble" style="position: absolute; left: 0; top: 232px; width: 124px; height: 124px; border-radius: 30px; background: #ffffff; transform: rotate(14deg); box-shadow: inset 0 -8px 0 #f1dde5, 0 8px 0 rgba(var(--shadow),0.10), 0 18px 34px rgba(var(--shadow),0.18);">
            <span style="position: absolute; left: 22px; top: 22px; width: 20px; height: 20px; border-radius: 999px; background: var(--accent);"></span>
            <span style="position: absolute; right: 22px; top: 22px; width: 20px; height: 20px; border-radius: 999px; background: var(--accent);"></span>
            <span style="position: absolute; left: 52px; top: 48px; width: 20px; height: 20px; border-radius: 999px; background: var(--accent);"></span>
            <span style="position: absolute; left: 22px; bottom: 30px; width: 20px; height: 20px; border-radius: 999px; background: var(--accent);"></span>
            <span style="position: absolute; right: 22px; bottom: 30px; width: 20px; height: 20px; border-radius: 999px; background: var(--accent);"></span>
          </div>
          <div class="motion-bob" style="position: absolute; right: 0; top: 0; width: 66px; height: 66px; border-radius: 999px; background: #fada20; box-shadow: inset 0 5px 0 rgba(255,255,255,0.5), 0 5px 0 #b59a00;"></div>
        </div>
      {/if}
    </section>

    <section class="cards" aria-label={t('Raccourcis')} style="margin-top: auto; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 20px; height: 222px;">
      <div style="background: #ffffff; border-radius: 28px; padding: 20px 22px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.08); display: flex; flex-direction: column; gap: 12px;">
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <h2 style="margin: 0; font-size: 17px; font-weight: 800; color: var(--text);">{t('Mods')}</h2>
          <a href="#/Library" style="font-size: 13px; font-weight: 800;">{t('Bibliothèque')}</a>
        </div>
        <p style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{modsSummary()}</p>
      </div>

      <div style="background: #ffffff; border-radius: 28px; padding: 20px 22px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.08); display: flex; flex-direction: column; gap: 12px;">
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <h2 style="margin: 0; font-size: 17px; font-weight: 800; color: var(--text);">{t('Manettes')}</h2>
          <a href="#/Controllers" style="font-size: 13px; font-weight: 800;">{t('Gérer')}</a>
        </div>
        {#if wiimotes.bluetooth === 'unsupported'}
          <p style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{t('Sur ce système, le launcher ne connecte pas encore les Wiimotes : c’est Dolphin qui s’en charge une fois le jeu lancé.')}</p>
        {:else if wiimotes.slots[0]}
          <p style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{t('Pointe la Wiimote J1 vers l’écran pour déplacer la main. La croix passe d’un bouton à l’autre, A valide, B revient en arrière.')}</p>
        {:else}
          <p style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700; color: var(--muted);">{t('Connecte une Wiimote : celle du joueur 1 sert aussi à naviguer dans le launcher.')}</p>
        {/if}
      </div>

      <div style="position: relative; overflow: hidden; background: var(--tint); border-radius: 28px; padding: 20px 22px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10); display: flex; flex-direction: column; align-items: flex-start; gap: 6px;">
        <div aria-hidden="true" style="position: absolute; right: -50px; top: -50px; width: 170px; height: 170px; border-radius: 999px; border: 26px solid var(--grad-top); box-sizing: border-box;"></div>
        <span style="position: relative; padding: 3px 10px; border-radius: 999px; background: #ffffff; font-size: 12px; font-weight: 800; color: var(--label);">{t('Version')}</span>
        <h2 style="position: relative; margin: 4px 0 0; font-size: 30px; line-height: 34px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 6px var(--accent-deep); paint-order: stroke fill;">{local(activeVersion()?.name) ?? t('Wii Party d’origine')}</h2>
        <p style="position: relative; margin: 0; max-width: 250px; font-size: 13px; line-height: 19px; font-weight: 700; color: var(--label);">{activeVersion() ? t('Version {v} : le jeu et le launcher sont à ses couleurs.', { v: activeVersion().version }) : t('Le jeu sans version, et le launcher en rose.')}</p>
        <a href="#/Versions" style="position: relative; margin-top: auto; display: inline-flex; align-items: center; height: 38px; padding: 0 18px; border-radius: 999px; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); text-decoration: none; box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 var(--accent-deep);">
          <span style="font-size: 15px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px var(--accent-deep); paint-order: stroke fill;">{t('Changer de version')}</span>
        </a>
      </div>
    </section>
  </main>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
