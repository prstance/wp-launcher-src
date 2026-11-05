<script>
  // A mod's or a version's page: what it is, its own settings, what it changes in
  // the game. "#/ModDetail/<id>" (src-tauri/src/mods.rs, package_detail).
  import { invoke } from '@tauri-apps/api/core';
  import Sidebar from '../components/Sidebar.svelte';
  import Select from '../components/Select.svelte';
  import { inTauri } from '../lib/tauri.js';
  import { router } from '../lib/router.svelte.js';
  import { t, plural, lang, local } from '../lib/i18n.svelte.js';
  import { refreshMods, setModEnabled, removeMod } from '../lib/mods.svelte.js';
  import { activateVersion, removeVersion } from '../lib/versions.svelte.js';

  const id = router.param;

  let detail = $state(null);
  let error = $state(null);
  let busy = $state(false);

  const isVersion = $derived(detail?.kind === 'version');
  const back = $derived(isVersion ? '#/Versions' : '#/Library');

  const size = (bytes) =>
    bytes < 1024 * 1024
      ? t('{n} Ko', { n: Math.ceil(bytes / 1024) })
      : t('{n} Mo', { n: (bytes / 1024 / 1024).toLocaleString(lang(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }) });
  const meta = (d) =>
    [t('Version {v}', { v: d.version }), ...(d.authors.length ? [d.authors.join(', ')] : []), d.games.join(', '), size(d.size), ...(d.shipped ? [t('Fournie avec le launcher')] : [])].join(' · ');
  const number = (n) => n.toLocaleString(lang(), { maximumFractionDigits: 3 });

  const attempt = async (work) => {
    busy = true;
    error = null;
    try {
      await work();
    } catch (e) {
      error = String(e);
    }
    busy = false;
  };

  const load = async () => {
    detail = await invoke('package_detail', { id });
  };

  $effect(() => {
    if (inTauri && id) attempt(load);
  });

  /** Saves every value at once, with `setting` at `value`; {} puts the defaults back. */
  const save = (values) =>
    attempt(async () => {
      try {
        detail = await invoke('package_settings_set', { id, values });
      } catch (e) {
        await load();
        throw e;
      }
      await refreshMods();
    });

  /** A preset's choice gives its values to the settings it sets (its own value comes back from them). */
  const change = (setting, value) => {
    setting.value = value;
    const sets = setting.choices.find((c) => c.value === value)?.sets ?? {};
    for (const s of detail.settings) if (s.id in sets) s.value = sets[s.id];
    save(Object.fromEntries(detail.settings.map((s) => [s.id, s.value])));
  };

  /** A preset's "custom" choice (no `sets`) is only shown while it is the one. */
  const offered = (s) => {
    const preset = s.choices.some((c) => c.sets);
    return s.choices.filter((c) => !preset || c.sets || c.value === s.value).map((c) => [c.value, local(c.label)]);
  };

  const step = (setting, direction) => {
    const next = +(setting.value + direction * (setting.step || 1)).toFixed(10);
    change(setting, Math.min(setting.max ?? Infinity, Math.max(setting.min ?? -Infinity, next)));
  };

  const enable = () => attempt(async () => (isVersion ? activateVersion(id) : setModEnabled(id, !detail.enabled)).then(load));

  const remove = () =>
    attempt(async () => {
      await (isVersion ? removeVersion(id) : removeMod(id));
      location.hash = back;
    });

  const isDefault = $derived(detail?.settings.every((s) => s.value === s.default) ?? true);
  const range = (r) => (r === '*' ? '' : ` (${r})`);
</script>

