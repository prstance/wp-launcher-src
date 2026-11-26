<script>
  // A Wii Remote on a phone's screen (remote.svelte.js): held like one, its top towards the TV.
  import { remote, B, start, press, reconnect, leave, hasToken } from './remote.svelte.js';
  import { PLAYERS } from '../lib/wiimotes.svelte.js';
  import { t } from '../lib/i18n.svelte.js';

  let menu = $state(false);

  /** The player whose LED is lit (null: none yet). */
  const slot = $derived([0, 1, 2, 3].find((i) => remote.leds === 1 << i) ?? null);
  const sideways = $derived(remote.turn === 90 || remote.turn === 270);

  // Each finger on the screen holds what is under it: sliding from a button to another works,
  // and so do several at once.
  const fingers = new Map();

  /** The D-pad's directions under a point, diagonals included. */
  function pad(el, x, y) {
    const r = el.getBoundingClientRect();
    let dx = x - (r.left + r.width / 2);
    let dy = y - (r.top + r.height / 2);
    // In the remote's own frame, which the page turned back.
    const a = remote.turn * (Math.PI / 180);
    [dx, dy] = [dx * Math.cos(a) - dy * Math.sin(a), dx * Math.sin(a) + dy * Math.cos(a)];
    if (Math.hypot(dx, dy) < r.width * 0.1) return 0;
    const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
    let bits = 0;
    if (angle > -157.5 && angle < -22.5) bits |= B.up;
    if (angle > 22.5 && angle < 157.5) bits |= B.down;
    if (Math.abs(angle) < 67.5) bits |= B.right;
    if (Math.abs(angle) > 112.5) bits |= B.left;
    return bits;
  }

  function under(x, y) {
    const el = document.elementFromPoint(x, y)?.closest('[data-button]');
    if (!el) return 0;
    return el.dataset.button === 'pad' ? pad(el, x, y) : Number(el.dataset.button);
  }

  const held = () => press([...fingers.values()].reduce((all, bits) => all | bits, 0));

  function down(e) {
    if (menu) return;
    e.preventDefault();
    fingers.set(e.pointerId, under(e.clientX, e.clientY));
    held();
  }

  function move(e) {
    if (!fingers.has(e.pointerId)) return;
    fingers.set(e.pointerId, under(e.clientX, e.clientY));
    held();
  }

  function up(e) {
    fingers.delete(e.pointerId);
    held();
  }

  const on = (bits) => (remote.buttons & bits) === bits;

  const BYE = $derived({
    removed: t('Le launcher a déconnecté ce téléphone.'),
    full: t('Les 4 places sont prises : déconnecte une manette dans le launcher.'),
    stopped: t('Les téléphones ont été désactivés dans le launcher.'),
    left: t('Ce téléphone est déconnecté.'),
  });
</script>

<svelte:window oncontextmenu={(e) => e.preventDefault()} />

