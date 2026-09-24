# Contract: CLI additions

Additions to `newsbuilder`. Everything in `specs/001-news-builder-port/contracts/cli.md` stands.

---

## `newsbuilder server site`

```bash
newsbuilder server site set newsroom \
  --joomla-root /var/www/html \
  --site-url https://law.example.org/ \
  --category 8 \
  [--php /usr/bin/php8.2] [--state published|unpublished] [--featured|--no-featured] \
  [--access 1] [--language '*'] [--author 42] [--author-alias TEXT] [--meta-description TEXT]

newsbuilder server site check newsroom [--json]   # describe; writes nothing (FR-017)
newsbuilder server site disable newsroom          # site = None; nothing on the site changes
```

`set` on a server that already has a site target changes only the flags given. `--category` is
required the first time (INV-S1). `check` prints the Joomla version and the categories, access
levels, languages and authors with their ids — the values `set` and `publish` accept.

`server list --json` gains a `site` object per server (no secret exists to hide).

## `newsbuilder publish` — new flags

| Flag | Meaning |
|------|---------|
| `--no-article` | Upload photos only, as in 001, even when the server has a site target |
| `--category <ID>` | Override for this item |
| `--state <published\|unpublished>` | Override |
| `--featured` / `--no-featured` | Override |
| `--access <ID>` | Override |
| `--language <CODE>` | Override |
| `--author <ID>` | Override |
| `--author-alias <TEXT>` | Override |
| `--publish-up <RFC3339>` / `--publish-down <RFC3339>` | Override |
| `--meta-description <TEXT>` | Override |
| `--tag <ID>` (repeat) / `--no-tags` | Tags; also accepted by `server site set` |
| `--intro-image <first\|none\|N>` | The cover: first photo in the text (default), none, or photo number `N` |
| `--overwrite-article` | Confirms `edited_on_site` and `trashed` |
| `--create-article` | Confirms `gone` |

Setting flags given for a server without a site target are a usage error (exit 1), naming the
server.

## Exit codes

| Situation | Exit |
|-----------|------|
| Article created, updated or unchanged | 0 |
| Article needs confirmation | 3, stderr names the reason and the flag that confirms it |
| Article failed after photos uploaded | 5, photos still reported; fragment still printed or written |

## `--json`

`publish` gains `article` when the server has a site target:

```json
"article": {
  "outcome": "created",
  "id": 1234,
  "url": "https://law.example.org/index.php?option=com_content&view=article&id=1234&catid=8",
  "settings": { "category": 8, "state": "published", "featured": false }
}
```

`outcome` ∈ `created`, `updated`, `unchanged`, `would_create`, `would_update`,
`needs_confirmation` (with `reason` ∈ `gone`, `trashed`, `edited_on_site`), `failed` (with
`step`, `detail`). `ok` is `false` for `needs_confirmation` and `failed`.
