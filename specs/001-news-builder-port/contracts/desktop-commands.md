# Contract: desktop IPC surface

The Tauri command boundary between the Rust core and the TypeScript UI.

The governing rule comes from constitution principle II: **the frontend holds no domain rule.**
Every command below is a thin call into `newsbuilder-core`. If a behaviour cannot be expressed
as one of these commands, it does not belong in the UI either.

---

## Session state

The Rust side owns the open `NewsItem` in Tauri managed state. The frontend holds a *view* of
it — never a second copy it mutates independently. Commands take ids and values, apply them to
the owned item, and return a fresh `ItemView`.

```typescript
type ItemView = {
  title: string;
  blocks: BlockView[];
  photos: PhotoView[];
  slug: string;
  dirty: boolean;
};

type PhotoView = {
  id: string;
  fileName: string;
  width: number; height: number;
  used: boolean;              // FR-010
  thumbUrl: string;           // asset-protocol URL, cached
  crop: CropRect | null;
  rotation: 0 | 1 | 2 | 3;
};
```

`thumbUrl` points at a cached thumbnail served over Tauri's asset protocol. Full-size photo
bytes never cross the IPC boundary — that is what keeps SC-008 reachable for 30-photo items.

---

## Commands

### Document and item

| Command | Signature | Notes |
|---------|-----------|-------|
| `open_document` | `(path: string) → ImportResult` | `.docx` extracts photos and positions (FR-002, FR-003) |
| `new_item` | `() → ItemView` | Empty item for the drag-and-drop path (US4) |
| `set_title` | `(title: string) → ItemView` | Used when import detects none (FR-004) |
| `set_body_text` | `(text: string) → ItemView` | Editor writes; markers are reparsed |
| `set_slug` | `(slug: string) → ItemView` | Override (FR-026) |

`ImportResult` is `{ item: ItemView, titleDetected: boolean, warnings: Warning[] }`. When
`titleDetected` is false the UI prompts; it never invents a title.

### Photos

| Command | Signature | Notes |
|---------|-----------|-------|
| `add_photos_from_paths` | `(paths: string[]) → ItemView` | Drag-drop and file picker (FR-006, FR-008) |
| `add_photo_from_clipboard` | `() → ItemView` | Clipboard image (FR-007) |
| `remove_photo` | `(id: string) → ItemView` | |
| `reorder_photos` | `(order: string[]) → ItemView` | Placements follow (FR-009) |
| `rename_photo` | `(id, name) → ItemView` | |
| `set_crop` | `(id, crop \| null) → ItemView` | `null` reverts (FR-014) |
| `rotate_photo` | `(id, quarters) → ItemView` | FR-017 |
| `suggest_crop` | `(id, aspect) → CropRect` | Headroom-biased default (FR-013) |

Both add paths converge on `core::add_photos`. The drag-drop handler subscribes to Tauri's
`onDragDropEvent` and forwards the paths it supplies — HTML5 drag-and-drop is not used (see
research R8).

### Arrangement

| Command | Signature | Notes |
|---------|-----------|-------|
| `arrange_auto` | `(replaceManual: boolean) → ArrangeResult` | FR-019 |
| `set_placement` | `(blockIndex, placement) → ItemView` | Single placement (FR-021) |

`ArrangeResult` reports `replacedManual`. The UI calls with `replaceManual: false` first; if the
result shows manual placements would be lost, it asks before calling again with `true`
(FR-020).

### Preview and export

| Command | Signature | Notes |
|---------|-----------|-------|
| `build_preview` | `() → { fragment: string, warnings: Warning[] }` | |
| `export_fragment` | `(path: string) → void` | Writes the same fragment |
| `copy_fragment` | `() → void` | To the clipboard, for pasting into the CMS |

**The preview rule.** `build_preview` returns the string `core::build` produced. The frontend
puts that exact string into a sandboxed iframe. It does not template it, post-process it,
re-style it, or assemble any part of the news page itself. This is the whole reason the
preview matches the export (FR-022), and it is the one frontend rule worth failing a review
over.

### Servers and publishing

| Command | Signature | Notes |
|---------|-----------|-------|
| `list_servers` | `() → ServerView[]` | Never includes secrets |
| `save_server` | `(config: ServerView) → void` | Connection settings only |
| `set_credential` | `(name, kind, secret) → void` | Straight into the OS secret store (FR-040) |
| `delete_credential` | `(name) → void` | FR-042 |
| `secret_store_available` | `() → boolean` | False → per-session prompt (FR-041) |
| `publish` | `(server: string, dryRun: boolean) → PublicationView` | FR-026, FR-031 |

`ServerView` carries host, user, port, base path, and public URL — and a credential *kind*, not
a credential. No command returns a secret to the frontend. `set_credential` is the only one that
accepts one, and it passes it to `SecretStore` without logging, storing, or echoing it.

Publishing runs off the UI thread and emits `publish-progress` events (`{ file, index, total }`)
so the interface stays responsive.

---

## Error handling

Commands return `Result`; failures arrive as `{ kind, detail }` mirroring `core::Error`. The UI
renders `detail` directly — the core has already made it name the offending file, marker, or
photo (SC-007), so the frontend must not rewrite it into something vaguer.

## Capability restrictions

Tauri capabilities are granted narrowly: filesystem read for user-chosen paths and the
thumbnail cache, clipboard read and write, dialog. No HTTP client and no shell access are
granted to the webview — the only network path in the product is `Transport`, on the Rust side.

The preview iframe is sandboxed with no script execution. Fragments contain no scripts by
contract, and a sandbox means a malformed one cannot execute anything either.
