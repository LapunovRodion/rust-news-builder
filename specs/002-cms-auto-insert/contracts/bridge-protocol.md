# Contract: the Joomla bridge protocol

Between `newsbuilder-core`'s SSH site adapter and the PHP bridge it runs on the server
(research R1, R2). Golden examples of every message live in `fixtures/bridge/` and are
deserialised by `core` tests; changing this contract is a reviewed diff there.

---

## Invocation

One exec channel per operation, on the SSH session already open for the photos:

```sh
cd -- '<joomla_root>' && '<php>' -r '$n=(int)fgets(STDIN);eval("?>".stream_get_contents(STDIN,$n));'
```

`<joomla_root>` and `<php>` are POSIX single-quoted. Nothing else in the command varies.

stdin: `<byte length of bridge source>\n<bridge source><request JSON>`, then EOF.

stdout: arbitrary lines, of which exactly one starts with `NEWSBUILDER-BRIDGE:` followed by the
response JSON. No such line, or a non-zero exit with no such line, is a `bridge_failed` error
carrying the exit status and the last 2 KB of stderr.

All requests carry `"protocol": 1`. A bridge that does not speak the requested protocol answers
`unsupported_protocol`.

---

## `describe`

Writes nothing.

```json
{ "protocol": 1, "op": "describe" }
```

```json
{
  "ok": true,
  "joomla_version": "5.2.3",
  "categories": [{ "id": 8, "title": "Новости", "level": 1, "published": true, "language": "*" }],
  "access_levels": [{ "id": 1, "title": "Public" }],
  "languages": [{ "code": "*", "title": "All" }, { "code": "ru-RU", "title": "Русский" }],
  "authors": [{ "id": 42, "name": "Пресс-служба" }]
}
```

## `find`

Writes nothing.

```json
{
  "protocol": 1,
  "op": "find",
  "site_url": "https://law.example.org/",
  "alias": "den-konstitutsii",
  "category": 8,
  "title": "День Конституции",
  "articletext": "<hr id=\"system-readmore\"/>…",
  "settings": { "category": 8, "state": "published", "featured": false }
}
```

```json
{
  "ok": true,
  "articles": [
    {
      "id": 1234,
      "category": 8,
      "trashed": false,
      "content_matches_mark": true,
      "identical": false,
      "url": "https://law.example.org/index.php?option=com_content&view=article&id=1234&catid=8"
    }
  ],
  "alias_taken_by": null
}
```

- `articles`: every article with this alias in **any** category whose `note` starts with
  `newsbuilder:`.
- `alias_taken_by`: the id of an article *without* the mark holding this alias in `category`,
  else `null`.
- `identical` compares title, `articletext` as Joomla would split it, and every key present in
  `settings`.

## `save`

```json
{
  "protocol": 1,
  "op": "save",
  "site_url": "https://law.example.org/",
  "id": null,
  "restore": false,
  "alias": "den-konstitutsii",
  "title": "День Конституции",
  "articletext": "<hr id=\"system-readmore\"/>…",
  "settings": {
    "category": 8,
    "state": "published",
    "featured": false,
    "access": 1,
    "language": "*",
    "author": 42,
    "author_alias": "Пресс-служба",
    "publish_up": "2026-09-17T10:00:00+03:00",
    "meta_description": "…"
  }
}
```

Keys absent from `settings` are not sent to Joomla, which applies its own default (FR-012);
`core` never sends `null`. `state` is `"published"` (1) or `"unpublished"` (0) — there is no other
value. The bridge sets `note` to `newsbuilder:` + sha256 of `title` + `\n` + `introtext` + `\0` +
`fulltext` (the text split the way com_content splits it, so the mark does not depend on how the
readmore tag was spelled), and a restored article gets the requested `state`.

`site_url` is only used to build the article address the answer carries.

`settings.tags` (optional): existing tag ids; `[]` clears. Written through the model's tag
handling (`data['tags']`), so Joomla's tag map and counts stay right; `find` compares the
article's current tags. `describe` also answers `tags: [{ id, title, level }]` (published, root
excluded, tree order).

`intro_image` (on `find` and `save`, optional): the cover's site-relative path or full address,
`""` for none. Absent means "leave the cover alone". `save` merges it into the article's `images`
JSON (`image_intro`, `image_intro_alt` = title) and keeps every other key; `find` counts a
different cover as not `identical`.

```json
{ "ok": true, "id": 1234, "created": true, "url": "https://law.example.org/index.php?option=com_content&view=article&id=1234&catid=8" }
```

---

## Errors

```json
{ "ok": false, "error": { "kind": "category_missing", "detail": "category 8 does not exist" } }
```

| `kind` | When | `core` maps to |
|--------|------|----------------|
| `unsupported_protocol` | request `protocol` unknown | `Error::SiteBridge` |
| `unsupported_joomla` | major version not 4, 5 or 6; carries `found` | `Error::SiteUnsupported { found }` |
| `boot_failed` | Joomla did not start (bad root, unreadable configuration) | `Error::SiteBridge { step: "starting Joomla" }` |
| `category_missing` | `settings.category` absent on the site; carries `id` | `Error::CategoryMissing { id }` |
| `save_rejected` | the model's `save()` returned false; `detail` is `getError()` | `Error::SiteBridge { step: "saving the article" }` |
| `not_found` | `id` given but the article vanished between `find` and `save` | `Error::SiteBridge { step: "saving the article" }` — a race; the next publish sees `gone` |
| `fatal`, `exception` | PHP or Joomla failed; `detail` is the message and location | `Error::SiteBridge` |

`detail` never includes anything read from `configuration.php`.

## Forbidden

The bridge contains no operation that deletes, trashes, archives or checks in/out an article, and
no operation keyed on anything but the request above. `core`'s static test (research R9) enforces
the absence of deleting calls in the source.
