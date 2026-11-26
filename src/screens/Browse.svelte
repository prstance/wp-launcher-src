<script>
  import { t } from '../lib/i18n.svelte.js';
  import Sidebar from '../components/Sidebar.svelte';
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; left: 960px; top: -260px; width: 560px; height: 560px; border-radius: 999px; border: 56px solid var(--decor); box-sizing: border-box;"></div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="library" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 14px; padding: 18px 40px 24px 44px; min-width: 0;">
    <header data-tauri-drag-region style="display: flex; align-items: center; gap: 20px; height: 52px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Bibliothèque')}</h1>
      <nav aria-label={t('Sources')} style="display: flex; gap: 6px; padding: 5px; border-radius: 999px; background: var(--card); box-shadow: 0 4px 0 rgba(var(--shadow),0.10);">
        <a href="#/Library" style="display: inline-flex; align-items: center; height: 36px; padding: 0 16px; border-radius: 999px; text-decoration: none; font-size: 14px; font-weight: 800; color: var(--muted);">{t('Installés')}</a>
        <a href="#/Browse" aria-current="page" style="display: inline-flex; align-items: center; gap: 7px; height: 36px; padding: 0 16px; border-radius: 999px; text-decoration: none; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45); font-size: 14px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 3px var(--accent-deep); paint-order: stroke fill;">{t('Chercher en ligne')}</a>
      </nav>
    </header>

    <section aria-label={t('Catalogue en ligne')} style="flex-grow: 1; min-height: 0; background: var(--card); border-radius: 28px; padding: 32px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center;">
      <span aria-hidden="true" style="width: 72px; height: 72px; border-radius: 24px; background: var(--tint); display: flex; align-items: center; justify-content: center;">
        <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke-width="2.2" stroke-linecap="round" style="stroke: var(--accent)"><circle cx="12" cy="12" r="9"></circle><path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18"></path></svg>
      </span>
      <p style="margin: 6px 0 0; font-size: 20px; font-weight: 800;">{t('Le catalogue en ligne n’est pas encore disponible.')}</p>
      <p style="margin: 0; max-width: 480px; font-size: 15px; line-height: 23px; font-weight: 500; color: var(--muted);">{t('Quand il sera prêt, tu pourras y chercher des mods créés par la communauté et les installer depuis le launcher.')}</p>
    </section>
  </main>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