{#snippet card(title, body)}
  <section style="background: #ffffff; border-radius: 28px; padding: 18px 24px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10); display: flex; flex-direction: column; gap: 8px;">
    <h2 style="margin: 0; font-size: 18px; font-weight: 800;">{title}</h2>
    {@render body()}
  </section>
{/snippet}

{#snippet note(level, text)}
  <p role={level === 'error' ? 'alert' : 'status'} style="margin: 0; padding: 8px 14px; border-radius: 14px; background: {level === 'error' ? '#fde0e0' : level === 'ok' ? '#d9f0dd' : '#f8e6c6'}; font-size: 13px; font-weight: 700; color: {level === 'error' ? '#7a1424' : level === 'ok' ? '#17532a' : '#6b4100'}; overflow-wrap: anywhere;">{text}</p>
{/snippet}

{#snippet setting(s, last)}
  <div style="display: flex; align-items: center; gap: 16px; padding: 11px 0; box-shadow: {last ? 'none' : 'inset 0 -2px 0 var(--surface)'};">
    <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column;">
      <span style="font-size: 15px; font-weight: 800;">{local(s.label)}</span>
      {#if s.description}
        <span style="font-size: 12px; font-weight: 700; color: var(--muted); overflow-wrap: anywhere;">{local(s.description)}</span>
      {/if}
    </span>
    {#if s.type === 'toggle'}
      <button type="button" role="switch" aria-checked={!!s.value} aria-label={local(s.label)} disabled={busy} onclick={() => change(s, s.value ? 0 : 1)} style="position: relative; flex-shrink: 0; width: 52px; height: 30px; border: 0; border-radius: 999px; cursor: pointer; background: {s.value ? '#25a720' : 'var(--off)'};"><span style="position: absolute; top: 4px; left: {s.value ? 26 : 4}px; width: 22px; height: 22px; border-radius: 999px; background: #ffffff; box-shadow: 0 2px 0 rgba(0,0,0,0.15);"></span></button>
    {:else if s.type === 'choice'}
      <Select label={local(s.label)} value={s.value} options={offered(s)} onchange={(v) => change(s, v)} />
    {:else}
      <span role="group" aria-label={local(s.label)} style="flex-shrink: 0; display: flex; align-items: center; gap: 4px; height: 38px; padding: 0 4px; border-radius: 12px; background: var(--surface);">
        <button type="button" class="step" aria-label={t('Moins')} disabled={busy || s.value <= (s.min ?? -Infinity)} onclick={() => step(s, -1)}>−</button>
        <span style="min-width: 48px; text-align: center; font-size: 14px; font-weight: 800; font-variant-numeric: tabular-nums;">{number(s.value)}</span>
        <button type="button" class="step" aria-label={t('Plus')} disabled={busy || s.value >= (s.max ?? Infinity)} onclick={() => step(s, 1)}>+</button>
      </span>
    {/if}
  </div>
{/snippet}

<div style="width: 100%; height: 100%; position: relative; overflow: hidden; display: flex; background: var(--bg); font-family: 'M PLUS Rounded 1c', system-ui, sans-serif; color: var(--text);">
  <div style="position: relative; z-index: 1;">
    <Sidebar active={isVersion ? 'versions' : 'library'} />
  </div>

  <main style="flex-grow: 1; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 14px; padding: 18px 40px 0 44px; min-width: 0;">
    <a href={back} style="flex-shrink: 0; align-self: flex-start; display: inline-flex; align-items: center; gap: 10px; height: 40px; padding: 0 16px 0 8px; border-radius: 999px; background: #ffffff; text-decoration: none; box-shadow: 0 4px 0 rgba(var(--shadow),0.10);">
      <span aria-hidden="true" style="width: 26px; height: 26px; border-radius: 8px; border: 3px solid var(--text); box-sizing: border-box; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 800; color: var(--text);">B</span>
      <span style="font-size: 14px; font-weight: 800; color: var(--text);">{isVersion ? t('Retour aux versions') : t('Retour à la bibliothèque')}</span>
    </a>

    {#if !inTauri || !detail}
      <section style="flex-grow: 1; margin-bottom: 24px; background: #ffffff; border-radius: 28px; padding: 32px; box-sizing: border-box; box-shadow: 0 6px 0 rgba(var(--shadow),0.10); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center;">
        <span aria-hidden="true" style="width: 72px; height: 72px; border-radius: 24px; background: var(--tint); display: flex; align-items: center; justify-content: center;">
          <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--accent)"><path d="M4 7.5l8-4 8 4v9l-8 4-8-4z"></path><path d="M4 7.5l8 4 8-4M12 11.5v9"></path></svg>
        </span>
        <p style="margin: 0; max-width: 480px; font-size: 15px; line-height: 23px; font-weight: 700; color: var(--muted);">{!inTauri ? t('Disponible dans l’application de bureau.') : (error ?? t('Un instant…'))}</p>
      </section>
    {:else}
      <div style="flex-grow: 1; min-height: 0; overflow-y: auto; display: flex; gap: 22px; align-items: flex-start; padding-bottom: 24px;">
        <div class="fade-in" style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; gap: 16px;">
          <section style="background: #ffffff; border-radius: 28px; padding: 22px 24px; box-shadow: 0 6px 0 rgba(var(--shadow),0.10), 0 10px 20px rgba(var(--shadow),0.06); display: flex; flex-direction: column; gap: 14px;">
            <div style="display: flex; align-items: center; gap: 16px;">
              {#if detail.icon}
                <img src={detail.icon} alt="" style="flex-shrink: 0; width: 72px; height: 72px; border-radius: 22px; object-fit: cover;">
              {:else}
                <span aria-hidden="true" style="flex-shrink: 0; width: 72px; height: 72px; border-radius: 22px; background: var(--tint); display: flex; align-items: center; justify-content: center;">
                  <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="stroke: var(--accent)"><path d="M4 7.5l8-4 8 4v9l-8 4-8-4z"></path><path d="M4 7.5l8 4 8-4M12 11.5v9"></path></svg>
                </span>
              {/if}
              <span style="flex-grow: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;">
                <span style="align-self: flex-start; padding: 2px 10px; border-radius: 999px; background: var(--tint); font-size: 12px; font-weight: 800; color: var(--label);">{isVersion ? t('Version') : t('Mod')}</span>
                <h1 style="margin: 0; font-size: 28px; line-height: 36px; font-weight: 800; color: var(--accent); overflow-wrap: anywhere;">{local(detail.name)}</h1>
                <span style="font-size: 12px; font-weight: 700; color: var(--muted);">{meta(detail)}</span>
              </span>
              {#if isVersion}
                {#if detail.enabled}
                  <span style="flex-shrink: 0; display: inline-flex; align-items: center; height: 40px; padding: 0 18px; border-radius: 999px; background: var(--tint); font-size: 14px; font-weight: 800; color: var(--label);">{t('Active')}</span>
                {:else}
                  <button type="button" onclick={enable} disabled={busy} style="flex-shrink: 0; height: 40px; padding: 0 18px; border: 0; border-radius: 999px; cursor: pointer; background: linear-gradient(180deg,var(--grad-top) 0%,var(--grad-bottom) 62%); box-shadow: inset 0 3px 0 rgba(255,255,255,0.45), 0 3px 0 var(--accent-deep); font-size: 15px; font-weight: 800; color: #ffffff; -webkit-text-stroke: 4px var(--accent-deep); paint-order: stroke fill;">{t('Utiliser')}</button>
                {/if}
              {:else}
                <span style="flex-shrink: 0; display: flex; align-items: center; gap: 10px; font-size: 14px; font-weight: 800; color: {detail.enabled ? 'var(--label)' : 'var(--muted)'};">
                  {detail.enabled ? t('Activé') : t('Désactivé')}
                  <button type="button" role="switch" aria-checked={detail.enabled} aria-label={t('Activer {name}', { name: local(detail.name) })} onclick={enable} disabled={busy} style="position: relative; flex-shrink: 0; width: 52px; height: 30px; border: 0; border-radius: 999px; cursor: pointer; background: {detail.enabled ? '#25a720' : 'var(--off)'};"><span style="position: absolute; top: 4px; left: {detail.enabled ? 26 : 4}px; width: 22px; height: 22px; border-radius: 999px; background: #ffffff; box-shadow: 0 2px 0 rgba(0,0,0,0.15);"></span></button>
                </span>
              {/if}
            </div>
            {#if local(detail.description)}
              <p style="margin: 0; font-size: 15px; line-height: 23px; font-weight: 500; white-space: pre-wrap; overflow-wrap: anywhere;">{local(detail.description)}</p>
            {/if}
          </section>

          {#if error}
            {@render note('error', error)}
          {/if}

          {#snippet settingsBody()}
            {#if detail.settings.length}
              <div style="display: flex; flex-direction: column;">
                {#each detail.settings as s, i (s.id)}
                  {@render setting(s, i === detail.settings.length - 1)}
                {/each}
              </div>
              <div style="display: flex; align-items: center; gap: 12px;">
                <span style="flex-grow: 1; font-size: 12px; font-weight: 700; color: var(--muted);">{t('Les réglages s’appliquent au prochain lancement du jeu.')}</span>
                <button type="button" onclick={() => save({})} disabled={busy || isDefault} style="flex-shrink: 0; height: 34px; padding: 0 14px; border: 0; border-radius: 999px; cursor: pointer; background: var(--surface); font-size: 13px; font-weight: 800; color: var(--text);">{t('Valeurs par défaut')}</button>
              </div>
            {:else}
              <p style="margin: 0; font-size: 14px; font-weight: 700; color: var(--muted);">{isVersion ? t('Cette version n’a pas de réglages.') : t('Ce mod n’a pas de réglages.')}</p>
            {/if}
          {/snippet}
          {@render card(t('Réglages'), settingsBody)}
        </div>

        <aside class="fade-in" style="width: 300px; flex-shrink: 0; display: flex; flex-direction: column; gap: 16px;">
          {#snippet changesBody()}
            {@const files = detail.changes.files}
            {#if files.length}
              <details>
                <summary style="cursor: pointer; font-size: 14px; font-weight: 800;">{plural(files.length, '{n} fichier du jeu remplacé', '{n} fichiers du jeu remplacés')}</summary>
                <ul style="margin: 6px 0 0; padding: 8px 10px; max-height: 180px; overflow-y: auto; list-style: none; border-radius: 12px; background: var(--surface-soft); font-family: ui-monospace, monospace; font-size: 11px; line-height: 17px; color: var(--muted); user-select: text; -webkit-user-select: text;">
                  {#each files as file}
                    <li style="overflow-wrap: anywhere;">{file}</li>
                  {/each}
                </ul>
              </details>
            {/if}
            {#if detail.changes.memory}
              <p style="margin: 0; font-size: 14px; font-weight: 800;">{plural(detail.changes.memory, '{n} modification du code du jeu', '{n} modifications du code du jeu')}</p>
            {/if}
            {#if !files.length && !detail.changes.memory && !detail.changes.partial}
              <p style="margin: 0; font-size: 14px; font-weight: 700; color: var(--muted);">{t('Aucune modification détectée avec ces réglages.')}</p>
            {/if}
            {#if detail.changes.partial}
              <p style="margin: 0; font-size: 12px; line-height: 18px; font-weight: 700; color: #6b4100;">{t('Une partie de ce qu’il modifie n’a pas pu être analysée.')}</p>
            {/if}
            {#if Object.keys(detail.requires).length}
              <p style="margin: 4px 0 0; font-size: 13px; line-height: 19px; font-weight: 700;"><span style="color: var(--label);">{t('A besoin de :')}</span> {Object.entries(detail.requires).map(([m, r]) => m + range(r)).join(', ')}</p>
            {/if}
            {#if Object.keys(detail.conflicts).length}
              <p style="margin: 4px 0 0; font-size: 13px; line-height: 19px; font-weight: 700;"><span style="color: var(--label);">{t('Incompatible avec :')}</span> {Object.entries(detail.conflicts).map(([m, r]) => m + range(r)).join(', ')}</p>
            {/if}
          {/snippet}
          {@render card(t('Ce qu’il modifie'), changesBody)}

          {#each detail.problems as problem}
            {@render note(problem.level, problem.level === 'error' ? t('Le jeu ne se lancera pas : {message}', { message: problem.message }) : problem.message)}
          {:else}
            {#if detail.enabled}
              {@render note('ok', t('Aucun conflit avec les autres mods activés.'))}
            {/if}
          {/each}

          {#if !detail.shipped}
            <button type="button" onclick={remove} disabled={busy} style="align-self: flex-start; height: 38px; padding: 0 16px; border: 0; border-radius: 999px; cursor: pointer; background: #ffffff; box-shadow: 0 4px 0 rgba(var(--shadow),0.10); font-size: 13px; font-weight: 800; color: #a3283c;">{isVersion ? t('Retirer cette version') : t('Retirer ce mod')}</button>
          {/if}
        </aside>
      </div>
    {/if}
  </main>
</div>

<style>
  a { color:var(--label); }
  a:hover { color:var(--accent-deep); }
  .step {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 9px;
    cursor: pointer;
    background: #ffffff;
    font-size: 18px;
    font-weight: 800;
    color: var(--text);
  }
  .step:disabled {
    cursor: default;
    opacity: 0.4;
  }
</style>
