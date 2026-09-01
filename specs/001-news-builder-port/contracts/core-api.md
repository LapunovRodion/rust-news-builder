# Contract: `newsbuilder-core` public API

The seam both frontends sit on. Constitution principle II makes this the only place domain
rules may live; the CLI and the desktop application must be expressible entirely in terms of
what follows.

Signatures are indicative Rust, not final code.

---

## Ports (the isolation boundary)

Every side effect enters through one of these traits (constitution IV). `core` depends on the
traits; the adapters live in `crates/cli` and `crates/desktop`, and tests substitute fakes.

```rust
pub trait FileStore {
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn exists(&self, path: &Path) -> Result<bool>;
}

pub trait Transport {
    fn connect(&mut self, target: &ServerConfig, cred: &Secret) -> Result<()>;
    fn list(&mut self, remote_dir: &str) -> Result<Vec<RemoteEntry>>;
    fn ensure_dir(&mut self, remote_dir: &str) -> Result<()>;
    fn put(&mut self, remote_path: &str, bytes: &[u8]) -> Result<()>;
    // No removal method, deliberately: deviation D-7 forbids deleting anything from the
    // server, and the absence of the capability is what enforces it.
}

pub trait SecretStore {
    fn get(&self, r: &CredentialRef) -> Result<Option<Secret>>;
    fn set(&self, r: &CredentialRef, s: &Secret) -> Result<()>;
    fn delete(&self, r: &CredentialRef) -> Result<()>;
    fn available(&self) -> bool;   // false → per-session entry (FR-041)
}

pub trait Clock { fn now(&self) -> OffsetDateTime; }
```

**Contract rules**

- No function in `core` performs I/O except through a port. Violations are caught by review and
  by the absence of `std::fs`, `std::net`, and `reqwest`-shaped dependencies in the crate.
- `Transport` is the only route to the network. `core::build` never touches it — building is
  offline (FR-031 falls out of this).
- `Secret` is a newtype whose `Debug` and `Display` print `[redacted]`. It exposes bytes only
  through `expose()`, which is called in exactly one place: the transport adapter. This is the
  structural half of FR-032 and SC-010.

---

## Import

```rust
pub fn import_document(bytes: &[u8], format: SourceFormat) -> Result<Imported>;

pub struct Imported {
    pub title: Option<String>,
    pub item: NewsItem,
    pub warnings: Vec<Warning>,
}
```

**Guarantees**

- `.docx`: paragraph text in document order; every embedded image becomes a `Photo` with
  `origin: Embedded { doc_order }` and a `Placement` at its document position (FR-002, FR-003).
- `.txt` / `.md`: markers in the text become placements, byte-for-byte compatible with the
  reference (FR-005).
- `title` is `None` when no headline can be detected; the caller prompts (FR-004). It is never
  invented.
- Out-of-scope Word constructs (tables, footnotes, tracked changes) are skipped with a
  `Warning`, never an error (FR-034).
- Pure: takes bytes, returns data. No port needed, so import is trivially testable.

---

## Photos

```rust
pub fn add_photos(item: &mut NewsItem, sources: Vec<PhotoSource>) -> Vec<PhotoId>;
pub fn remove_photo(item: &mut NewsItem, id: PhotoId);
pub fn reorder_photos(item: &mut NewsItem, order: &[PhotoId]);
pub fn rename_photo(item: &mut NewsItem, id: PhotoId, name: &str) -> Result<()>;

pub fn set_crop(item: &mut NewsItem, id: PhotoId, crop: Option<CropRect>) -> Result<()>;
pub fn rotate(item: &mut NewsItem, id: PhotoId, quarters: i8);
pub fn default_frame(dims: (u32, u32), target: AspectRatio) -> Option<CropRect>;
```

**Guarantees**

- `add_photos` is the single entry point for every intake path — dropped, pasted, or picked
  (FR-006 – FR-008). Callers differ only in the `PhotoSource` they construct.
- Placements follow reordering and removal; they never silently re-point at a different photo
  (FR-009, INV-1). Removing a placed photo also removes it from its placement, and a placement
  left empty is dropped, with a warning.
- `rename_photo` rejects names that would collide after suffixing (INV-5).
- Adjustments never touch the source: `PhotoSource::Path` is opened read-only and the file is
  never written (FR-015).
