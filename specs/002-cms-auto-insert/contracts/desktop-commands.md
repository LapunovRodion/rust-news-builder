# Contract: desktop IPC additions

Additions to `specs/001-news-builder-port/contracts/desktop-commands.md`. The frontend still
holds no domain rule: resolution of settings, the re-publish matrix and every validation happen
in `core`.

---

## Types

```typescript
type ArticleSettingsView = {
  category: number | null;
  state: 'published' | 'unpublished' | null;
  featured: boolean | null;
  access: number | null;
  language: string | null;
  author: number | null;
  authorAlias: string | null;
  publishUp: string | null;     // RFC 3339
  publishDown: string | null;
  metaDescription: string | null;
};

type SiteTargetView = {
  joomlaRoot: string;
  siteUrl: string;
  php: string;
  defaults: ArticleSettingsView;
};

type ServerView = /* 001 fields */ & { site: SiteTargetView | null };

type SiteCatalogView = {
  joomlaVersion: string;
  categories: { id: number; title: string; level: number; published: boolean }[];
  accessLevels: { id: number; title: string }[];
  languages: { code: string; title: string }[];
  authors: { id: number; title: string }[];
};

type ArticleOutcomeView =
  | { outcome: 'created' | 'updated' | 'unchanged'; id: number; url: string }
  | { outcome: 'would_create' | 'would_update'; id: number | null; settings: ArticleSettingsView }
  | { outcome: 'needs_confirmation'; reason: 'gone' | 'trashed' | 'edited_on_site'; id: number | null }
  | { outcome: 'failed'; step: string; detail: string };

type PublicationView = /* 001 fields */ & { article: ArticleOutcomeView | null };
```

## Commands

| Command | Signature | Notes |
|---------|-----------|-------|
| `save_server` | `(config: ServerView) → ()` | Unchanged name; now carries `site`. INV-S1 enforced in core |
| `check_site` | `(server: string) → SiteCatalogView` | "Проверить подключение". Writes nothing. The UI keeps the answer per server while the dialog is open; nothing is cached in Rust |
| `get_article_settings` | `() → ArticleSettingsView` | The open item's overrides |
| `set_article_settings` | `(settings: ArticleSettingsView) → ArticleSettingsView` | Replaces the open item's overrides; returns them validated |
| `get_intro_image` / `set_intro_image` | `() → string` / `(choice: string) → string` | The cover: `"first"`, `"none"`, or a photo id |
| `publish` | `(server: string, dryRun: boolean, confirmation: 'none' \| 'overwrite' \| 'create_new') → PublicationView` | `confirmation` is new, default `'none'` |

## Screens

- **Серверы** (`ServerSettings.svelte`): a "Сайт Joomla" section per server — enable toggle,
  install directory, site address, PHP command, "Проверить подключение", then the defaults form
  whose selects are filled from `SiteCatalogView`. Saving without a category is blocked with the
  message core returns.
- **Публикация** (`PublishDialog.svelte`): a "Параметры статьи" section pre-filled with the
  resolved values, editable per item. After publishing: the article link and id, or the named
  failure plus the fragment for copying, or a confirmation question whose answer re-runs
  `publish` with the matching `confirmation`.
