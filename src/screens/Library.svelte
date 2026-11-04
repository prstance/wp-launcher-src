<script>
  import { t } from '../lib/i18n.svelte.js';
  import { theme } from '../lib/versions.svelte.js';
  import Decor from '../components/Decor.svelte';
  import Sidebar from '../components/Sidebar.svelte';
  import ModDrop from '../components/ModDrop.svelte';
  import { modsSummary } from '../lib/mods.svelte.js';
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div aria-hidden="true" style="position: absolute; left: 960px; top: -260px; width: 560px; height: 560px; border-radius: 999px; border: 56px solid var(--tint); box-sizing: border-box;"></div>
  <div aria-hidden="true"><Decor items={theme.library} /></div>
  <div style="position: relative; z-index: 1;">
    <Sidebar active="library" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 14px; padding: 18px 40px 24px 44px; min-width: 0;">
    <header data-tauri-drag-region style="display: flex; align-items: center; gap: 20px; height: 52px;">
      <h1 style="margin: 0; font-size: 34px; line-height: 40px; font-weight: 800; color: var(--accent);">{t('Bibliothèque')}</h1>
      <nav aria-label={t('Sources')} style="display: flex; gap: 6px; padding: 5px; border-radius: 999px; background: #ffffff; box-shadow: 0 4px 0 rgba(var(--shadow),0.10);">
        <a href="#/Library" aria-current="page" style="display: inline-flex; align-items: center; height: 36px; padding: 0 16px; border-radius: 999px; text-decoration: none; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45); font-size: 14px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 3px var(--accent-deep); paint-order: stroke fill;">{t('Installés')}</a>
        <a href="#/Browse" style="display: inline-flex; align-items: center; gap: 7px; height: 36px; padding: 0 16px; border-radius: 999px; text-decoration: none; font-size: 14px; font-weight: 800; color: var(--muted);">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18"></path></svg>
          {t('Chercher en ligne')}
        </a>
      </nav>
    </header>

    <div style="display: flex; gap: 24px; flex-grow: 1; min-height: 0;">
      <section aria-label={t('Mes mods')} style="flex-grow: 1; min-width: 0; background: #ffffff; border-radius: 28px; padding: 20px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06);">
        <ModDrop />
      </section>

      <aside style="width: 270px; flex-shrink: 0; background: #ffffff; border-radius: 28px; padding: 20px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.08); display: flex; flex-direction: column; gap: 8px;">
        <p style="margin: 0; font-size: 13px; font-weight: 800; color: var(--label);">{t('Lancer le jeu')}</p>
        <p style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700;">{modsSummary()}</p>
        <a href="#/LaunchIso" style="margin-top: auto; display: inline-flex; align-items: center; justify-content: center; gap: 10px; height: 56px; border-radius: 999px; text-decoration: none; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 4px 0 rgba(255,255,255,0.45), 0 5px 0 var(--accent-deep);">
          <span aria-hidden="true" style="width: 28px; height: 28px; border-radius: 999px; background: #ffffff; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 800; color: var(--text);">A</span>
          <span style="white-space: nowrap; font-size: 19px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 5px var(--accent-deep); paint-order: stroke fill;">{t('Jouer')}</span>
        </a>
      </aside>
    </div>
  </main>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
