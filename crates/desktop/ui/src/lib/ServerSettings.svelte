<!--
  T066 — the publishing targets.

  This screen never receives a secret back. It can *send* one to `set_credential`, which passes
  it straight to the OS store; everything it ever reads about a credential is whether one exists
  and what kind it is (FR-040, SC-010).
-->
<script lang="ts">
  import * as api from './api';
  import ArticleSettingsForm from './ArticleSettingsForm.svelte';
  import { run } from './store.svelte';
  import { emptyArticleSettings, type ServerView, type SiteCatalogView } from './types';

  const { onclose }: { onclose: () => void } = $props();

  let servers = $state<ServerView[]>([]);
  let storeAvailable = $state(true);
  let editing = $state<ServerView | null>(null);
  let secret = $state('');
  let note = $state('');
  /** What "Проверить подключение" read from the site; fills the selects (002 FR-011). */
  let catalog = $state<SiteCatalogView | null>(null);
  let siteNote = $state('');

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
      site: null,
    };
  }

  function edit(server: ServerView) {
    editing = { ...server, site: server.site ? { ...server.site, defaults: { ...server.site.defaults } } : null };
    catalog = null;
    siteNote = '';
  }

  function toggleSite(on: boolean) {
    if (!editing) return;
    editing.site = on
      ? {
          joomlaRoot: '/var/www/html',
          siteUrl: '',
          php: 'php',
          defaults: { ...emptyArticleSettings(), state: 'published' },
        }
      : null;
  }

  /** Saves without closing the form. */
  async function persist(): Promise<boolean> {
    if (!editing) return false;
    const config = editing;
    const done = await run(async () => {
      await api.saveServer(config);
      // The secret is a separate call so it never travels with the settings, and never lands
      // in the configuration file.
      if (secret) await api.setCredential(config.name, secret);
      return true;
    });
    if (done) secret = '';
    return done === true;
  }

  async function save() {
    if (await persist()) {
      editing = null;
      note = 'Сохранено';
      await refresh();
    }
  }

  /** Saves, then reads the site's categories and the rest, writing nothing to it (FR-017). */
  async function checkConnection() {
    if (!editing || !(await persist())) return;
    siteNote = 'Подключение…';
    const found = await run(() => api.checkSite(editing!.name));
    catalog = found ?? null;
    siteNote = found
      ? `Joomla ${found.joomlaVersion}: ${found.categories.length} категорий`
      : 'Не удалось подключиться — подробности выше';
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
            {#if server.site}
              <p class="meta">
                статьи → {server.site.siteUrl}
                {server.site.defaults.category ? '' : '· категория не выбрана'}
              </p>
            {/if}
          </div>
          <div class="actions">
            <button type="button" onclick={() => edit(server)}>Изменить</button>
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

        <fieldset>
          <legend>
            <label class="check">
              <input
                type="checkbox"
                checked={draft.site !== null}
                onchange={(e) => toggleSite(e.currentTarget.checked)}
              />
              Сайт Joomla: вставлять статью автоматически
            </label>
          </legend>
          {#if draft.site}
            {@const site = draft.site}
            <label>Каталог Joomla на сервере <input bind:value={site.joomlaRoot} required /></label>
            <label>Адрес сайта <input bind:value={site.siteUrl} placeholder="https://example.org/" required /></label>
            <label>Команда PHP <input bind:value={site.php} /></label>
            <div class="actions">
              <button type="button" onclick={checkConnection}>Проверить подключение</button>
              {#if siteNote}<span class="hint">{siteNote}</span>{/if}
            </div>
            <p class="hint">Настройки статьи по умолчанию. Категорию нужно выбрать до публикации.</p>
            <ArticleSettingsForm bind:settings={site.defaults} {catalog} unsetLabel="как в Joomla" />
          {:else}
            <p class="hint">Выключено: публикуются только фото, фрагмент вставляется вручную.</p>
          {/if}
        </fieldset>
        <div class="actions">
          <button type="button" onclick={() => ((editing = null), (secret = ''))}>Отмена</button>
          <button type="submit" class="primary">Сохранить</button>
        </div>
      </form>
    {:else}
      <button type="button" onclick={() => edit(blank())}>Добавить сервер</button>
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
  fieldset {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.5rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  legend {
    padding: 0 0.3rem;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
    color: inherit;
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
