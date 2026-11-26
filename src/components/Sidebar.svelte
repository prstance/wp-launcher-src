<script>
  import { t } from '../lib/i18n.svelte.js';
  import { settings } from '../lib/settings.svelte.js';
  import { myMii } from '../lib/miis.svelte.js';
  import MiiHead from './MiiHead.svelte';

  let props = $props();

  const v = $derived.by(() => {
    const a = props.active ?? 'home';
    const onBg = 'linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%)';
    const onSh = 'inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 var(--accent-deep)';
    const idle = 'var(--muted)';
    const pick = (k) => a === k ? { bg: onBg, fg: '#ffffff', sh: onSh } : { bg: 'transparent', fg: idle, sh: 'none' };
    const h = pick('home'), m = pick('miis'), l = pick('library'), v = pick('versions'), p = pick('pads'), s = pick('settings'), d = pick('debug');
    return {
      railBg: 'var(--rail)',
      railLine: 'var(--rail-line)',
      tileBase: 'rgba(var(--shadow),0.10)',
      dot: 'var(--dot)',
      homeHref: '#/Main',
      libHref: '#/Library',
      homeBg: h.bg, homeFg: h.fg, homeSh: h.sh,
      miiBg: m.bg, miiFg: m.fg, miiSh: m.sh,
      libBg: l.bg, libFg: l.fg, libSh: l.sh,
      verBg: v.bg, verFg: v.fg, verSh: v.sh,
      padBg: p.bg, padFg: p.fg, padSh: p.sh,
      setBg: s.bg, setFg: s.fg, setSh: s.sh,
      dbgBg: d.bg, dbgFg: d.fg, dbgSh: d.sh
    };
  });
</script>

<nav aria-label={t('Navigation principale')} style="width: 104px; height: 100%; box-sizing: border-box; padding: 20px 20px 24px; display: flex; flex-direction: column; align-items: center; gap: 22px; background: {v.railBg}; font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; box-shadow: inset -2px 0 0 {v.railLine};">
  <a href="#/Miis" aria-label={t('Mon compte : {name}', { name: settings.player?.name ?? '' })} title={settings.player?.name} style="margin-bottom: -10px; display: flex; flex-direction: column; align-items: center; gap: 6px; text-decoration: none;">
    <span style="width: 64px; height: 64px; border-radius: 22px; background: var(--card); display: flex; align-items: center; justify-content: center; overflow: hidden; box-shadow: inset 0 -5px 0 {myMii()?.color ?? v.dot}33, 0 5px 0 {v.tileBase}, 0 10px 20px rgba(60,30,50,0.10);">
      <MiiHead mii={myMii()} size={58} />
    </span>
    <span style="max-width: 92px; font-size: 13px; font-weight: 800; color: var(--label); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">{settings.player?.name ?? ''}</span>
  </a>

  <div style="background: var(--card); border-radius: 28px; padding: 10px; display: flex; flex-direction: column; gap: 10px; box-shadow: 0 6px 0 {v.tileBase}, 0 12px 24px rgba(60,30,50,0.10);">
    <a href={v.homeHref} aria-label={t('Accueil')} aria-current={(props.active ?? 'home') === 'home' ? 'page' : undefined} title={t('Accueil')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.homeBg}; color: {v.homeFg}; box-shadow: {v.homeSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 11l8-7 8 7v9a1 1 0 0 1-1 1h-4v-6H9v6H5a1 1 0 0 1-1-1z"></path></svg>
    </a>
    <a href="#/Miis" aria-label={t('Mes Mii')} aria-current={props.active === 'miis' ? 'page' : undefined} title={t('Mes Mii')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.miiBg}; color: {v.miiFg}; box-shadow: {v.miiSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="9" r="5"></circle><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6"></path></svg>
    </a>
    <a href={v.libHref} aria-label={t('Bibliothèque de mods')} aria-current={props.active === 'library' ? 'page' : undefined} title={t('Bibliothèque de mods')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.libBg}; color: {v.libFg}; box-shadow: {v.libSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3.5" y="3.5" width="7" height="7" rx="2"></rect><rect x="13.5" y="3.5" width="7" height="7" rx="2"></rect><rect x="3.5" y="13.5" width="7" height="7" rx="2"></rect><path d="M17 13.5v7M13.5 17h7"></path></svg>
    </a>
    <a href="#/Versions" aria-label={t('Versions')} aria-current={(props.active ?? 'home') === 'versions' ? 'page' : undefined} title={t('Versions')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.verBg}; color: {v.verFg}; box-shadow: {v.verSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l9 5-9 5-9-5z"></path><path d="M3 12.5l9 5 9-5"></path><path d="M3 17l9 5 9-5"></path></svg>
    </a>
    <a href="#/Controllers" aria-label={t('Manettes')} aria-current={(props.active ?? 'home') === 'pads' ? 'page' : undefined} title={t('Manettes')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.padBg}; color: {v.padFg}; box-shadow: {v.padSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="8" y="2" width="8" height="20" rx="3"></rect><path d="M12 5.5v4M10 7.5h4"></path><circle cx="12" cy="14" r="1.3"></circle><path d="M10.5 18h3"></path></svg>
    </a>
    <a href="#/Settings" aria-label={t('Paramètres')} aria-current={(props.active ?? 'home') === 'settings' ? 'page' : undefined} title={t('Paramètres')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.setBg}; color: {v.setFg}; box-shadow: {v.setSh};">
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3.2"></circle><path d="M12 2.8v2.6M12 18.6v2.6M2.8 12h2.6M18.6 12h2.6M5.5 5.5l1.8 1.8M16.7 16.7l1.8 1.8M5.5 18.5l1.8-1.8M16.7 7.3l1.8-1.8"></path></svg>
    </a>
    {#if settings.dev}
      <!-- (the launcher in development only: its test tools, screens/Debug.svelte) -->
      <a href="#/Debug" aria-label={t('Debug')} aria-current={props.active === 'debug' ? 'page' : undefined} title={t('Debug')} style="width: 56px; height: 56px; border-radius: 18px; display: flex; align-items: center; justify-content: center; text-decoration: none; background: {v.dbgBg}; color: {v.dbgFg}; box-shadow: {v.dbgSh};">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="7" y="7" width="10" height="13" rx="5"></rect><path d="M9.5 7.5a2.5 2.5 0 0 1 5 0M12 11v9M7 12H3.5M20.5 12H17M7.5 17l-3 2M16.5 17l3 2M7.5 8.5l-3-2M16.5 8.5l3-2"></path></svg>
      </a>
    {/if}
  </div>

</nav>
