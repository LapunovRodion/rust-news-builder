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
  import ArticleSettingsForm from './ArticleSettingsForm.svelte';
  import { run, ui } from './store.svelte';
  import {
    emptyArticleSettings,
    type ArticleConfirmation,
    type ArticleSettingsView,
    type ConfirmationReason,
    type PublicationView,
    type PublishProgress,
    type ServerView,
    type SiteCatalogView,
  } from './types';

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

  /** This item's article overrides (002 US3); unset fields take the server's defaults. */
  let articleSettings = $state<ArticleSettingsView>(emptyArticleSettings());
  let showArticleSettings = $state(false);
  let catalogs = $state<Record<string, SiteCatalogView>>({});
  let urlCopied = $state(false);

  const questions: Record<ConfirmationReason, { text: string; answer: ArticleConfirmation; action: string }> = {
    edited_on_site: {
      text: 'Статью изменили на сайте после прошлой публикации. Перезаписать её?',
      answer: 'overwrite',
      action: 'Перезаписать',
    },
    trashed: {
      text: 'Статья лежит в корзине Joomla. Восстановить и обновить её?',
      answer: 'overwrite',
      action: 'Восстановить и обновить',
    },
    gone: {
      text: 'Эту новость уже публиковали, но статьи на сайте больше нет. Создать её заново?',
      answer: 'create_new',
      action: 'Создать заново',
    },
  };

  /** The cover: `"first"`, `"none"`, or a photo id (Joomla's Intro Image). */
  let introImage = $state('first');
  /** Only placed photos are published, so only they can be the cover. */
  const coverPhotos = $derived(ui.item.photos.filter((photo) => photo.used));

  $effect(() => {
    void (async () => {
      const saved = await run(() => api.getArticleSettings());
      if (saved) articleSettings = saved;
      const cover = await run(() => api.getIntroImage());
      if (cover !== undefined) introImage = cover;
    })();
  });

  async function chooseCover(choice: string) {
    const chosen = await run(() => api.setIntroImage(choice));
    if (chosen !== undefined) introImage = chosen;
  }

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
  const catalog = $derived(catalogs[chosen] ?? null);

  async function openArticleSettings() {
    showArticleSettings = !showArticleSettings;
    if (showArticleSettings && selected?.site && !catalogs[chosen]) {
      const found = await run(() => api.checkSite(chosen));
      if (found) catalogs = { ...catalogs, [chosen]: found };
    }
  }

  function resetArticleSettings() {
    articleSettings = emptyArticleSettings();
  }

  async function copyUrl(url: string) {
    await writeText(url);
    urlCopied = true;
  }
  /** FR-041: with no store and no stored credential, one is asked for and kept for the session. */
  const needsSessionCredential = $derived(
    selected !== undefined && !storeAvailable && !selected.hasCredential,
  );

  async function go(confirmation: ArticleConfirmation = 'none') {
    result = null;
    copied = false;
    urlCopied = false;
    progress = null;
    if (needsSessionCredential) {
      if (!sessionSecret) return;
      const stored = await run(() => api.setSessionCredential(chosen, sessionSecret));
      if (stored === undefined) return;
      // Out of the component's memory as soon as the Rust side has it.
      sessionSecret = '';
    }
    if (selected?.site) {
      const stored = await run(() => api.setArticleSettings(articleSettings));
      if (stored === undefined) return;
      articleSettings = stored;
    }
    const publication = await run(() => api.publish(chosen, dryRun, confirmation));
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

      {#if selected?.site}
        <div class="article-settings">
          <button type="button" onclick={openArticleSettings}>
            {showArticleSettings ? '▾' : '▸'} Параметры статьи
          </button>
          {#if showArticleSettings}
            <p class="hint">
              Пустые поля берутся из настроек сервера «{selected.name}».
              {#if !selected.site.defaults.category && !articleSettings.category}
                <strong>Категория не выбрана ни здесь, ни на сервере.</strong>
              {/if}
            </p>
            <div class="cover">
              <span class="cover-label">Заставка (изображение анонса)</span>
              <div class="cover-choices">
                <button
                  type="button"
                  class="cover-choice text"
                  class:chosen={introImage === 'first'}
                  onclick={() => chooseCover('first')}
                >
                  Первое фото в тексте
                </button>
                {#each coverPhotos as photo (photo.id)}
                  <button
                    type="button"
                    class="cover-choice"
                    class:chosen={introImage === photo.id}
                    title={photo.fileName}
                    aria-label={`Заставка: ${photo.fileName}`}
                    onclick={() => chooseCover(photo.id)}
                  >
                    <img src={photo.thumbUrl} alt="" />
                  </button>
                {/each}
                <button
                  type="button"
                  class="cover-choice text"
                  class:chosen={introImage === 'none'}
                  onclick={() => chooseCover('none')}
                >
                  Без заставки
                </button>
              </div>
            </div>
            <ArticleSettingsForm bind:settings={articleSettings} {catalog} unsetLabel="как на сервере" />
            <button type="button" onclick={resetArticleSettings}>Сбросить к настройкам сервера</button>
          {/if}
        </div>
      {/if}

      <button type="button" class="primary" onclick={() => go()} disabled={ui.busy || !chosen}>
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
        {#if result.article}
          {@const article = result.article}
          <div class="article" class:failed={article.outcome === 'failed'}>
            {#if article.outcome === 'created' || article.outcome === 'updated' || article.outcome === 'unchanged'}
              <p>
                {article.outcome === 'created'
                  ? 'Статья создана на сайте'
                  : article.outcome === 'updated'
                    ? 'Статья на сайте обновлена'
                    : 'Статья на сайте уже такая — без изменений'}
                (№ {article.id}).
              </p>
              <p><code>{article.url}</code></p>
              <button type="button" onclick={() => copyUrl(article.url)}>
                {urlCopied ? 'Адрес скопирован' : 'Скопировать адрес статьи'}
              </button>
            {:else if article.outcome === 'would_create'}
              <p>Статья была бы создана в категории {article.settings.category}.</p>
            {:else if article.outcome === 'would_update'}
              <p>Статья № {article.id} была бы обновлена.</p>
            {:else if article.outcome === 'needs_confirmation'}
              {@const question = questions[article.reason]}
              <p>{question.text}{article.id ? ` (статья № ${article.id})` : ''}</p>
              <div class="actions">
                <button type="button" class="primary" onclick={() => go(question.answer)} disabled={ui.busy}>
                  {question.action}
                </button>
                <button type="button" onclick={() => (result = null)}>Отмена</button>
              </div>
            {:else if article.outcome === 'failed'}
              <p>
                Фото загружены, статья не записана: {article.step} — {article.detail}. Фрагмент ниже
                можно вставить вручную.
              </p>
            {/if}
          </div>
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
    word-break: break-all;
  }
  .cover {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    width: 100%;
  }
  .cover-label {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .cover-choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    align-items: stretch;
  }
  .cover-choice {
    padding: 0;
    border: 2px solid transparent;
    border-radius: 6px;
    overflow: hidden;
    width: 4.5rem;
    height: 3.2rem;
    background: var(--line);
  }
  .cover-choice img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cover-choice.text {
    width: auto;
    padding: 0 0.6rem;
    font-size: 0.75rem;
  }
  .cover-choice.chosen {
    border-color: var(--accent, #2f6fed);
  }
  .article-settings {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    align-items: flex-start;
  }
  .article {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.5rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    align-items: flex-start;
  }
  .article.failed {
    background: var(--warn-wash);
    color: var(--warn);
  }
  .actions {
    display: flex;
    gap: 0.35rem;
  }
</style>
