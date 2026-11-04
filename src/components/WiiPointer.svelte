<script>
  import { pointer } from '../lib/remote-nav.svelte.js';
  import hand from '../assets/cursor/point.png';
</script>

<!-- Same hand as the minigame picker; its fingertip (17, 2) is the hotspot. -->
{#if pointer.shown}
  <img class="hand" src={hand} width="48" height="48" alt="" style="transform: translate({pointer.x - 17}px, {pointer.y - 2}px) scale({pointer.pressed ? 0.88 : 1});" />
{/if}

<style>
  .hand {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 1000;
    pointer-events: none;
    transform-origin: 17px 2px;
  }
</style>
