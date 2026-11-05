<script>
  import { routes, router } from './lib/router.svelte.js';
  import WindowControls from './components/WindowControls.svelte';
  import WiiPointer from './components/WiiPointer.svelte';
  import { startRemoteNav } from './lib/remote-nav.svelte.js';
  import { onMount } from 'svelte';
  import { versions } from './lib/versions.svelte.js';
  import { game } from './lib/game.svelte.js';
  import { followLanguage, t } from './lib/i18n.svelte.js';
  import { settings } from './lib/settings.svelte.js';

  // Accueil draws its own window buttons in its header.
  const OWN_CONTROLS = ['Main'];

  // Screens are drawn for 1280×720: scale to fit the window, then let the
  // layout absorb whatever is left over on the other axis.
  let w = $state(window.innerWidth);
  let h = $state(window.innerHeight);
  const zoom = $derived(Math.min(w / 1280, h / 720));

  const Screen = $derived(routes[router.name]);

  // No account yet: the welcome screen makes one first.
  $effect(() => {
    if (settings.loaded && !settings.player && router.name !== 'Welcome') location.replace('#/Welcome');
  });

  onMount(startRemoteNav);
  onMount(followLanguage);
</script>

<svelte:window bind:innerWidth={w} bind:innerHeight={h} />

<div class="stage" class:playing={game.phase === 'running'} style="zoom: {zoom}; width: {w / zoom}px; height: {h / zoom}px;">
  {#if versions.ready}
    {#key router.key}
      <Screen />
    {/key}
  {:else}
    <p class="preparing">{t('Préparation du launcher…')}</p>
  {/if}

  <div class="drag" data-tauri-drag-region></div>
  {#if !OWN_CONTROLS.includes(router.name)}
    <div class="controls">
      <WindowControls color="var(--muted)" />
    </div>
  {/if}
</div>

<WiiPointer />

<style>
  .preparing {
    height: 100%;
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    font-weight: 800;
    color: var(--muted);
  }
  .stage {
    position: relative;
    overflow: hidden;
  }
  /* Thin grab strip along the top edge so screens without a header can still be moved. */
  .drag {
    position: absolute;
    top: 0;
    left: 104px;
    right: 140px;
    height: 12px;
    z-index: 50;
  }
  .controls {
    position: absolute;
    top: 8px;
    right: 12px;
    z-index: 50;
  }
</style>