{#if remote.status === 'idle'}
  <main class="start">
    <div class="decor" aria-hidden="true"></div>
    <h1>Wii Party</h1>
    <p class="lead">{t('Ton téléphone devient une Wiimote avec MotionPlus.')}</p>
    <ol>
      <li>{t('Tiens-le comme une Wiimote : l’écran vers le haut, le haut du téléphone vers la télé.')}</li>
      <li>{t('Vise l’écran : le gyroscope déplace le pointeur.')}</li>
      <li>{t('Garde cette page ouverte pendant la partie.')}</li>
    </ol>
    {#if hasToken()}
      <button type="button" class="go" onclick={start}>{t('Commencer')}</button>
    {:else}
      <p class="alert">{t('Ouvre cette page avec le QR code du launcher (Manettes → Téléphone).')}</p>
    {/if}
  </main>
{:else}
  <div class="stage">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="remote"
      class:rumbling={remote.rumble}
      style="width: {sideways ? '100vh' : '100vw'}; height: {sideways ? '100vw' : '100vh'}; rotate: {-remote.turn}deg;"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
    >
      <div class="body">
        <div class="top">
          <button type="button" class="power" aria-label={t('Menu')} onclick={() => (menu = true)}>
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><path d="M12 3v8M6.3 6.6a8 8 0 1 0 11.4 0"></path></svg>
          </button>
          <span class="aim">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20V5M5 11l7-7 7 7"></path></svg>
            {t('Vers la télé')}
          </span>
          {#if slot != null}
            <span class="player" style="background: {PLAYERS[slot].color}; --deep: {PLAYERS[slot].deep};">{t('J{n}', { n: slot + 1 })}</span>
          {:else}
            <span class="player waiting">…</span>
          {/if}
        </div>

        <div class="pad" data-button="pad" aria-label={t('Croix directionnelle')}>
          <span class="arm v" class:lit={on(B.up)} style="top: 0;"><i class="arrow" style="rotate: 0deg;"></i></span>
          <span class="arm v" class:lit={on(B.down)} style="bottom: 0;"><i class="arrow" style="rotate: 180deg;"></i></span>
          <span class="arm h" class:lit={on(B.left)} style="left: 0;"><i class="arrow" style="rotate: -90deg;"></i></span>
          <span class="arm h" class:lit={on(B.right)} style="right: 0;"><i class="arrow" style="rotate: 90deg;"></i></span>
          <span class="hub"></span>
        </div>

        <div class="a" class:down={on(B.a)} data-button={B.a}>A</div>
        <div class="b" class:down={on(B.b)} data-button={B.b}>B</div>

        <div class="row">
          <div class="small" class:down={on(B.minus)} data-button={B.minus}>−</div>
          <div class="small home" class:down={on(B.home)} data-button={B.home} aria-label="HOME">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 11l8-7 8 7M6.5 9.5V20h11V9.5"></path></svg>
          </div>
          <div class="small" class:down={on(B.plus)} data-button={B.plus}>+</div>
        </div>

        <div class="grille" class:talking={remote.talking} aria-hidden="true">
          {#each Array(15) as _}<i></i>{/each}
        </div>

        <div class="numbers">
          <div class="num" class:down={on(B.one)} data-button={B.one}>1</div>
          <div class="num" class:down={on(B.two)} data-button={B.two}>2</div>
        </div>

        <div class="leds" aria-label={slot != null ? t('Joueur {n}', { n: slot + 1 }) : t('Aucun joueur')}>
          {#each [0, 1, 2, 3] as i}<i class:lit={remote.leds & (1 << i)}></i>{/each}
        </div>
      </div>
    </div>

    {#if remote.status !== 'connected' || remote.motion === false}
      <div class="notice" role="status">
        {#if remote.status === 'bye'}
          <p>{BYE[remote.bye] ?? BYE.left}</p>
          <button type="button" class="go" onclick={reconnect}>{t('Se reconnecter')}</button>
        {:else if remote.status === 'lost'}
          <p>{t('Le launcher ne répond pas. Vérifie que les téléphones sont activés (Manettes → Téléphone) et que tu es sur le même Wi-Fi.')}</p>
        {:else if remote.status === 'connecting'}
          <p>{t('Connexion au launcher…')}</p>
        {:else}
          <p>{t('Ce navigateur ne donne pas les mouvements du téléphone : les boutons marchent, mais pas le pointeur.')}</p>
        {/if}
      </div>
    {/if}

    {#if menu}
      <div class="scrim">
        <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
        <section role="dialog" aria-modal="true" aria-label={t('Menu')} class="sheet">
          <button type="button" class="go" onclick={() => (menu = false)}>{t('Reprendre')}</button>
          {#if remote.status === 'connected'}
            <button type="button" class="plain" onclick={() => { menu = false; leave(); }}>{t('Déconnecter ce téléphone')}</button>
          {/if}
        </section>
      </div>
    {/if}
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    touch-action: none;
    overscroll-behavior: none;
    -webkit-touch-callout: none;
    -webkit-tap-highlight-color: transparent;
  }

  /* Touch screens keep :hover after a tap: nothing grows here. */
  :global(button:not(:disabled):hover) {
    scale: 1;
    filter: none;
  }

  .start {
    position: relative;
    height: 100%;
    box-sizing: border-box;
    padding: max(32px, env(safe-area-inset-top)) 24px 32px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 18px;
    overflow: hidden;
    color: var(--text);
  }

  .decor {
    position: absolute;
    right: -140px;
    top: -140px;
    width: 340px;
    height: 340px;
    border-radius: 999px;
    border: 40px solid var(--decor);
    box-sizing: border-box;
    z-index: -1;
  }

  h1 {
    margin: 0;
    font-size: 56px;
    line-height: 60px;
    font-weight: 800;
    color: var(--accent);
    -webkit-text-stroke: 10px var(--title-stroke);
    paint-order: stroke fill;
  }

  .lead {
    margin: 0;
    font-size: 20px;
    line-height: 28px;
    font-weight: 800;
  }

  ol {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 15px;
    line-height: 22px;
    font-weight: 700;
    color: var(--muted);
  }

  .alert {
    margin: 0;
    padding: 12px 16px;
    border-radius: 18px;
    background: var(--danger-bg);
    color: var(--danger);
    font-size: 14px;
    font-weight: 800;
  }

  .go {
    height: 56px;
    padding: 0 28px;
    border: 0;
    border-radius: 999px;
    background: linear-gradient(180deg, var(--grad-top) 0%, var(--grad-bottom) 70%);
    box-shadow:
      inset 0 3px 0 rgba(255, 255, 255, 0.45),
      0 5px 0 var(--accent-deep);
    font-size: 20px;
    font-weight: 800;
    color: #ffffff;
    -webkit-text-stroke: 4px var(--accent-deep);
    paint-order: stroke fill;
  }

  .plain {
    height: 52px;
    border: 0;
    border-radius: 999px;
    background: var(--card);
    box-shadow: inset 0 0 0 2px var(--line);
    font-size: 16px;
    font-weight: 800;
    color: var(--text);
  }

  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  /* The buttons' sizes follow the remote's own width and height, turned or not. */
  .remote {
    container-type: size;
    flex-shrink: 0;
    box-sizing: border-box;
    padding: max(10px, env(safe-area-inset-top)) 12px max(10px, env(safe-area-inset-bottom));
    display: flex;
    justify-content: center;
    user-select: none;
    -webkit-user-select: none;
  }

  .body {
    width: 100%;
    max-width: 380px;
    height: 100%;
    box-sizing: border-box;
    padding: 14px 18px 18px;
    border-radius: 44px;
    background: var(--card);
    box-shadow:
      0 6px 0 rgba(var(--shadow), 0.1),
      0 12px 26px rgba(var(--shadow), 0.1);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
  }

  .rumbling .body {
    animation: rumble 0.07s linear infinite;
  }

  @keyframes rumble {
    25% {
      translate: -2px 1px;
    }
    75% {
      translate: 2px -1px;
    }
  }

  .top {
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .power {
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 999px;
    background: var(--surface);
    color: #e5484d;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .aim {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 800;
    color: var(--muted);
  }

  .player {
    min-width: 40px;
    height: 32px;
    padding: 0 10px;
    box-sizing: border-box;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 17px;
    font-weight: 800;
    color: #ffffff;
    -webkit-text-stroke: 4px var(--deep);
    paint-order: stroke fill;
  }

  .player.waiting {
    background: var(--surface);
    color: var(--muted);
    -webkit-text-stroke: 0;
  }

  /* Pressed: pushed into the remote. */
  [data-button] {
    transition:
      translate 0.06s ease,
      box-shadow 0.06s ease;
  }

  .down {
    translate: 0 4px;
  }

  /* One shadow for the whole cross. */
  .pad {
    position: relative;
    width: min(46cqw, 21cqh, 184px);
    aspect-ratio: 1;
    flex-shrink: 0;
    filter: drop-shadow(0 5px 0 rgba(var(--shadow), 0.14));
  }

  .arm {
    position: absolute;
    background: var(--surface);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background-color 0.06s ease;
  }

  .arm.v {
    left: 34%;
    width: 32%;
    height: 40%;
  }

  .arm.v:first-child {
    border-radius: 12px 12px 0 0;
  }

  .arm.v:nth-child(2) {
    border-radius: 0 0 12px 12px;
  }

  .arm.h {
    top: 34%;
    width: 40%;
    height: 32%;
  }

  .arm.h:nth-child(3) {
    border-radius: 12px 0 0 12px;
  }

  .arm.h:nth-child(4) {
    border-radius: 0 12px 12px 0;
  }

  .hub {
    position: absolute;
    left: 34%;
    top: 34%;
    width: 32%;
    height: 32%;
    background: var(--surface);
  }

  .arm.lit {
    background: var(--tint);
  }

  .arrow {
    width: 0;
    height: 0;
    border-left: 9px solid transparent;
    border-right: 9px solid transparent;
    border-bottom: 12px solid var(--muted);
  }

  .lit .arrow {
    border-bottom-color: var(--accent);
  }

  .a {
    width: min(30cqw, 13cqh, 116px);
    aspect-ratio: 1;
    flex-shrink: 0;
    border-radius: 999px;
    background: linear-gradient(180deg, var(--grad-top) 0%, var(--grad-bottom) 70%);
    box-shadow:
      inset 0 4px 0 rgba(255, 255, 255, 0.45),
      0 6px 0 var(--accent-deep);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 44px;
    font-weight: 800;
    color: #ffffff;
    -webkit-text-stroke: 7px var(--accent-deep);
    paint-order: stroke fill;
  }

  .a.down {
    box-shadow:
      inset 0 4px 0 rgba(255, 255, 255, 0.45),
      0 2px 0 var(--accent-deep);
  }

  /* The trigger under a real remote, here under A. */
  .b {
    width: min(62cqw, 250px);
    height: min(9cqh, 68px);
    flex-shrink: 0;
    border-radius: 22px 22px 34px 34px;
    background: linear-gradient(180deg, #8fd6f6 0%, #3cb4e8 60%);
    box-shadow:
      inset 0 3px 0 rgba(255, 255, 255, 0.45),
      0 5px 0 #244480;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 30px;
    font-weight: 800;
    color: #ffffff;
    -webkit-text-stroke: 6px #244480;
    paint-order: stroke fill;
  }

  .b.down {
    box-shadow:
      inset 0 3px 0 rgba(255, 255, 255, 0.45),
      0 1px 0 #244480;
  }

  .row {
    display: flex;
    gap: min(9cqw, 40px);
    align-items: center;
  }

  .small {
    width: min(14cqw, 6.4cqh, 54px);
    aspect-ratio: 1;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 4px 0 rgba(var(--shadow), 0.14);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 26px;
    font-weight: 800;
    color: var(--muted);
  }

  .small.home {
    color: #2cb0fc;
  }

  .small.down,
  .num.down {
    box-shadow: 0 1px 0 rgba(var(--shadow), 0.14);
    background: var(--tint);
    color: var(--accent);
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(5, 7px);
    gap: 7px;
  }

  .grille i {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: var(--line);
    transition: background-color 0.12s ease;
  }

  .grille.talking i {
    background: var(--accent-soft);
  }

  .numbers {
    display: flex;
    flex-direction: column;
    gap: min(2.4cqh, 18px);
  }

  .num {
    width: min(18cqw, 7.6cqh, 66px);
    aspect-ratio: 1;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 4px 0 rgba(var(--shadow), 0.14);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 28px;
    font-weight: 800;
    color: var(--muted);
  }

  .leds {
    display: flex;
    gap: 12px;
  }

  .leds i {
    width: 18px;
    height: 8px;
    border-radius: 4px;
    background: var(--off);
  }

  .leds i.lit {
    background: #2cb0fc;
    box-shadow: 0 0 10px #2cb0fc;
  }

  .notice {
    position: fixed;
    left: 16px;
    right: 16px;
    bottom: max(16px, env(safe-area-inset-bottom));
    padding: 14px 16px;
    border-radius: 22px;
    background: var(--card);
    box-shadow: 0 10px 30px rgba(var(--shadow), 0.24);
    display: flex;
    flex-direction: column;
    gap: 12px;
    text-align: center;
  }

  .notice p {
    margin: 0;
    font-size: 15px;
    line-height: 21px;
    font-weight: 800;
    color: var(--text);
  }

  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(var(--scrim), 0.52);
    display: flex;
    align-items: flex-end;
    padding: 16px;
    box-sizing: border-box;
  }

  .sheet {
    width: 100%;
    padding: 18px;
    padding-bottom: max(18px, env(safe-area-inset-bottom));
    box-sizing: border-box;
    border-radius: 32px;
    background: var(--card);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
