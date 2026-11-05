<script>
  /** A Mii's head, drawn from the ISO (lib/miis.svelte.js). Until then, or
   *  without a Mii, a plain Mii-like face in its favorite color. */
  let { mii = null, size = 64 } = $props();
</script>

{#if mii?.head}
  <img src={mii.head} alt="" width={size} height={size} style="display: block; width: {size}px; height: {size}px; object-fit: contain;">
{:else}
  <svg width={size} height={size} viewBox="0 0 100 100" aria-hidden="true" style="display: block;">
    <ellipse cx="50" cy="56" rx="31" ry="34" fill="#f7d3b0"></ellipse>
    <path d="M18 52c0-24 14-38 32-38s32 14 32 38c-6-9-14-15-24-17-9 5-26 9-40 17z" fill={mii?.color ?? 'var(--accent)'}></path>
    <ellipse cx="39" cy="60" rx="3.6" ry="5.4" fill="#3c3c3c"></ellipse>
    <ellipse cx="61" cy="60" rx="3.6" ry="5.4" fill="#3c3c3c"></ellipse>
    <path d="M42 75q8 6 16 0" stroke="#c0504a" stroke-width="3.4" fill="none" stroke-linecap="round"></path>
  </svg>
{/if}
