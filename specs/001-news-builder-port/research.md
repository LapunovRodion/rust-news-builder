# Phase 0 Research: News Builder Port

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-09-01

Every entry below resolves an unknown from the plan's Technical Context. No `NEEDS
CLARIFICATION` remains.

---

## R1. Desktop framework

**Decision**: Tauri 2.x, with a TypeScript + Svelte 5 + Vite frontend under
`crates/desktop/ui`.

**Rationale**: FR-022 demands a live preview that matches the exported fragment. The exported
artefact *is* HTML with inline styles, so the only honest preview is one produced by an HTML
engine. A webview is that engine: the Rust core renders the fragment once, and the frontend
displays that same string in a sandboxed iframe. Preview fidelity becomes structural rather
than something to keep in sync.

The secondary requirements point the same way. The crop frame of FR-012 is a few dozen lines of
pointer handling in the DOM against custom-drawn widgets and hit-testing in an immediate-mode
GUI. The "заметно приятнее и удобнее" bar from `task.md` §2.2 is reachable with ordinary CSS.
Tauri 2 has been stable since late 2024 and is actively released (2.10.x as of March 2026).

**Alternatives considered**:

- **egui / eframe** — fastest path to a window and the most-downloaded Rust GUI, but it cannot
  render HTML. Previewing the fragment would mean writing a second renderer that approximates
  the browser, which either drifts from the export or becomes a project of its own. Rejected on
  FR-022.
- **Iced** — a cleaner declarative model than egui and a more native feel, but the same
  disqualifying gap on HTML preview, with a steeper learning curve.
- **Slint** — good for embedded and declarative UI; same HTML gap.
- **egui + an embedded `wry` webview for the preview pane only** — technically possible, but it
  ships a webview anyway while giving up the ease of building the rest of the UI in it. If a
  webview is in the binary, Tauri is the coherent version of that choice.

**Cost accepted**: a Node toolchain in the build, recorded in the plan's Complexity Tracking.
It is confined to `crates/desktop/ui`; `core` and `cli` build with cargo alone.

---

## R2. Frontend framework inside the webview

**Decision**: Svelte 5 with Vite.

**Rationale**: The UI is a handful of screens with genuinely local state — a text editor, a
photo list, a crop overlay, a preview pane. Svelte compiles away, keeps the bundle small (which
matters for startup time, Tauri's weakest measured area), and needs no runtime state library
for this size of application.

**Alternatives considered**: React, which has a larger ecosystem and more familiar tooling but
a heavier bundle and more ceremony for this scope; and no framework at all, which is tempting
until the crop overlay and the photo list start sharing state. This is a reversible decision
confined to `crates/desktop/ui` — nothing in `core` depends on it.

**Revisited 2026-09-01 and confirmed.** The strongest argument for React was `react-easy-crop`,
a mature library that returns crop coordinates in original-image pixels — exactly the shape
`set_crop` wants — and would have saved roughly a day on the crop overlay. Weighed against it:
the bundle-size argument for Svelte is weak for an application loading from local disk, but the
state argument is real. `NewsItem` lives in Rust and the frontend holds a projection
(`ItemView`), so React's central strength — managing complex client state — goes unused here,
while its costs (stale closures, re-render discipline) do not. The crop overlay is
approximately 200 lines of pointer handling, which is a bounded amount of work to own.

Author's decision: Svelte 5.

---

## R3. Reading `.docx`, including embedded images and their positions

**Decision**: A purpose-built reader in `core::import::docx`, using `zip` for the package and
`quick-xml` for `word/document.xml` and `word/_rels/document.xml.rels`.

**Rationale**: FR-002 and FR-003 need three things and nothing else: the paragraph text in
document order, the position of each drawing relative to those paragraphs, and the bytes of
each referenced media part. That is a bounded walk — `w:body` → `w:p` → `w:r` → `w:drawing` →
`a:blip/@r:embed` → relationship id → `word/media/*` — and owning it gives exact control over
ordering, which determinism (constitution IV) depends on.

The existing crates are shaped for a different job. `docx-rs` (bokuweb) is primarily a writer
with a read path bolted on and a document model built for round-tripping; even using it, we
would still open the zip ourselves for the media bytes. Its image handling decodes and
re-encodes previews unless explicitly disabled, which is work we do not want done for us.
`docx-rust` (PoiScript) is a parser but has thinner coverage of drawing anchors.

The reference implementation already validates this approach: it carries a stdlib-only DOCX
reader as a fallback path when `python-docx` is absent.

**Alternatives considered**: `docx-rs` — rejected as above, though it stays a fallback if the
purpose-built reader proves fragile on real editorial documents; `rdocx` — a higher-level
python-docx-inspired API with a layout engine, far more than this feature needs and a much
larger surface to depend on; shelling out to a converter — rejected outright, it violates the
offline and determinism constraints.

**Risk and mitigation**: real Word files vary. Mitigation is fixture-driven: collect actual
editorial `.docx` files early, one per structural variant (inline images, anchored/floating
images, images inside tables, headings styled as titles), and grow the corpus as failures
appear. Content the spec puts out of scope — tables, footnotes, tracked changes — is skipped
with a warning rather than a failure (FR-034).

---

## R4. Image geometry and re-encoding to a size budget

**Decision**: `image` for decoding, resizing, cropping, and rotating; `kamadak-exif` for reading
orientation; `image`'s JPEG encoder for quality-controlled JPEG; the `webp` crate (libwebp
bindings) behind a cargo feature for lossy WebP.

**Rationale**: `image` covers every format FR-011 lists and all the geometry FR-012 – FR-017
need. Orientation is read with `kamadak-exif` and applied as an explicit transform, so the
correction is visible in one place and testable (FR-016).

Meeting the byte budget (FR-028) is a quality search: encode, measure, and step quality down
within the configured bounds until the result fits or the floor is reached. This mirrors the
reference's `85 → 50` behaviour and is what keeps parity.

Lossy WebP is the one gap in the pure-Rust stack: `image-webp` encodes losslessly only. A
`.webp` source therefore cannot be re-encoded to a size budget without libwebp. Since parity
requires keeping the source extension — the file name appears in the published URL — the C
dependency is taken, confined to `core::photo::encode` and gated by a feature so `core` still
builds without it.

**Alternatives considered**: transcoding `.webp` sources to JPEG, which changes published file
names and breaks URL parity for those items; rejecting `.webp` inputs, which fails FR-011;
`zenjpeg`, which offers a convenient `apply_exif_orientation` but adds a second imaging stack
for one function that is a small transform over `image` types.

---

## R5. Portrait framing without machine learning

**Decision**: Photos are never cropped by default — layouts scale to fit. Where the editor or a
layout does crop, the crop rectangle for a portrait image is anchored with **headroom bias**:
the excess is removed asymmetrically, taking roughly one quarter from the top and three
quarters from the bottom, rather than centring the frame.

**Rationale**: FR-013 asks that a person's head not be cut off without the editor's action. In
editorial photographs of people the subject's head sits in the upper third, so biasing the
retained region upward preserves it in the overwhelming majority of cases, at the cost of some
floor. The rule is arithmetic: deterministic (constitution IV), instant, testable against the
twenty-photo benchmark of SC-003, and it adds no dependency.

**Alternatives considered**: face detection (a bundled ONNX or OpenCV model) — the accurate
answer, but it adds tens of megabytes, a model licence, per-photo latency, and
non-determinism if the runtime changes; saliency or entropy-based smart crop — cheaper than a
model but still heuristic, harder to explain to an editor, and non-obvious to test. Either can
be added later behind the same `frame.rs` seam if the benchmark shows the bias rule failing;
neither is needed for this feature.

---

## R6. SSH and SFTP

**Decision**: `russh` + `russh-sftp` on `tokio`, behind the `Transport` trait in
`core::ports`.

**Rationale**: Pure Rust, so cross-compilation and Windows builds need no system libssh2 and no
OpenSSL. `russh` supports both authentication modes FR-029 requires — private key and password
— and `russh-sftp` implements the client side of SFTP v3 with a `std::fs`-shaped API, which
maps directly onto the reference's SFTP usage.

The trait boundary is what matters most: it keeps the core testable with no server (constitution
IV) and makes the idempotent-republish rule of FR-030 a property of `core::publish` rather than
of a particular SSH library.

**Alternatives considered**: `ssh2` (libssh2 bindings), the most established option but a C
dependency with awkward Windows builds, and blocking-only; `openssh-sftp-client`, which is
pure Rust and well made but drives an external `ssh` process — precisely the external-binary
dependency FR-035 removes.

---

## R7. Credential storage

**Decision**: `keyring` 3.x, wrapping the macOS Keychain, the Windows Credential Manager, and
the Secret Service on Linux. Credentials are referenced from a server configuration by name and
never serialised into it.

**Rationale**: FR-040 requires exactly this, and `keyring` 3 is the established cross-platform
abstraction. Version 3 added synchronous Secret Service access via `dbus-secret-service`, so
Linux support no longer drags in an async runtime for what is a blocking, once-per-publish
lookup.

FR-041 covers the failure mode that matters on Linux: Secret Service needs a running daemon
(`gnome-keyring-daemon`, KWallet), and headless or minimal desktops may have none. The
application detects the absence, says so plainly, and asks for the credential for that session
only — it never falls back to writing the secret to disk.

**Alternatives considered**: an encrypted local file, which only moves the problem to where the
key lives; the reference's plaintext JSON, which the spec explicitly replaces.

**Supporting design**: a `Secret` newtype in `core::secret` whose `Debug` and `Display` render
`[redacted]`. Combined with the clippy denials, this makes FR-032 and SC-010 hard to violate by
accident rather than a matter of reviewer vigilance.

---

## R8. Drag-and-drop and clipboard paste

**Decision**: Use Tauri's window-level drag-drop event (`onDragDropEvent`, which supplies
absolute paths on `enter` and `drop`), not HTML5 drag-and-drop. Clipboard images come from
`tauri-plugin-clipboard-manager`.

**Rationale**: With `dragDropEnabled` set, Tauri intercepts the OS-level drop before the webview
sees it, so HTML5 `drop` handlers do not fire reliably across platforms. Using the Tauri event
also hands us real filesystem paths rather than sandboxed `File` objects, which is what the
core needs to read the bytes. `dragDropEnabled` is on by default in the window configuration.

FR-006 and FR-007 are then two paths into one core operation, `add_photos`, differing only in
whether the bytes arrive from a path or from the clipboard.

**Alternatives considered**: disabling `dragDropEnabled` to use HTML5 drag-and-drop, which
would give the frontend `File` objects it must marshal across the IPC boundary — more work and
a worse fit for large photos.

---

## R9. Parity fixtures and the capture harness

**Decision**: `tools/capture-reference/` clones the Python reference at a pinned commit, runs it
over `fixtures/inputs/` in a virtual environment with uploads stubbed, and writes goldens into
`fixtures/reference/`. Rust tests compare against those files byte for byte. The harness runs on
demand, not in CI; goldens are committed.

**Rationale**: Constitution III requires fixtures captured from the reference rather than
hand-written expectations, and constitution I makes those fixtures the parity gate. Committing
the goldens keeps the test suite hermetic — CI needs no Python — while the harness keeps them
reproducible and lets a pinned-commit bump show up as a reviewable diff.

Coverage required before implementation starts: fragment rendering for each of the four marker
layouts, slug transliteration across the Cyrillic table, marker parsing including malformed
input, natural-sort image ordering, and remote path plus public URL construction.

**Alternatives considered**: hand-written expectations, which constitution III rules out and
which would encode our reading of the reference rather than its behaviour; running Python inside
CI, which makes every build depend on a second toolchain for no gain over committed goldens.

---

## R10. Testing the transport and the pipeline

**Decision**: Three layers. Unit and golden tests run against in-memory fakes for every port.
An integration suite exercises the full build through `core::build` with a fake `Transport` that
records operations, which is where FR-030's convergence and FR-031's dry-run are asserted. One
end-to-end test publishes to a local `sshd` and is opt-in via an environment variable.

**Rationale**: Convergence and dry-run are properties of the orchestration, not of the wire
protocol, so a recording fake proves them precisely and fast. The end-to-end test exists to
catch the thin layer the fake cannot — real authentication, real path handling — and stays
opt-in so the default `cargo test` needs no server.

**Alternatives considered**: `testcontainers` with an OpenSSH image, which is a reasonable
upgrade if the opt-in test proves too easy to skip, but it makes Docker a test prerequisite for
a single thin layer.

---

## R11. The editor surface with inline placement cards

**Decision**: Build the text surface on TipTap (ProseMirror) with `svelte-tiptap`, rendering each
placement as a custom node view backed by a Svelte component. Do not hand-roll `contenteditable`.

**Rationale**: Deviation D-9 removed marker typing, which makes the editor the most demanding
part of the frontend: editable text interleaved with non-editable widgets that can be inserted at
a caret, dragged in from outside, dropped onto one another to form rows, and dragged to new
positions. Hand-written `contenteditable` handles none of that well — selection across widget
boundaries, undo, and paste normalisation are exactly where it fails, and each failure is a bug
an editor will hit in their first hour.

ProseMirror exists for this problem. Its document model is a validated node tree, which maps
directly onto `body: Vec<Block>` — a paragraph node and a placement node, nothing else in the
schema. `SvelteNodeViewRenderer` from `svelte-tiptap` lets the placement card be an ordinary
Svelte component with its own layout dropdown and remove control, so T107 stays a normal
component rather than DOM surgery.

The constrained schema is what keeps this honest under constitution principle II: with only two
node types allowed, the editor structurally cannot produce a body the Rust core would reject, and
`set_body` remains a straight serialisation of the node tree into `BlockInput[]`.

**Alternatives considered**: raw `contenteditable`, rejected above; a plain `textarea` with the
cards rendered beside it, which is the "storyboard panel" shape the author rejected as indirect;
Lexical, which is capable but React-first and would drag in what R2 just decided against; Tipex,
a Svelte-5-native wrapper over TipTap, which is appealing but a thinner project than TipTap
itself and adds a layer over the node-view API this design leans on most.

**Cost**: TipTap and ProseMirror are a real dependency in the frontend. They stay entirely inside
`crates/desktop/ui`; `core` and `cli` are untouched, and the published HTML is produced by Rust
regardless of what the editor is built from.

## Sources

- [Tauri (software framework) — Wikipedia](https://en.wikipedia.org/wiki/Tauri_(software_framework))
- [Tauri documentation — drag-drop events and window configuration](https://github.com/tauri-apps/tauri-docs)
- [The Rust GUI Landscape in 2026](https://wrenlearnsrust.com/posts/2026-03-11-rust-gui-landscape-2026.html)
- [Tauri vs Iced vs egui: performance comparison](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html)
- [The state of Rust GUI libraries — LogRocket](https://blog.logrocket.com/state-rust-gui-libraries/)
- [bokuweb/docx-rs](https://github.com/bokuweb/docx-rs) and [docx-rs on lib.rs](https://lib.rs/crates/docx-rs)
- [PoiScript/docx-rs](https://github.com/PoiScript/docx-rs)
- [image-rs/image](https://github.com/image-rs/image) and [image-webp](https://crates.io/crates/image-webp)
- [kamadak/exif-rs](https://github.com/kamadak/exif-rs)
- [russh-sftp](https://crates.io/crates/russh-sftp) and [russh](https://lib.rs/crates/russh)
- [openssh-sftp-client](https://github.com/openssh-rust/openssh-sftp-client)
- [keyring crate](https://docs.rs/keyring)
- [Tiptap — ProseMirror core concepts](https://tiptap.dev/docs/editor/core-concepts/prosemirror) and [Schema](https://tiptap.dev/docs/editor/core-concepts/schema)
- [svelte-tiptap](https://www.npmjs.com/package/svelte-tiptap)
- [Tipex — Svelte 5 editor built on Tiptap](https://tipex.pages.dev/)
