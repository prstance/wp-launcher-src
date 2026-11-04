<script>
  import { minimize, toggleMaximize, close } from '../lib/window.js';
  import { t } from '../lib/i18n.svelte.js';

  let { color = '#6b6468' } = $props();
</script>

<div style="display: flex; gap: 4px; color: {color};">
  <button type="button" aria-label={t('Réduire')} onclick={minimize}><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M2 7h10" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg></button>
  <button type="button" aria-label={t('Agrandir')} onclick={toggleMaximize}><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><rect x="2" y="2" width="10" height="10" rx="2.5" stroke="currentColor" stroke-width="2" fill="none"></rect></svg></button>
  <button type="button" aria-label={t('Fermer')} onclick={close}><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg></button>
</div>

<style>
  button {
    width: 40px;
    height: 32px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  button:hover {
    background: rgba(255, 255, 255, 0.7);
  }
</style>
