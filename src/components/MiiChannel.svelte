<script>
  /** While Dolphin shows the Mii Channel, or after it failed to. */
  import { game, stop, dismiss } from '../lib/game.svelte.js';
  import { t } from '../lib/i18n.svelte.js';

  const open = $derived(game.title === 'miis' && (game.phase === 'running' || game.phase === 'error'));
</script>

{#if open}
  <div style="position: absolute; inset: 0; z-index: 40; background: rgba(var(--scrim),0.52); display: flex; align-items: center; justify-content: center;">
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
    <section role="dialog" aria-modal="true" aria-labelledby="channel-title" class="fade-in" style="width: 560px; box-sizing: border-box; background: #ffffff; border-radius: 40px; padding: 30px 34px 26px; box-shadow: 0 12px 40px rgba(var(--shadow),0.24); display: flex; flex-direction: column; gap: 14px; text-align: center; align-items: center;">
      {#if game.phase === 'running'}
        <svg class="motion-bob" width="72" height="72" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="9" r="5"></circle><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6"></path></svg>
        <h2 id="channel-title" style="margin: 0; font-size: 28px; font-weight: 800; color: var(--accent);">{t('La Chaîne Mii est ouverte')}</h2>
        <p style="margin: 0; font-size: 15px; line-height: 23px; font-weight: 600;">{t('Crée ton Mii dans Dolphin, avec la Wiimote. Dès qu’il est enregistré, la chaîne se ferme et il arrive dans le launcher.')}</p>
        <button type="button" onclick={stop} style="margin-top: 6px; height: 50px; padding: 0 26px; border: 0; border-radius: 999px; cursor: pointer; background: #ffffff; box-shadow: inset 0 0 0 2px var(--line), 0 4px 0 rgba(var(--shadow),0.10); font-size: 16px; font-weight: 800; color: var(--text);">{t('Fermer la Chaîne Mii')}</button>
      {:else}
        <h2 id="channel-title" style="margin: 0; font-size: 26px; font-weight: 800; color: #7a1424;">{t('La Chaîne Mii n’a pas pu s’ouvrir')}</h2>
        <p role="alert" style="margin: 0; font-size: 14px; line-height: 21px; font-weight: 700; color: #7a1424;">{game.message}</p>
        <button type="button" onclick={dismiss} style="margin-top: 6px; height: 50px; padding: 0 26px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 58%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 4px 0 var(--accent-deep); font-size: 16px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px var(--accent-deep); paint-order: stroke fill;">{t('OK')}</button>
      {/if}
    </section>
  </div>
{/if}
