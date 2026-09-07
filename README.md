# News Builder

Turns a Word file, a Markdown file, or a plain-text file into an inline-styled HTML fragment for
a CMS, and uploads its photos over SFTP.

Two interfaces over one set of rules:

- **`newsbuilder`** — the command line. Builds and publishes an item that needs no human
  decision. Where a run *would* need one, it refuses and says so rather than guessing.
- **News Builder** — the desktop application (Tauri). Where photos are arranged, cropped and
  rotated, and where an item that arrived as a bare folder of photographs gets made into a page.

Both call the same library, `newsbuilder-core`. There is no second code path, which is what
makes "the same item publishes identically from either interface" a property of the code rather
than something to keep in step by hand.

---

## Contents

- [Building](#building)
- [The markers](#the-markers)
- [`newsbuilder build`](#newsbuilder-build)
- [`newsbuilder publish`](#newsbuilder-publish)
- [`newsbuilder server`](#newsbuilder-server)
- [Exit codes](#exit-codes)
- [`--json` output](#--json-output)
- [Appearance configuration](#appearance-configuration)
- [Credentials](#credentials)
- [Development](#development)

---

## Building

A Nix flake pins everything, including the Rust toolchain, Node, and the Tauri system
dependencies:

```bash
nix develop          # or `direnv allow`, which does the same
cargo build --workspace
```

Without Nix you need the toolchain from `rust-toolchain.toml`, Node 20+ with pnpm for the
desktop frontend, and — on Linux — `webkit2gtk`, `gtk3`, `libsoup3` and `libwebp`. The `core`
and `cli` crates build without any of the desktop dependencies.

```bash
cargo run -p newsbuilder-cli -- build --input news.docx --output news.html
cargo tauri dev                      # the desktop application
```

---

## The markers

A `.txt` or `.md` document places its photos with markers. There are four, and no more — the
language is frozen, and matches the tool this port replaces exactly.

| Marker | Layout | Photos |
|--------|--------|--------|
| `[image:N]` | Full width, centred | exactly one |
| `[images:N,M,…]` | A row, side by side | one or more |
| `[image-left:N]` | Floated left, text wraps to the right | exactly one |
| `[image-right:N]` | Floated right, text wraps to the left | exactly one |

`N` is a photo's **1-based position in the images folder**, after the folder is sorted the way a
person would sort it: `photo2.jpg` comes before `photo10.jpg`.

```text
The lead paragraph introduces the item.

[image:1]

A body paragraph follows the full-width photo.

[images:2,3]

[image-left:4]

Text that wraps around the left float.
```

Rules worth knowing:

- **Whitespace inside the brackets is allowed**: `[images: 2, 3 ]` is the same as `[images:2,3]`.
- **A marker that does not parse stays as text, and warns.** `[image:1,2]` — a full-width marker
  given two photos — is reported by name and left in the prose exactly as written. Nothing is
  ever silently dropped.
- **A marker naming a photo that is not there warns and the rest still builds.** `[image:7]`
  with six photos costs you that photo, not the run.
- **A photo no marker references is not uploaded**, and you are told which one.
- Repeats are allowed: `[images:1,1]` renders the photo twice.

A `.docx` needs no markers at all. Its embedded images are read out of the package and placed
where they sat in the document, so a Word file with photographs in it publishes with no images
folder and no marker typing.

**The desktop application has no markers.** Photos are arranged with placement cards, and marker
text is never typed or shown. Markers exist for documents arriving from elsewhere.

### The title

The title is *detected*, never invented: it is the first non-blank paragraph of the document (or
the first Markdown `#` heading). When none can be found, the CLI wants `--title` and the
application asks. It also becomes the slug — the remote folder name and the stem of every
published photo — by transliteration: `День Конституции` → `den-konstitutsii`.

---

## `newsbuilder build`

Renders the fragment locally. Never touches the network.

| Flag | Required | Meaning |
|------|----------|---------|
| `--input <PATH>` | yes | The source document: `.docx`, `.txt`, or `.md` |
| `--output <PATH>` | yes | Where the HTML fragment is written; parent directories are created |
| `--images-dir <PATH>` | no | Photos for a marker document. Unnecessary for a `.docx` with embedded images |
| `--title <TEXT>` | no | Overrides the title detected in the document. Re-derives the slug too, unless `--news-slug` is also given |
| `--news-slug <TEXT>` | no | Overrides the slug derived from the title. Lowercase letters, digits and hyphens |
| `--public-base-url <URL>` | no | Where the photos will live once published. Without it the image URLs are local placeholders and the fragment is a preview |
| `--appearance <PATH>` | no | A JSON appearance override, applied over the built-in appearance |
| `--json` | no | Write a single JSON result object to stdout instead of prose |
| `--strict` | no | Exit non-zero when the run produced warnings |

```bash
newsbuilder build \
  --input fixtures/inputs/word-with-photos/news.docx \
  --output /tmp/news.html
```

Files in `--images-dir` that are not images are passed over in silence — a stray `notes.txt` in
a photo folder is not something you need telling about. A file that *looks* like an image and
will not decode is a different matter, and warns by name.

Accepted photo formats: `.jpg`, `.jpeg`, `.png`, `.webp`, `.gif`, `.bmp`, `.tif`, `.tiff`. The
container decides, not the name: a `.tmp` inside a Word package that is really a JPEG publishes
correctly.

### One refusal

An item holding photos with no placements, and a document with no markers to derive them from,
has not been arranged. Where those photos go is an editorial decision, so the CLI declines to
invent an answer and points at the application. Exit code 3.

---

## `newsbuilder publish`

Builds, then uploads the photos over SFTP and reports the fragment. Takes every `build` flag,
except that `--output` is optional and `--public-base-url` comes from the server configuration.

| Flag | Required | Meaning |
|------|----------|---------|
| `--server <NAME>` | yes | A saved server configuration, by name |
| `--output <PATH>` | no | Where to also write the fragment. Without it the fragment is printed |
| `--dry-run` | no | Plan everything and mutate nothing |

```bash
newsbuilder publish --input news.docx --server newsroom --dry-run --json
newsbuilder publish --input news.docx --server newsroom --output /tmp/news.html
```

**Publishing converges.** The desired set of remote files is computed from the item, compared
against what is already on the server, and only the difference is uploaded. Re-publishing an
unchanged item performs zero writes and leaves the server byte for byte as it was.

**Nothing is ever deleted.** A photo dropped from an item stays on the server as an orphan. The
transport has no removal operation at all, so this cannot be violated by mistake.

**The host key is checked** against `~/.ssh/known_hosts`. An unknown or changed key is refused,
and the message says how to make it known. Connect once with `ssh` and accept the key, or use
`ssh-keyscan`.

**A folder already holding a different item's files** gets a suffix — `den-znaniy-2` — and you
are told.

---

## `newsbuilder server`

```bash
newsbuilder server add --name newsroom \
  --host news.example.org --user editor --port 22 \
  --remote-base-path /var/www/html/news \
  --public-base-url https://example.org/news/2026/03/ \
  --key ~/.ssh/id_ed25519          # omit for password authentication

newsbuilder server set-credential newsroom
newsbuilder server list [--json]
newsbuilder server remove newsroom
```

| Subcommand | Flags |
|------------|-------|
| `server list` | `--json` |
| `server add` | `--name`, `--host`, `--user`, `--port` (default 22), `--remote-base-path`, `--public-base-url`, `--key` (optional) |
| `server remove <NAME>` | — |
| `server set-credential <NAME>` | — |

`server list` never shows a credential — there is nothing there to show, because the
configuration file holds a *reference* to a secret and never a secret.

`server remove` also clears the stored credential. A secret nobody can reach and nobody can
clear is worse than no secret.

---

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success. Warnings are permitted unless `--strict` was given |
| 1 | Usage error — bad flags, a missing required argument |
| 2 | Input error — an unreadable document, an unsupported format, a missing photo |
| 3 | Refusal — the item needs a human decision, or no credential resolved |
| 4 | Processing error — a photo could not be brought within the size budget |
| 5 | Transport error — connection, authentication, or upload failed |

Results go to stdout; diagnostics and warnings go to stderr, so a script that reads only the
document still leaves a readable trace in its log.

Every failure names the specific offending thing — the file, the marker, or the photo. A message
that says only "upload failed" is a defect, not a terse style.

---

## `--json` output

`--json` switches stdout to a single JSON object. stderr stays human-readable.

```json
{
  "ok": true,
  "fragment_path": "/tmp/news.html",
  "slug": "den-konstitutsii",
  "dry_run": false,
  "remote_folder": "/var/www/html/news/den-konstitutsii",
  "photos": [
    {
      "name": "den-konstitutsii-01.jpg",
      "bytes": 412883,
      "quality": 78,
      "url": "https://example.org/news/2026/03/den-konstitutsii/den-konstitutsii-01.jpg",
      "uploaded": true
    }
  ],
  "warnings": [
    { "kind": "unused_photo", "detail": "photo #7 is referenced by no placement and was not uploaded" }
  ]
}
```

`remote_folder` and `uploaded` appear only for a publish. `fragment` carries the fragment itself
when no `--output` was given. `uploaded: false` means the server already had that exact file and
it was left alone.

On failure:

```json
{ "ok": false, "error": { "kind": "no_credential", "detail": "…", "exit": 3 } }
```

No field of this document ever contains a password or key material.

---

## Appearance configuration

One appearance ships built in. `--appearance <PATH>` overrides it key by key — overriding
`image.max_width` leaves every style string at its built-in value. The schema is the reference
tool's, so existing `style-presets/*.json` files load unmodified.

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
    "container": "max-width: 920px; margin: 0 auto; …",
    "title": "…",
    "lead": "…",
    "paragraph": "…",
    "image_wrapper": "…",
    "image": "…",
    "row_wrapper": "…",
    "row_item": "…",
    "row_image": "…",
    "float_left": "…",
    "float_right": "…",
    "clear": "…"
  }
}
```

### `image`

| Key | Meaning |
|-----|---------|
| `max_width` | The longest edge a published photo may have. Photos are never scaled up. At least 1 |
| `max_bytes` | The size a published photo must fit into. At least 1024 |
| `jpeg_quality` | The quality the JPEG search starts at, 1–100 |
| `jpeg_min_quality` | The quality the JPEG search will not go below, 1–100 |
| `webp_quality` | The quality the WebP search starts at, 1–100 |
| `webp_min_quality` | The quality the WebP search will not go below, 1–100 |

A photo is re-encoded down a quality ladder until it fits `max_bytes`. A photo still over budget
at the minimum quality is published anyway, with a warning naming that photo and the size it
reached.

### `styles`

Each key is the inline `style` attribute for one slot of the output. Values are emitted verbatim
— not parsed, not normalised.

| Key | Applied to |
|-----|-----------|
| `container` | The wrapping `div` |
| `title` | The `h1` |
| `lead` | The first paragraph |
| `paragraph` | Every other paragraph |
| `image_wrapper` | The wrapper of a full-width placement |
| `image` | The `img` of a full-width placement |
| `row_wrapper` | The wrapper of a row |
| `row_item` | One cell of a row |
| `row_image` | The `img` inside a row cell |
| `float_left` | A `[image-left:N]` placement |
| `float_right` | A `[image-right:N]` placement |
| `clear` | The clearing element after a float |

### Validation

| Rule | On violation |
|------|--------------|
| `min_quality <= quality`, both 1–100 | Rejected, naming the field and the file |
| `max_width >= 1`, `max_bytes >= 1024` | Rejected, naming the field and the file |
| A style string containing `</` or `<script` | Rejected — it would break out of the attribute |
| Malformed JSON | Rejected, with the line and column |
| An unknown key under `styles` or `image` | Warned about, ignored, and the run continues |

Unknown keys warn rather than fail so that preset files written for an older or newer version
still load.

### Resolution order

1. The built-in appearance.
2. A user appearance file in the OS configuration directory, if present.
3. `--appearance <PATH>`, or the file chosen in the application.

---

## Credentials

A publishing password or key passphrase lives in the **operating system's secret store** —
Keychain on macOS, Credential Manager on Windows, the Secret Service on Linux. It is never
written to a file by this tool.

It is never an argument, either. `server set-credential` reads it from a terminal prompt with
echo disabled, or from stdin when stdin is not a terminal, so it cannot land in shell history or
in a process listing.

```bash
newsbuilder server set-credential newsroom          # prompts
printf '%s' "$PASSWORD" | newsbuilder server set-credential newsroom   # from a pipe, in CI
```

If no secret store is reachable — a headless Linux box with no `gnome-keyring-daemon`, for
instance — the tool says so plainly. The application then asks once per session and holds the
credential in memory only. It does not fall back to a file.

The fragment, the `--json` document, the logs and the configuration file are all free of secrets
by construction: the type that holds one renders as `[redacted]` through both `Debug` and
`Display`, so there is no formatting path that can print it.

---

## Development

```bash
just gates        # the merge bar: fmt, clippy, tests, parity fixtures, frontend checks
just test         # cargo test --workspace
just parity       # only the fixtures captured from the Python reference
just invariants   # the data-model invariants
just desktop      # cargo tauri dev
```

Two suites are opt-in, because one needs a server and the other measures a clock:

```bash
just e2e             # publishes to a throwaway local sshd; needs OpenSSH on PATH
just bench-preview   # the preview-latency measurements, built optimised
```

Parity with the Python tool it replaces is asserted against golden files captured from that tool
at a pinned commit, not against hand-written expectations. Regenerate them only when the pinned
commit moves:

```bash
just capture         # rewrites fixtures/inputs/ and fixtures/reference/
just capture-check   # fails if the committed goldens are stale
git diff --stat fixtures/reference    # a non-empty diff is a behaviour change to review
```

### Layout

| Path | What is in it |
|------|---------------|
| `crates/core` | Every domain rule: import, markers, arrangement, photo processing, rendering, publishing |
| `crates/cli` | The `newsbuilder` binary |
| `crates/desktop` | The Tauri application; its frontend is under `crates/desktop/ui` |
| `tools/capture-reference` | The harness that regenerates the parity goldens from the Python reference |
| `fixtures` | The input corpus and the captured goldens |
| `specs/001-news-builder-port` | The specification, plan, contracts and task list |

Side effects reach `core` only through the traits in `crates/core/src/ports.rs`, which is why
the test suite needs no filesystem, no network and no clock. The same fragment is produced on
every machine and every run: nothing in the pipeline reads the clock, the locale, or hash
iteration order.
