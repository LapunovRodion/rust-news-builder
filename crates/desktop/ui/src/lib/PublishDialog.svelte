<!--
  T067, T068 — publishing.

  A dry run first if the editor wants one (FR-031), per-file progress while it works, and the
  fragment offered for copying at the end — which is the thing the whole exercise was for.

  When the operating system has no secret store, the credential is asked for here and held for
  this session only. It is sent to the Rust side and kept in memory; nothing writes it to disk
  (FR-041).
-->
<script lang="ts">
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { onDestroy } from 'svelte';
  import * as api from './api';
  import { run, ui } from './store.svelte';
  import type { PublicationView, PublishProgress, ServerView } from './types';

  const { onclose }: { onclose: () => void } = $props();

  let servers = $state<ServerView[]>([]);
  let chosen = $state('');
  let dryRun = $state(true);
  let storeAvailable = $state(true);
  let sessionSecret = $state('');
  let progress = $state<PublishProgress | null>(null);
  let result = $state<PublicationView | null>(null);
  let copied = $state(false);
  let unlisten: UnlistenFn | null = null;

  $effect(() => {
    void (async () => {
      storeAvailable = (await run(() => api.secretStoreAvailable())) ?? false;
      servers = (await run(() => api.listServers())) ?? [];
      if (!chosen && servers.length > 0) chosen = servers[0].name;
    })();
  });

  $effect(() => {
    void (async () => {
      unlisten = await listen<PublishProgress>('publish-progress', (event) => {
        progress = event.payload;
      });
    })();
    return () => unlisten?.();
  });

  onDestroy(() => unlisten?.());

  const selected = $derived(servers.find((server) => server.name === chosen));
  /** FR-041: with no store and no stored credential, one is asked for and kept for the session. */
  const needsSessionCredential = $derived(
    selected !== undefined && !storeAvailable && !selected.hasCredential,
  );

  async function go() {
    result = null;
    copied = false;
    progress = null;
    if (needsSessionCredential) {
      if (!sessionSecret) return;
      const stored = await run(() => api.setSessionCredential(chosen, sessionSecret));
      if (stored === undefined) return;
      // Out of the component's memory as soon as the Rust side has it.
      sessionSecret = '';
    }
    const publication = await run(() => api.publish(chosen, dryRun));
    if (publication) result = publication;
  }

  async function copy() {
    if (!result) return;
    await writeText(result.fragment);
    copied = true;
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Публикация">
  <div class="panel">
    <header>
      <h2>Публикация</h2>
      <button type="button" onclick={onclose}>Закрыть</button>
    </header>

    {#if servers.length === 0}
      <p class="warn">Сначала добавьте сервер в настройках.</p>
    {:else}
      <label>
        Сервер
        <select bind:value={chosen}>
          {#each servers as server (server.name)}
            <option value={server.name}>{server.name} — {server.host}</option>
          {/each}
        </select>
      </label>

      <label class="check">
        <input type="checkbox" bind:checked={dryRun} />
        Пробный запуск: посчитать всё, но ничего не менять на сервере
      </label>

      {#if needsSessionCredential}
        <label>
          Пароль на этот сеанс
          <input type="password" bind:value={sessionSecret} autocomplete="off" />
        </label>
        <p class="hint">
          Хранилище секретов недоступно. Пароль сохранится только до закрытия программы и не
          будет записан на диск.
        </p>
      {/if}

      <button type="button" class="primary" onclick={go} disabled={ui.busy || !chosen}>
        {dryRun ? 'Проверить' : 'Опубликовать'}
      </button>
    {/if}

    {#if ui.busy && progress}
      <p class="progress">
        {progress.file ? `Отправляется ${progress.file}` : 'Подготовка'}
        {#if progress.total > 0}({progress.index} из {progress.total}){/if}
      </p>
    {/if}

    {#if result}
      <div class="result">
        <p>
          {result.dryRun ? 'Пробный запуск. ' : ''}
          Каталог: <code>{result.remoteFolder}</code>
        </p>
        <p>
          {result.uploaded.length}
          {result.dryRun ? 'файлов было бы отправлено' : 'файлов отправлено'},
          {result.unchanged.length} уже на месте.
        </p>
        {#if result.warnings.length > 0}
          <ul class="warnings">
            {#each result.warnings as warning, index (index)}
              <li>{warning.detail}</li>
            {/each}
          </ul>
        {/if}
        <label>
          Фрагмент для CMS
          <textarea readonly rows="8" value={result.fragment}></textarea>
        </label>
        <button type="button" onclick={copy}>{copied ? 'Скопировано' : 'Скопировать'}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.55);
    display: grid;
    place-items: center;
    z-index: 20;
  }
  .panel {
    background: var(--surface);
    border-radius: 10px;
    padding: 1rem;
    width: min(44rem, 94vw);
    max-height: 88vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
    font-size: 1rem;
  }
  label {
    display: flex;
    flex-direction: column;
    font-size: 0.8rem;
    color: var(--muted);
    gap: 0.15rem;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }
  textarea {
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    width: 100%;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--muted);
    margin: 0;
  }
  .warn {
    background: var(--warn-wash);
    color: var(--warn);
    padding: 0.5rem 0.6rem;
    border-radius: 6px;
    margin: 0;
  }
  .progress {
    font-size: 0.85rem;
    color: var(--muted);
    margin: 0;
  }
  .result {
    border-top: 1px solid var(--line);
    padding-top: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .result p {
    margin: 0;
    font-size: 0.85rem;
  }
  .warnings {
    margin: 0;
    padding-left: 1.1rem;
    font-size: 0.8rem;
    color: var(--warn);
  }
  code {
    font-size: 0.8rem;
  }
</style>
