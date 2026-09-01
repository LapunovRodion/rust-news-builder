# Contract: rendered HTML fragment

The product's real deliverable. The CMS applies no styling of its own, so the fragment must
carry everything inline.

This contract is the strictest parity surface in the project: the golden fixtures of
constitution III compare against it byte for byte.

---

## Hard rules

1. **Inline styles only** (FR-023). No `<style>` element, no `<link>`, no `class` or `id`
   attribute used for styling, no `@media`, no `<script>`. A fragment containing any of these is
   a defect, and a test asserts their absence.
2. **Fragment, not document.** No `<!doctype>`, `<html>`, `<head>`, or `<body>`. The output is
   pasted into a CMS field.
3. **Deterministic** (FR-024). Attribute order, whitespace, and style-string content are fixed.
   The same item renders to the same bytes on every machine and every run.
4. **Escaped.** Text from the source document is HTML-escaped: `&`, `<`, `>`, `"`. Style strings
   come from configuration and are emitted as given.

---

## Structure

The fragment is a container holding the title, then the body blocks in order.

```html
<div style="{container}">
  <h1 style="{title}">Escaped Title</h1>
  <p style="{lead}">First paragraph.</p>
  <p style="{paragraph}">Later paragraph.</p>
  ...
</div>
```

The first paragraph uses the `lead` style, the rest `paragraph` — reproducing the reference.

### `FullWidth` — `[image:N]`

```html
<div style="{image_wrapper}"><img src="{url}" alt="" style="{image}" /></div>
```

### `Row` — `[images:N,M,...]`

```html
<div style="{row_wrapper}">
  <div style="{row_item}"><img src="{url}" alt="" style="{row_image}" /></div>
  <div style="{row_item}"><img src="{url}" alt="" style="{row_image}" /></div>
</div>
```

Row items are flex children of equal basis, so the row stays within the container width
regardless of how many photos it holds or how extreme their aspect ratios are (edge case).

### `FloatLeft` / `FloatRight` — `[image-left:N]`, `[image-right:N]`

```html
<img src="{url}" alt="" style="{float_left}" />
```

The float is emitted before the paragraphs that wrap it. A clearing element closes the wrap:

```html
<div style="{clear}"></div>
```

Clearing follows the reference's placement rules exactly; the goldens are the specification of
record.

---

## Image URLs

Published: `{public_base_url}/{slug}/{file_name}`, joined with exactly one slash between parts
regardless of trailing slashes in configuration (FR-027).

Preview (no `public_base_url`): a local placeholder URL. The HTML structure is otherwise
identical, so what the editor checks is what the CMS receives.

---

## Photo processing before rendering

Each published photo is, in order:

1. decoded, and EXIF orientation applied (FR-016);
2. rotated by the editor's quarter-turns, then cropped to `CropRect` if set (FR-012, FR-017);
3. scaled down so its longest edge is at most `max_width` — never scaled up;
4. re-encoded, stepping quality from `quality` down to `min_quality` until the result is at most
   `max_bytes`.

If the floor is reached and the photo is still over budget, the build emits
`SizeBudgetUnreachable` naming that photo, and the rest of the item still builds (FR-028,
FR-034).

The published file keeps its source extension, so URLs match the reference for the same input.

---

## Parity fixtures

`fixtures/reference/` holds, per input case, the fragment the Python tool produces. Coverage
required before implementation (constitution III):

| Case | Asserts |
|------|---------|
| Each of the four markers, alone | Per-layout element structure |
| Several markers in one document | Block ordering, clearing behaviour |
| Cyrillic title | Slug transliteration across the mapping table |
| Non-sequential photo names | Natural-sort ordering |
| Marker naming a missing photo | Warning text and continued build |
| Photo referenced by no marker | Unused-photo warning, photo not uploaded |
| Oversized photo | Quality-step search and final size |