- `default_frame` implements the headroom bias of FR-013 and is pure arithmetic over
  dimensions — no image decoding, fully unit-testable.

---

## Arrangement

```rust
pub fn arrange_auto(item: &mut NewsItem, opts: ArrangeOptions) -> ArrangeReport;
pub fn set_placement(item: &mut NewsItem, at: BlockIndex, placement: Placement) -> Result<()>;

pub struct ArrangeOptions { pub replace_manual: bool }
pub struct ArrangeReport { pub placed: usize, pub replaced_manual: usize }
```

**Guarantees**

- Every unplaced photo receives a placement; surplus photos are grouped into `Row` placements
  rather than dropped (FR-019 and its edge case).
- Layout follows shape: landscape photos tend to `FullWidth`, portraits to a float, and
  same-orientation neighbours group into a `Row` (FR-019, acceptance 3).
- With `replace_manual: false`, existing placements are preserved and `replaced_manual` is 0.
  The caller uses this to warn before overwriting (FR-020).
- Deterministic: the same item and options always produce the same arrangement.
- `set_placement` changes exactly one placement and leaves the others untouched (FR-021).

---

## Build

```rust
pub fn build(item: &NewsItem, ctx: &BuildContext) -> Result<BuildOutput>;

pub struct BuildContext { pub public_base_url: Option<Url>, pub slug: Slug }
```

**Guarantees**

- The single rendering path. The desktop preview and the CLI export call this and nothing else,
  which is what makes "preview matches export" structural rather than aspirational (FR-022).
- `fragment` uses inline styles exclusively — no `<style>`, no `link`, no styling classes
  (FR-023). See [html-output.md](./html-output.md).
- Byte-identical for identical input (FR-024, constitution IV).
- Offline. No port is consulted; photo bytes are already in the item.
- With `public_base_url: None`, image URLs are local placeholders for preview; the HTML
  structure is otherwise identical to the published fragment.
- Problems are `warnings`, not errors: a build with a missing photo still returns a fragment
  (FR-034).

---

## Publish

```rust
pub fn publish(
    item: &NewsItem,
    server: &ServerConfig,
    transport: &mut dyn Transport,
    secrets: &dyn SecretStore,
    mode: PublishMode,
) -> Result<Publication>;

pub enum PublishMode { DryRun, Live }
```

**Guarantees**

- Refuses before processing any photo when no credential resolves (FR-029, INV-8) or when the
  remote base path is missing or unwritable (edge case).
- Creates `remote_base_path/slug`, uploads every used photo into it, and returns each public
  URL (FR-026, FR-027).
- Convergent: computes the desired remote set, lists what is present, and uploads only what
  differs. An unchanged item performs zero writes (FR-030, SC-009).
- Never deletes. A photo dropped from the item is left on the server as an orphan (D-7). The
  `Transport` trait has no removal method, so this cannot be violated by mistake.
- `DryRun` calls no mutating `Transport` method and returns a `Publication` with
  `dry_run: true` describing what would happen (FR-031).
- A slug that already names a different item's folder gets a numeric suffix rather than being
  written into (edge case).
- Failure names the file in flight and leaves local state untouched (FR-034, US2 acceptance 5).

---

## Errors and warnings

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    UnsupportedFormat { name: String, detail: String },
    DocumentUnreadable { detail: String },
    PhotoUnreadable { name: String, detail: String },
    SizeBudgetUnreachable { name: String, floor_quality: u8, achieved: u64 },
    NoCredential { server: String },
    SecretStoreUnavailable { detail: String },
    RemotePathUnusable { path: String, detail: String },
    Transport { step: String, detail: String },
    Io { path: PathBuf, source: std::io::Error },
}

pub enum Warning {
    MissingPhoto { marker: String },
    UnusedPhoto { id: PhotoId },
    PhotoSkipped { name: String, reason: String },
    UnsupportedDocumentFeature { what: String },
    SlugSuffixed { from: String, to: String },
}
```

**Contract rules** (constitution V)

- Every variant names the specific offending item — the file, the marker, the photo. This is
  what SC-007 measures.
- No variant carries a `Secret`, a password, or key bytes.
- `core` denies `clippy::unwrap_used`, `clippy::expect_used`, and `clippy::panic` at crate
  level. Errors are returned, never raised.
