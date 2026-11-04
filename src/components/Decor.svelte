<script>
  /** Images of a theme (src-tauri/src/versions.rs). Offsets keep them from moving in step. */
  let { items } = $props();
</script>

{#each items as d, i}
  <img src={d.image} alt="" class={d.motion ? `motion-${d.motion}` : ''} style="position: absolute; left: {d.x}px; top: {d.y}px; width: {d.size}px; height: {d.size}px; opacity: {d.opacity}; --o: {d.opacity}; animation-delay: -{i * 1.3}s; {d.motion === 'fall' ? `animation-duration: ${6 + (i % 4) * 1.5}s;` : ''} pointer-events: none;">
{/each}
