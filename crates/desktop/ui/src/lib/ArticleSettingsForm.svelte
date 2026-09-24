<!--
  002 US3 — the options an editor used to set by hand in Joomla's administrator.

  Used twice: a server's defaults (ServerSettings) and one item's overrides (PublishDialog).
  Every field may be left unset; `unsetLabel` says what unset means in that place. Nothing is
  decided here — the Rust core resolves and validates.
-->
<script lang="ts">
  import { fromLocalInput, toLocalInput } from './dates';
  import type { ArticleSettingsView, SiteCatalogView } from './types';

  let {
    settings = $bindable(),
    catalog,
    unsetLabel,
  }: {
    settings: ArticleSettingsView;
    catalog: SiteCatalogView | null;
    unsetLabel: string;
  } = $props();

  const indent = (level: number) => '\u00a0\u00a0'.repeat(Math.max(0, level - 1));

  /** Adding or removing one tag starts from what is chosen here; unset starts empty. */
  function toggleTag(id: number) {
    const current = settings.tags ?? [];
    settings.tags = current.includes(id) ? current.filter((t) => t !== id) : [...current, id];
  }
</script>

<div class="grid">
  <label>
    Категория
    {#if catalog}
      <select bind:value={settings.category}>
        <option value={null}>{unsetLabel}</option>
        {#each catalog.categories as category (category.id)}
          <option value={category.id}>
            {indent(category.level)}{category.title}{category.published ? '' : ' (не опубликована)'}
          </option>
        {/each}
      </select>
    {:else}
      <input
        type="number"
        min="1"
        placeholder="ID категории"
        value={settings.category ?? ''}
        oninput={(e) => {
          const v = e.currentTarget.valueAsNumber;
          settings.category = Number.isNaN(v) ? null : v;
        }}
      />
    {/if}
  </label>

  <label>
    Состояние
    <select bind:value={settings.state}>
      <option value={null}>{unsetLabel}</option>
      <option value="published">опубликовано</option>
      <option value="unpublished">не опубликовано</option>
    </select>
  </label>

  <label>
    Избранное
    <select bind:value={settings.featured}>
      <option value={null}>{unsetLabel}</option>
      <option value={true}>да</option>
      <option value={false}>нет</option>
    </select>
  </label>

  <label>
    Доступ
    <select bind:value={settings.access} disabled={!catalog}>
      <option value={null}>{unsetLabel}</option>
      {#each catalog?.accessLevels ?? [] as level (level.id)}
        <option value={level.id}>{level.title}</option>
      {/each}
    </select>
  </label>

  <label>
    Язык
    <select bind:value={settings.language} disabled={!catalog}>
      <option value={null}>{unsetLabel}</option>
      {#each catalog?.languages ?? [] as language (language.code)}
        <option value={language.code}>{language.title}</option>
      {/each}
    </select>
  </label>

  <label>
    Автор
    <select bind:value={settings.author} disabled={!catalog}>
      <option value={null}>{unsetLabel}</option>
      {#each catalog?.authors ?? [] as author (author.id)}
        <option value={author.id}>{author.title}</option>
      {/each}
    </select>
  </label>

  <label>
    Автор (псевдоним)
    <input
      value={settings.authorAlias ?? ''}
      oninput={(e) => (settings.authorAlias = e.currentTarget.value || null)}
    />
  </label>

  <label>
    Начало публикации
    <input
      type="datetime-local"
      value={toLocalInput(settings.publishUp)}
      oninput={(e) => (settings.publishUp = fromLocalInput(e.currentTarget.value))}
    />
  </label>

  <label>
    Окончание публикации
    <input
      type="datetime-local"
      value={toLocalInput(settings.publishDown)}
      oninput={(e) => (settings.publishDown = fromLocalInput(e.currentTarget.value))}
    />
  </label>

  <div class="wide tags">
    <span class="caption">Метки</span>
    {#if catalog}
      <div class="chips">
        {#each catalog.tags as tag (tag.id)}
          <button
            type="button"
            class="chip"
            class:on={settings.tags?.includes(tag.id)}
            aria-pressed={settings.tags?.includes(tag.id) ?? false}
            onclick={() => toggleTag(tag.id)}
          >
            {tag.title}
          </button>
        {:else}
          <span class="caption">На сайте нет меток.</span>
        {/each}
      </div>
      <div class="chips">
        <span class="caption">
          {settings.tags === null
            ? unsetLabel
            : settings.tags.length === 0
              ? 'без меток'
              : `выбрано: ${settings.tags.length}`}
        </span>
        {#if settings.tags !== null}
          <button type="button" class="link" onclick={() => (settings.tags = null)}>{unsetLabel}</button>
        {/if}
        {#if settings.tags === null || settings.tags.length > 0}
          <button type="button" class="link" onclick={() => (settings.tags = [])}>без меток</button>
        {/if}
      </div>
    {:else}
      <span class="caption">Список меток появится после проверки подключения.</span>
    {/if}
  </div>

  <label class="wide">
    Мета-описание
    <textarea
      rows="2"
      maxlength="300"
      value={settings.metaDescription ?? ''}
      oninput={(e) => (settings.metaDescription = e.currentTarget.value || null)}
    ></textarea>
  </label>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 0.4rem 0.75rem;
  }
  label {
    display: flex;
    flex-direction: column;
    font-size: 0.8rem;
    color: var(--muted);
    gap: 0.15rem;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .tags {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .caption {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    align-items: center;
  }
  .chip {
    border-radius: 999px;
    padding: 0.1rem 0.6rem;
    font-size: 0.78rem;
  }
  .chip.on {
    background: var(--accent);
    color: white;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    font-size: 0.75rem;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
