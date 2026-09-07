<!--
  T066 — the publishing targets.

  This screen never receives a secret back. It can *send* one to `set_credential`, which passes
  it straight to the OS store; everything it ever reads about a credential is whether one exists
  and what kind it is (FR-040, SC-010).
-->
<script lang="ts">
  import * as api from './api';
  import { run } from './store.svelte';
  import type { ServerView } from './types';

  const { onclose }: { onclose: () => void } = $props();

  let servers = $state<ServerView[]>([]);
  let storeAvailable = $state(true);
  let editing = $state<ServerView | null>(null);
  let secret = $state('');
  let note = $state('');

  async function refresh() {
    storeAvailable = (await run(() => api.secretStoreAvailable())) ?? false;
    servers = (await run(() => api.listServers())) ?? [];
  }

  $effect(() => {
    void refresh();
  });

  function blank(): ServerView {
    return {
      name: '',
      host: '',
      user: '',
      port: 22,
      remoteBasePath: '/var/www/html/news',
      publicBaseUrl: 'https://example.org/news/',
      auth: 'password',
      keyPath: null,
      hasCredential: false,
    };
  }

  async function save() {
    if (!editing) return;
    const config = editing;
    const done = await run(async () => {
      await api.saveServer(config);
      // The secret is a separate call so it never travels with the settings, and never lands
      // in the configuration file.
      if (secret) await api.setCredential(config.name, secret);
      return true;
    });
    if (done) {
      secret = '';
      editing = null;
      note = 'Сохранено';
      await refresh();
    }
  }

  async function remove(name: string) {
    await run(() => api.deleteServer(name));
    await refresh();
  }

  async function forgetCredential(name: string) {
    await run(() => api.deleteCredential(name));
    note = `Учётные данные для «${name}» удалены`;
    await refresh();
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Серверы">
  <div class="panel">
    <header>
      <h2>Серверы</h2>
      <button type="button" onclick={onclose}>Закрыть</button>
    </header>

    {#if !storeAvailable}
      <p class="warn">
        Хранилище секретов операционной системы недоступно, поэтому пароль сохранить некуда.
        Его можно будет ввести на время сеанса при публикации — на диск он не попадёт.
      </p>
    {/if}

    <ul>
      {#each servers as server (server.name)}
        <li>
          <div>
            <strong>{server.name}</strong>
            <p class="meta">
              {server.user}@{server.host}:{server.port} · {server.remoteBasePath}
            </p>
            <p class="meta">{server.publicBaseUrl}</p>
            <p class="meta">
              {server.auth === 'key' ? `ключ ${server.keyPath ?? ''}` : 'пароль'} ·
              {server.hasCredential ? 'учётные данные сохранены' : 'учётных данных нет'}
            </p>
          </div>
          <div class="actions">
            <button type="button" onclick={() => (editing = { ...server })}>Изменить</button>
            <button
              type="button"
              onclick={() => forgetCredential(server.name)}
              disabled={!server.hasCredential}>Забыть пароль</button
            >
            <button type="button" class="danger" onclick={() => remove(server.name)}>Удалить</button>
          </div>
        </li>
      {/each}
      {#if servers.length === 0}
        <li class="empty">Серверов пока нет.</li>
      {/if}
    </ul>

    {#if editing}
      {@const draft = editing}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label>Название <input bind:value={draft.name} required /></label>
        <label>Хост <input bind:value={draft.host} required /></label>
        <label>Пользователь <input bind:value={draft.user} required /></label>
        <label>Порт <input type="number" bind:value={draft.port} min="1" max="65535" /></label>
        <label>Каталог на сервере <input bind:value={draft.remoteBasePath} required /></label>
        <label>Публичный адрес <input bind:value={draft.publicBaseUrl} required /></label>
        <label>
          Аутентификация
          <select bind:value={draft.auth}>
            <option value="password">пароль</option>
            <option value="key">ключ</option>
          </select>
        </label>
        {#if draft.auth === 'key'}
          <label>Путь к ключу <input bind:value={draft.keyPath} /></label>
        {/if}
        <label>
          {draft.auth === 'key' ? 'Пароль ключа' : 'Пароль'}
          <input type="password" bind:value={secret} autocomplete="off" />
        </label>
        <p class="hint">
          Пароль отправляется прямо в хранилище секретов операционной системы и не попадает ни в
          файл настроек, ни в журнал.
        </p>
        <div class="actions">
          <button type="button" onclick={() => ((editing = null), (secret = ''))}>Отмена</button>
          <button type="submit" class="primary">Сохранить</button>
        </div>
      </form>
    {:else}
      <button type="button" onclick={() => (editing = blank())}>Добавить сервер</button>
    {/if}

    {#if note}<p class="note">{note}</p>{/if}
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
    width: min(42rem, 94vw);
    max-height: 88vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  li {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.6rem;
  }
  li.empty {
    color: var(--muted);
    justify-content: center;
  }
  .meta {
    margin: 0.1rem 0 0;
    font-size: 0.78rem;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 0.35rem;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    border-top: 1px solid var(--line);
    padding-top: 0.75rem;
  }
  label {
    display: flex;
    flex-direction: column;
    font-size: 0.8rem;
    color: var(--muted);
    gap: 0.15rem;
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
    font-size: 0.85rem;
  }
  .note {
    font-size: 0.8rem;
    color: var(--muted);
    margin: 0;
  }
  .danger {
    color: var(--danger);
  }
</style>
