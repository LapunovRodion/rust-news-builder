# Contract: appearance configuration

One appearance ships built in (FR-025). Configuration overrides it, and the schema is the
reference tool's, so the existing `style-presets/*.json` files load unmodified — which is what
the constitution's Technology and Output Constraints require, even though the product no longer
offers a menu of presets (FR-035).

---

## Schema

```json
{
  "image": {
    "max_width": 1600,
    "max_bytes": 512000,
    "jpeg_quality": 85,
    "jpeg_min_quality": 50,
    "webp_quality": 85,
    "webp_min_quality": 50
  },
  "styles": {
    "container": "max-width: 920px; margin: 0 auto; ...",
    "title": "...",
    "paragraph": "...",
    "lead": "...",
    "image_wrapper": "...",
    "image": "...",
    "row_wrapper": "...",
    "row_item": "...",
    "row_image": "...",
    "float_left": "...",
    "float_right": "...",
    "clear": "..."
  }
}
```

Both top-level keys are optional, and so is every key within them. Anything absent falls back to
the built-in value, so a config may override a single style string.

---

## Style slots

Each `styles` key is the inline `style` attribute for one slot in
[html-output.md](./html-output.md). The names match the reference's `DEFAULT_STYLES` exactly —
that identity is what makes preset files portable.

| Slot | Applied to |
|------|-----------|
| `container` | The wrapping `div` |
| `title` | The `h1` |
| `lead` / `paragraph` | First paragraph / the rest |
| `image_wrapper` / `image` | Full-width placement |
| `row_wrapper` / `row_item` / `row_image` | Row placement |
| `float_left` / `float_right` | Floated placements |
| `clear` | The clearing element after a float |

Values are emitted verbatim, not parsed or normalised. Determinism (FR-024) means the same
config always yields the same bytes.

---

## Validation

| Rule | On violation |
|------|--------------|
| `min_quality <= quality`, both in 1..=100 (INV-7) | Reject the file, naming the field |
| `max_width >= 1`, `max_bytes >= 1024` | Reject the file, naming the field |
| Unknown key under `styles` or `image` | Warn, ignore, continue |
| Style string containing `</` or `<script` | Reject — it would break out of the attribute |
| Malformed JSON | Reject with line and column |

Unknown keys warn rather than fail so that preset files written for a future or older version
still load.

---

## Resolution order

1. The built-in appearance.
2. A user appearance file in the OS config directory, if present.
3. `--appearance <PATH>` (CLI) or the file chosen in the desktop application, if given.

Later layers override earlier ones key by key, not wholesale: overriding `image.max_width`
leaves every style string at its built-in value.

---

## Built-in appearance

The single shipped appearance is the reference's `DEFAULT_STYLES` — the warm editorial look
(serif body, soft gradient container, rounded images with a shadow). Keeping it as the built-in
means the parity fixtures compare like with like, and editors see no change in what they
publish.
