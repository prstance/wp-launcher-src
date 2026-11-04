<script>
  import { t } from '../lib/i18n.svelte.js';
  import Sidebar from '../components/Sidebar.svelte';
</script>

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div style="position: relative; z-index: 1;">
    <Sidebar active="library" />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 14px; padding: 18px 40px 24px 44px; min-width: 0;">
    <a href="#/Library" style="align-self: flex-start; display: inline-flex; align-items: center; gap: 10px; height: 40px; padding: 0 16px 0 8px; border-radius: 999px; background: #ffffff; text-decoration: none; box-shadow: 0 4px 0 rgba(var(--shadow),0.10);">
      <span aria-hidden="true" style="width: 26px; height: 26px; border-radius: 8px; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 800; color: var(--text);">B</span>
      <span style="font-size: 14px; font-weight: 800; color: var(--text);">{t('Retour à la bibliothèque')}</span>
    </a>

    <section style="flex-grow: 1; min-height: 0; background: #ffffff; border-radius: 28px; padding: 32px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center;">
      <span aria-hidden="true" style="width: 72px; height: 72px; border-radius: 24px; background: var(--tint); display: flex; align-items: center; justify-content: center;">
        <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--accent)"><path d="M12 3.5l9.5 16.5h-19z"></path><path d="M12 10v4.5M12 17.5v.5"></path></svg>
      </span>
      <h1 style="margin: 6px 0 0; font-size: 20px; font-weight: 800; color: var(--text);">{t('Aucun conflit à résoudre.')}</h1>
      <p style="margin: 0; max-width: 480px; font-size: 15px; line-height: 23px; font-weight: 500; color: var(--muted);">{t('Quand des mods ne pourront pas fonctionner ensemble, tu choisiras ici lesquels garder. Les mods ne sont pas encore disponibles.')}</p>
    </section>
  </main>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
</style>
