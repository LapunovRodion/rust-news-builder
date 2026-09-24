# Data Model: Insert the News Item into the Site Automatically

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Additions only. Everything in `specs/001-news-builder-port/data-model.md` stands unchanged.

---

## SiteTarget

Part of `ServerConfig`, as `site: Option<SiteTarget>`. `None` — including every configuration
file written before this feature (`#[serde(default)]`) — means insertion is off and publish
behaves exactly as in 001 (FR-013).

| Field | Type | Rule |
|-------|------|------|
| `joomla_root` | `String` | Absolute POSIX path of the Joomla install on the SSH host. Non-empty, starts with `/` |
| `site_url` | `Url` | The site's public root, `http`/`https`. Article addresses are built beneath it |
| `php` | `String` | The PHP CLI command. Default `php`. Non-empty, no newline |
| `defaults` | `ArticleSettings` | Server-level defaults. `category` MUST be set (INV-S1) |

Holds no secret and no secret reference: the bridge uses the site's own database settings
(research R1). `ServerConfig` still has no field a secret could be written into.

## ArticleSettings

Used twice with the same shape: `SiteTarget.defaults` and `NewsItem.article` (the per-item
overrides, default all-`None`). Every field optional (research R7).

| Field | Type | Joomla column | Rule |
|-------|------|---------------|------|
| `category` | `Option<u32>` | `catid` | Must exist on the site at publish (checked by the bridge) |
| `state` | `Option<ArticleState>` | `state` | `Published` (1) or `Unpublished` (0). Server default when never set: `Published` |
| `featured` | `Option<bool>` | `featured` | |
| `access` | `Option<u32>` | `access` | A view level id |
| `language` | `Option<String>` | `language` | `*` or a content language code such as `ru-RU` |
| `author` | `Option<u32>` | `created_by` | A user id |
| `author_alias` | `Option<String>` | `created_by_alias` | ≤ 255 chars |
| `publish_up` | `Option<OffsetDateTime>` | `publish_up` | |
| `publish_down` | `Option<OffsetDateTime>` | `publish_down` | Later than `publish_up` when both set (INV-S2) |
| `meta_description` | `Option<String>` | `metadesc` | ≤ 300 chars, no newline |
| `tags` | `Option<Vec<u32>>` | tag map, via the model | Existing tag ids. `Some([])` clears. An item's list replaces the server's. Sent sorted, once each |

`ArticleState` has no `Trashed` or `Archived` variant: the application cannot put an article in
the trash (FR-008).

**Resolution** — `ArticleSettings::resolve(overrides, defaults)`: field by field, override wins,
then default; a field `None` in both is omitted from the bridge request so Joomla applies its own
default (FR-012).

## IntroImage

On `NewsItem` as `intro_image` — per item only, since a server has no photo to default to.

| Variant | Meaning |
|---------|---------|
| `First` (default) | The first photo in the text |
| `Photo(PhotoId)` | That photo; it must be placed (refused before upload otherwise) |
| `None` | No cover; an existing one is cleared |

Resolved by `core` to the photo's public URL, made site-relative (`images/…`) when it lies under
`site_url`, and carried as `intro_image: Option<String>` on `ArticleProbe` and `ArticleWrite`
(`Some("")` clears, `None` leaves the cover alone). The bridge merges it into the article's
`images` JSON as `image_intro` + `image_intro_alt` (the title), keeping every other key.

## ArticleWrite

What `core` asks the bridge to save. Built only by `core`.

| Field | Source |
|-------|--------|
| `alias` | The folder slug the photos went into (`Publication.folder`) |
| `title` | `NewsItem.title` |
| `articletext` | `Publication.fragment`, byte for byte (FR-003) |
| `settings` | Resolved `ArticleSettings` |
| `id` | `Some` to update a found article, `None` to create |
| `restore` | `true` only when confirming a trashed article |

## SiteArticle

What `find` reports about an article already on the site.

| Field | Meaning |
|-------|---------|
| `id` | Article id |
| `category` | Current `catid` |
| `trashed` | `state == -2` |
| `content_matches_mark` | The hash in `note` equals the hash of the article's current title + text — false means it was edited on the site |
| `identical` | Current title, text and every requested setting already equal the write |
| `url` | Article address |

## ArticleOutcome

Carried on `Publication` as `article: Option<ArticleOutcome>` (`None` when the server has no site
target).

| Variant | Carries | Frontend behaviour |
|---------|---------|--------------------|
| `Created` | `id`, `url` | Show link (FR-015) |
| `Updated` | `id`, `url` | Show link |
| `Unchanged` | `id`, `url` | Show link, "без изменений" |
| `WouldCreate` / `WouldUpdate` | resolved settings, `id` for update | Dry run report (FR-009) |
| `NeedsConfirmation` | `reason: Gone \| Trashed \| EditedOnSite`, `id` when known | Ask; re-run publish with the matching `ArticleConfirmation` |
| `Failed` | `step`, `detail` | Name the step, keep offering the fragment (FR-015, US1 sc5) |

State transitions for one item on one server:

```text
            publish                 publish (changed)
 (absent) ──────────► Created ───────────────────────► Updated
                        │  publish (same)                 │
                        └──────────► Unchanged ◄──────────┘
 edited in CMS ─► NeedsConfirmation(EditedOnSite) ─confirm─► Updated
 deleted in CMS ─► NeedsConfirmation(Gone) ─confirm─► Created
 trashed in CMS ─► NeedsConfirmation(Trashed) ─confirm─► Updated (restored)
```

## ArticleConfirmation

Input to publish: `None` (default), `Overwrite` (accepts `EditedOnSite` and `Trashed`),
`CreateNew` (accepts `Gone`). A confirmation that does not match the situation found is ignored
and the outcome is `NeedsConfirmation` again — confirming one thing never authorises another.

## SiteCatalog

Answer of `describe` (research R8). Not persisted; the desktop keeps the last one per server for
the session.

| Field | Type |
|-------|------|
| `joomla_version` | `String` |
| `categories` | `[{ id, title, level, published, language }]` for `com_content`, in tree order |
| `access_levels` | `[{ id, title }]` |
| `languages` | `[{ code, title }]`, always including `*` |
| `authors` | `[{ id, name }]` |

## Invariants

- **INV-S1**: A `SiteTarget` without `defaults.category` is refused at publish before any photo
  is processed. It *can* be saved without one, because the category is chosen from the list
  "check connection" reads, and that needs a saved server; everything else about the target is
  checked at save time.
- **INV-S2**: `publish_down > publish_up` whenever both resolve.
- **INV-S3**: No request `core` can build asks the bridge to delete or trash an article.
- **INV-S4**: `ArticleWrite.articletext` equals `Publication.fragment`.
- **INV-S5**: With `site == None`, `Publication` is field-for-field what feature 001 produced,
  with `article == None`.
