<script>
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { inTauri } from '../lib/tauri.js';
  import { t } from '../lib/i18n.svelte.js';

  let { title, hint, busy, ondrop, onbrowse } = $props();

  let dragging = $state(false);

  $effect(() => {
    if (!inTauri) return;
    const stop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths.length) ondrop(payload.paths);
    });
    return () => stop.then((f) => f());
  });
</script>

<div style="flex-shrink: 0; padding: 14px 20px; border-radius: 22px; background: {dragging ? 'var(--tint)' : 'var(--surface)'}; border: 3px dashed var(--accent); display: flex; align-items: center; gap: 16px; scale: {dragging ? 1.02 : 1}; transition: scale 0.16s ease, background-color 0.16s ease;">
  <svg class:motion-bob={dragging} width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" style="flex-shrink: 0;"><path d="M12 15V4M7.5 8.5 12 4l4.5 4.5M4 15v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4"></path></svg>
  <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;">
    <span style="font-size: 18px; font-weight: 800;">{busy ? t('Un instant…') : dragging ? t('Lâche ici') : title}</span>
    <span style="font-size: 13px; font-weight: 700; color: var(--muted);">{hint}</span>
  </span>
  <button type="button" onclick={onbrowse} disabled={busy} style="flex-shrink: 0; height: 40px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; background: var(--card); font-size: 14px; font-weight: 800; color: var(--text);">{t('Parcourir')}</button>
</div>
