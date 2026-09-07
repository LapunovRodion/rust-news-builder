---

description: "Task list for the News Builder Rust port"
---

# Tasks: News Builder Port with Word Import and In-App Photo Editing

**Input**: Design documents from `/specs/001-news-builder-port/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: Test tasks are **included and mandatory**. Constitution principle III makes test-first
non-negotiable and requires golden fixtures captured from the Python reference rather than
hand-written expectations. Every test task in a story precedes that story's implementation and
must fail before the implementation is written.

**Organization**: Tasks are grouped by user story so each story can be implemented, tested, and
demonstrated on its own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US5)
- Exact file paths are given in every task

## Path Conventions

Three-crate Rust workspace per plan.md: `crates/core/` (all domain rules), `crates/cli/`,
`crates/desktop/` (Tauri, with the TypeScript frontend under `crates/desktop/ui/`). Fixtures in
`fixtures/`, the reference-capture harness in `tools/capture-reference/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace skeleton and the quality gates everything else is measured against

- [X] T001 Create workspace `Cargo.toml` at repository root with members `crates/core`, `crates/cli`, `crates/desktop`, `tools/capture-reference`, and a `[workspace.dependencies]` table pinning the versions from plan.md
- [X] T002 [P] Pin the toolchain in `rust-toolchain.toml` (stable channel, `rustfmt` and `clippy` components, edition 2024)
- [X] T003 [P] Add crate-level lint denials to `crates/core/src/lib.rs`: `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::exit)]` per constitution principle V
- [X] T004 [P] Add `rustfmt.toml` and a `justfile` at repository root exposing the four merge gates: `fmt --check`, `clippy --all-targets -- -D warnings`, `test --workspace`, and the parity fixture suite
- [X] T005 [P] Scaffold the Tauri application in `crates/desktop/`: `tauri.conf.json` (window with `dragDropEnabled: true`), `src/main.rs`, and `ui/` with Vite + Svelte 5 + TypeScript
- [X] T006 [P] Create `.gitignore`, `fixtures/inputs/`, `fixtures/reference/`, and `tools/capture-reference/` directory skeletons

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The parity fixtures, the port traits, and the shared model. Constitution III forbids
writing domain code before the fixtures exist, so T007–T009 genuinely block everything.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Parity fixtures (constitution III — must precede all domain code)

- [X] T007 Assemble the input corpus in `fixtures/inputs/`: `word-with-photos/` (a `.docx` with inline images), `word-anchored/` (floating images), `markers/` (a `.txt` using all four markers), `cyrillic-title/`, `natural-sort/` (photos named `photo1..photo12`), `missing-photo/`, `unused-photo/`, `oversized/`, `portraits/` (20 portrait photographs for the SC-003 benchmark), `mixed-orientation/`
- [X] T008 Implement `tools/capture-reference/src/main.rs`: clone `LapunovRodion/news-builder` at a pinned commit into a cache directory, create a virtualenv, install `requirements-news-builder.txt`, run `news_builder.py` over each `fixtures/inputs/*` case with the upload step stubbed, and write fragments plus warning text into `fixtures/reference/<case>/`
- [X] T009 Run the harness and commit the generated goldens under `fixtures/reference/`, recording the pinned reference commit in `fixtures/reference/PINNED_COMMIT`

### Core scaffolding

- [X] T010 [P] Define typed errors and warnings in `crates/core/src/error.rs` per [contracts/core-api.md](./contracts/core-api.md): `Error` with all nine variants and `Warning` with all five, each naming the offending item
- [X] T011 [P] Implement the redacting `Secret` newtype in `crates/core/src/secret.rs` with `Debug`/`Display` printing `[redacted]` and a single `expose()` accessor, plus tests in the same file asserting both formatters redact
- [X] T012 [P] Define the port traits `FileStore`, `Transport`, `SecretStore`, and `Clock` in `crates/core/src/ports.rs` exactly as specified in [contracts/core-api.md](./contracts/core-api.md)
- [X] T013 [P] Implement in-memory fakes for all four ports in `crates/core/tests/support/fakes.rs`, with `RecordingTransport` capturing an ordered log of every operation for the convergence and dry-run assertions

### Model

- [X] T014 [P] Implement `NewsItem`, `Block`, `ParagraphKind`, and `Layout` in `crates/core/src/model/item.rs` per [data-model.md](./data-model.md)
- [X] T015 [P] Implement `Photo`, `PhotoId`, `PhotoOrigin`, `PhotoSource`, `Adjustments`, `CropRect`, and `NaturalKey` in `crates/core/src/model/photo.rs`
- [X] T016 [P] Implement `Appearance` and `StyleSet` in `crates/core/src/model/appearance.rs`, with the built-in appearance reproducing the reference's `DEFAULT_STYLES` and `DEFAULT_CONFIG` values exactly
- [X] T017 [P] Implement `ServerConfig`, `ServerConfigRef`, `CredentialRef`, and the `Slug` newtype in `crates/core/src/model/server.rs`
- [X] T018 Write invariant tests in `crates/core/tests/model_invariants.rs` covering INV-1 through INV-8 from data-model.md (placement references resolve, slug shape, unique photo ids, layout/count agreement, unique file names, crop within bounds, quality bounds, credential presence)
- [X] T019 Implement the appearance configuration loader and validation in `crates/core/src/model/appearance_config.rs` per [contracts/appearance-config.md](./contracts/appearance-config.md), including a test that every `style-presets/*.json` file from the reference loads unmodified
- [X] T020 [P] Initialise `tracing` in `crates/core/src/lib.rs` with a subscriber configuration that cannot format a `Secret`, and wire module declarations so the crate compiles

**Checkpoint**: Goldens committed, ports faked, model in place — user stories can begin

---

## Phase 3: User Story 1 - Word file in, laid-out page out (Priority: P1) 🎯 MVP

**Goal**: Open a `.docx` and immediately see the finished news page — text formatted, every photo
placed where it stood in the document, with no manual photo export and no folder selection.

**Independent Test**: Open a `.docx` with text and embedded photos; the preview shows the same
text and the same photo order and positions as the source, and no step asked for a folder.

### Tests for User Story 1 ⚠️ Write first, confirm they fail

- [X] T021 [P] [US1] Golden test in `crates/core/tests/parity_render.rs` comparing rendered fragments against `fixtures/reference/markers/` for each of the four layouts, byte for byte
- [X] T022 [P] [US1] Golden test in `crates/core/tests/parity_ordering.rs` asserting natural-sort photo ordering against `fixtures/reference/natural-sort/`
- [X] T023 [P] [US1] Unit tests in `crates/core/tests/markers.rs` for marker parsing: all four forms, whitespace variants, multi-index lists, malformed markers, and a marker naming a missing photo producing `Warning::MissingPhoto`
- [X] T024 [P] [US1] Unit tests in `crates/core/tests/title_detection.rs` covering a headline as first line, a Markdown `#` heading, a `.docx` Heading-styled paragraph, and a document with no detectable title yielding `None`
- [X] T025 [P] [US1] Integration test in `crates/core/tests/import_docx.rs` asserting that `fixtures/inputs/word-with-photos/news.docx` yields paragraphs in document order, one `Photo` per embedded image with `origin: Embedded`, and a `Placement` at each image's document position
- [X] T026 [P] [US1] Contract test in `crates/core/tests/html_contract.rs` asserting the rendered fragment contains no `<style>`, no `<link>`, no `class=`, no `<script>`, and no `<!doctype>`/`<html>`/`<body>`
- [X] T027 [P] [US1] Determinism test in `crates/core/tests/determinism.rs` building the same item twice in one process and once after a shuffle of internal insertion order, asserting byte-identical fragments

### Implementation for User Story 1

- [X] T028 [P] [US1] Implement the marker language **parser** in `crates/core/src/markers/mod.rs`, reproducing the reference's `MARKER_PATTERN` semantics including single-photo `Row` normalisation to `FullWidth`. Parse only — no emitter is needed, because the editor works on structure and never sees marker text (deviation D-9)
- [X] T029 [P] [US1] Implement natural-sort key construction in `crates/core/src/model/photo.rs`, reproducing the reference's `natural_sort_key` so `photo2` precedes `photo10`
- [X] T030 [P] [US1] Implement plain-text and Markdown import in `crates/core/src/import/plain.rs`, including paragraph splitting and body normalisation
- [X] T031 [P] [US1] Implement title detection in `crates/core/src/import/title.rs` for all three source formats, returning `None` rather than inventing a title
- [X] T032 [US1] Implement the DOCX package reader in `crates/core/src/import/docx.rs` using `zip` and `quick-xml`: walk `word/document.xml` for paragraph text in order, resolve `w:drawing` → `a:blip/@r:embed` against `word/_rels/document.xml.rels`, and extract the referenced `word/media/*` bytes
- [X] T033 [US1] Map extracted media to `EmbeddedMedia { after_paragraph }` and convert those positions into `Placement` blocks in `crates/core/src/import/docx.rs` (FR-003)
- [X] T034 [US1] Emit `Warning::UnsupportedDocumentFeature` for out-of-scope Word constructs (tables, footnotes, comments, tracked changes) in `crates/core/src/import/docx.rs` rather than failing (FR-034)
- [X] T035 [US1] Implement `import_document` in `crates/core/src/import/mod.rs` dispatching on `SourceFormat` and returning `Imported { title, item, warnings }`
- [X] T036 [P] [US1] Implement EXIF orientation reading and application in `crates/core/src/photo/orient.rs` using `kamadak-exif`, as an explicit transform over `image` types
- [X] T037 [US1] Implement decode, scale-down, and thumbnail generation in `crates/core/src/photo/mod.rs` — never scaling up, and caching thumbnails by photo id and adjustment hash
- [X] T038 [US1] Implement the inline-style HTML renderer in `crates/core/src/render/mod.rs` per [contracts/html-output.md](./contracts/html-output.md): container, title, lead and body paragraphs, the four placement forms, clearing elements, and HTML escaping
- [X] T039 [US1] Implement the `build` pipeline in `crates/core/src/build.rs` returning `BuildOutput { fragment, processed, warnings }`, consulting no port so the build is offline
- [X] T040 [US1] Implement `newsbuilder build` in `crates/cli/src/main.rs` with `--input`, `--images-dir`, `--output`, `--title`, `--news-slug`, `--public-base-url`, `--appearance`, and `--json` per [contracts/cli.md](./contracts/cli.md)
- [X] T041 [US1] Implement the Tauri commands `open_document`, `new_item`, `set_title`, `set_body`, `set_slug`, and `build_preview` in `crates/desktop/src/commands/item.rs` per [contracts/desktop-commands.md](./contracts/desktop-commands.md), with the body crossing the boundary as `BlockInput[]` rather than as text
- [X] T042 [US1] Implement `ItemView`/`PhotoView` projection and the managed session state holding the open `NewsItem` in `crates/desktop/src/state.rs`, serving thumbnails over the asset protocol so full-size bytes never cross the IPC boundary
- [X] T043 [P] [US1] Build the editor screen in `crates/desktop/ui/src/lib/Editor.svelte` on TipTap with a two-node schema — paragraph and placement, nothing else (research R11) — plus the title field and the photo list showing document order. Marker text is never shown (FR-018b, deviation D-9)
- [X] T107 [US1] Build the placement card in `crates/desktop/ui/src/lib/PlacementCard.svelte` as a TipTap node view via `SvelteNodeViewRenderer`: photo thumbnails, a layout control (full width / row / left / right), and a remove control, calling `set_layout` and `remove_placement` (FR-018b)
- [X] T108 [US1] Implement serialisation between the TipTap document and `BlockInput[]` in `crates/desktop/ui/src/lib/body.ts`, with a round-trip test asserting that document → `BlockInput[]` → document is lossless and never forms marker text
- [X] T044 [US1] Build the preview pane in `crates/desktop/ui/src/lib/Preview.svelte`, injecting the exact string returned by `build_preview` into a sandboxed iframe with no templating, post-processing, or re-styling
- [X] T045 [US1] Wire live rebuild on edit in `crates/desktop/ui/src/routes/+page.svelte`, debounced so a text edit refreshes the preview within 150 ms
- [X] T046 [US1] Add the CLI refusal path in `crates/cli/src/main.rs`: exit 3 with a message pointing at the desktop application when an item has photos but no placements and no markers to derive them from

**Checkpoint**: A Word file opens, lays out, previews, and exports a CMS-ready fragment. Quickstart scenario 1 passes. This is the MVP.

---

## Phase 4: User Story 2 - Publish photos and hand a fragment to the CMS (Priority: P2)

**Goal**: Upload processed photos into a per-item remote folder and produce the fragment with
public URLs.

**Independent Test**: Publish a prepared item to a test server; the remote folder is created, all
photos land in it, and the fragment references them by public URL.

### Tests for User Story 2 ⚠️ Write first, confirm they fail

- [X] T047 [P] [US2] Golden test in `crates/core/tests/parity_slug.rs` covering the full Cyrillic transliteration table including `і ї є ў`, against `fixtures/reference/cyrillic-title/`
- [X] T048 [P] [US2] Golden test in `crates/core/tests/parity_paths.rs` asserting remote path and public URL construction, including base values with and without trailing slashes
- [X] T049 [P] [US2] Golden test in `crates/core/tests/parity_encode.rs` asserting the quality-step search reaches the same final size and quality as `fixtures/reference/oversized/`
- [X] T050 [P] [US2] Convergence test in `crates/core/tests/publish_converge.rs` using `RecordingTransport`: publish twice unchanged and assert the second run issues zero `put` calls; then remove a photo from the item, publish again, and assert its remote file is still listed and untouched (FR-030, deviation D-7, SC-009)
- [X] T051 [P] [US2] Dry-run test in `crates/core/tests/publish_dryrun.rs` asserting `PublishMode::DryRun` calls no mutating `Transport` method and returns `dry_run: true` with the full planned URL set
- [X] T052 [P] [US2] Refusal tests in `crates/core/tests/publish_refusals.rs`: no credential, unwritable remote base path, and a slug colliding with a different item's folder — each rejected before any photo is processed
- [X] T053 [P] [US2] Secret-leak test in `crates/core/tests/no_secret_leak.rs` running a full publish against fakes with a sentinel password, then asserting the sentinel appears in no log line, no error `Display`, no `Debug` output, and no written file (SC-010)
- [X] T054 [P] [US2] Warning tests in `crates/core/tests/publish_warnings.rs` for `fixtures/inputs/missing-photo/` and `fixtures/inputs/unused-photo/`, asserting the build completes and unused photos are not uploaded

### Implementation for User Story 2

- [X] T055 [P] [US2] Implement slug transliteration in `crates/core/src/publish/slug.rs`, reproducing the reference's `CYRILLIC_TRANSLIT` table exactly, then lowercasing, collapsing non-alphanumerics to single hyphens, and trimming
- [X] T056 [P] [US2] Implement remote path and public URL construction in `crates/core/src/publish/paths.rs`, joining with exactly one slash regardless of trailing slashes
- [X] T057 [US2] Implement quality-search encoding in `crates/core/src/photo/encode.rs`: encode, measure, step quality down toward the floor until the result fits `max_bytes`, emitting `SizeBudgetUnreachable` at the floor
- [X] T058 [US2] Add lossy WebP encoding behind the `webp` cargo feature in `crates/core/src/photo/encode.rs`, preserving the source extension so published URLs match the reference
- [X] T059 [US2] Implement publish orchestration in `crates/core/src/publish/mod.rs`: resolve credential, verify the remote base path, compute the desired remote file set, upload only what differs, and suffix a colliding slug — never deleting anything from the server (deviation D-7)
- [X] T060 [US2] Implement the `russh` + `russh-sftp` transport adapter in `crates/desktop/src/adapters/transport.rs` and `crates/cli/src/adapters/transport.rs`, supporting both key and password authentication, sharing one implementation module
- [X] T061 [US2] Implement the `keyring` secret-store adapter in `crates/core/src/adapters/keyring_store.rs`, reporting `available() == false` when no Secret Service is reachable
- [X] T062 [US2] Implement server configuration persistence in `crates/core/src/model/server_store.rs`, writing connection settings to the OS config directory with the credential held only as a `CredentialRef`
- [X] T063 [US2] Implement `newsbuilder publish` and the `newsbuilder server` subcommands in `crates/cli/src/main.rs`, reading secrets from a no-echo prompt or stdin and never from an argument
- [X] T064 [US2] Implement the CLI exit-code mapping in `crates/cli/src/exit.rs` (0/1/2/3/4/5) and the `--json` result document per [contracts/cli.md](./contracts/cli.md)
- [X] T065 [US2] Implement the Tauri commands `list_servers`, `save_server`, `set_credential`, `delete_credential`, `secret_store_available`, and `publish` in `crates/desktop/src/commands/publish.rs`, emitting `publish-progress` events off the UI thread
- [X] T066 [P] [US2] Build the server settings screen in `crates/desktop/ui/src/lib/ServerSettings.svelte`, which never receives a secret back from the backend
- [X] T067 [US2] Build the publish dialog in `crates/desktop/ui/src/lib/PublishDialog.svelte` with a dry-run option, per-file progress, and the resulting fragment offered for copying
- [X] T068 [US2] Implement the per-session credential fallback in `crates/desktop/src/commands/publish.rs` and its prompt in the UI, used when `secret_store_available` is false, writing nothing to disk (FR-041)

**Checkpoint**: The full publish loop works from both interfaces. Quickstart scenario 2 passes.

---

## Phase 5: User Story 3 - Portrait photos that keep their heads (Priority: P3)

**Goal**: Portrait photos are framed sensibly by default and adjustable with a visual crop
control, with no separate image editor.

**Independent Test**: Add a portrait photo of a person, confirm the default framing retains the
head, adjust the crop in the app, and confirm the published photo matches.

### Tests for User Story 3 ⚠️ Write first, confirm they fail

- [X] T069 [P] [US3] Unit tests in `crates/core/tests/frame.rs` for `default_frame`: no crop when the aspect already matches, a 1:3 top/bottom split when cropping a portrait vertically, an even split when cropping horizontally, and a result always inside the image bounds (INV-6)
- [X] T070 [P] [US3] Test in `crates/core/tests/adjustments.rs` asserting `set_crop(None)` restores full frame and that rotation composes correctly with EXIF orientation
- [X] T071 [P] [US3] Source-immutability test in `crates/core/tests/source_untouched.rs` checksumming every file in `fixtures/inputs/portraits/` before and after a full crop-rotate-build cycle and asserting no change (FR-015)
- [X] T072 [US3] Benchmark harness in `crates/core/tests/portrait_benchmark.rs` running `default_frame` over all twenty photos in `fixtures/inputs/portraits/` with the expected head region annotated per photo, asserting the head survives in every case (SC-003)

### Implementation for User Story 3

- [X] T073 [US3] Implement crop geometry and the headroom-bias rule in `crates/core/src/photo/frame.rs` as pure arithmetic over dimensions, with no image decoding
- [X] T074 [US3] Implement `set_crop`, `rotate`, and `default_frame` in `crates/core/src/photo/mod.rs`, recording adjustments on the `Photo` without touching the source
- [X] T075 [US3] Apply rotation and crop in the processing pipeline in `crates/core/src/photo/mod.rs`, ordered after EXIF orientation and before scaling per [contracts/html-output.md](./contracts/html-output.md)
- [X] T076 [US3] Implement the Tauri commands `set_crop`, `rotate_photo`, and `suggest_crop` in `crates/desktop/src/commands/photo.rs`
- [X] T077 [US3] Build the crop overlay in `crates/desktop/ui/src/lib/CropOverlay.svelte`: a movable, resizable frame over the photo with a live result and confirm/revert actions
- [X] T078 [US3] Add rotate controls and a revert action to `crates/desktop/ui/src/lib/PhotoList.svelte`

**Checkpoint**: Portrait framing and in-app editing work. Quickstart scenario 3 passes.

---

## Phase 6: User Story 4 - Build an item from loose photos, no folder required (Priority: P4)

**Goal**: Photos arrive by drag-and-drop or clipboard paste and are arranged without nominating
a directory.

**Independent Test**: Build and publish a complete item using only drag-and-drop and clipboard
paste, selecting no folder anywhere.

### Tests for User Story 4 ⚠️ Write first, confirm they fail

- [X] T079 [P] [US4] Tests in `crates/core/tests/photo_management.rs` asserting that reordering and removal carry placements with them and never re-point a placement at a different photo (FR-009, INV-1)
- [X] T080 [P] [US4] Test in `crates/core/tests/duplicate_names.rs` asserting two photos with identical file names are both kept and published under distinct names (INV-5)
- [X] T081 [P] [US4] Test in `crates/core/tests/unsupported_input.rs` asserting a non-image file is rejected by name and adds nothing to the item (FR-011)
- [X] T082 [P] [US4] Test in `crates/core/tests/photo_sources.rs` asserting `PhotoSource::Path` and `PhotoSource::Bytes` converge on the same `add_photos` behaviour

### Implementation for User Story 4

- [X] T083 [US4] Implement `add_photos`, `remove_photo`, `reorder_photos`, and `rename_photo` in `crates/core/src/model/item.rs`, maintaining placement integrity and file-name uniqueness
- [X] T113 [US4] Implement `insert_placement`, `move_placement`, `remove_placement`, `add_to_placement`, and `set_layout` in `crates/core/src/model/item.rs` per [contracts/core-api.md](./contracts/core-api.md), including row promotion and demotion
- [X] T114 [US4] Implement the Tauri commands `insert_placement`, `move_placement`, `remove_placement`, `add_to_placement`, and `set_layout` in `crates/desktop/src/commands/placement.rs`
- [X] T084 [US4] Implement format sniffing and rejection with a named reason in `crates/core/src/photo/mod.rs` for the formats listed in FR-011
- [X] T085 [US4] Implement the Tauri commands `add_photos_from_paths`, `add_photo_from_clipboard`, `remove_photo`, `reorder_photos`, and `rename_photo` in `crates/desktop/src/commands/photo.rs`
- [X] T086 [US4] Wire Tauri's `onDragDropEvent` in `crates/desktop/ui/src/routes/+page.svelte`, forwarding the supplied paths — HTML5 drag-and-drop is not used (research R8)
- [X] T087 [US4] Wire clipboard image paste via `tauri-plugin-clipboard-manager` in `crates/desktop/ui/src/lib/PhotoList.svelte`
- [X] T088 [US4] Add reorder, rename, remove, and used/unused indication to `crates/desktop/ui/src/lib/PhotoList.svelte` (FR-010)
- [X] T109 [US4] Implement cursor insertion in `crates/desktop/ui/src/lib/Editor.svelte`: with the caret between paragraphs, choosing a photo from the list calls `insert_placement` at that point (FR-018a)
- [X] T110 [US4] Implement dragging a photo from the photo list into the text in `crates/desktop/ui/src/lib/Editor.svelte`, showing the insertion point during hover, and dropping onto an existing card calling `add_to_placement` to build a row (FR-018a, FR-018c)
- [X] T111 [US4] Implement dragging a placement card to another point in the text in `crates/desktop/ui/src/lib/PlacementCard.svelte`, calling `move_placement` (FR-018d)
- [X] T112 [P] [US4] Tests in `crates/core/tests/placement_editing.rs` for `insert_placement`, `move_placement`, `remove_placement`, `add_to_placement`, and `set_layout`: row promotion and demotion (INV-4), emptied placements dropped, and INV-1 preserved throughout

**Checkpoint**: An item can be built end to end from loose photos. Quickstart scenario 4 passes.

---

## Phase 7: User Story 5 - Arrange the photos for me (Priority: P5)

**Goal**: One action distributes unplaced photos through the text and picks a layout for each,
with every choice overridable.

**Independent Test**: Trigger automatic arrangement on an item with unplaced photos; every photo
is placed, paragraph boundaries are respected, and any placement can be changed afterwards.

### Tests for User Story 5 ⚠️ Write first, confirm they fail

- [X] T089 [P] [US5] Tests in `crates/core/tests/arrange.rs` asserting every photo receives a placement, that surplus photos group into `Row` placements rather than being dropped, and that layout varies with photo orientation
- [X] T090 [P] [US5] Determinism test in `crates/core/tests/arrange_determinism.rs` asserting the same item and options always produce the identical arrangement
- [X] T091 [P] [US5] Test in `crates/core/tests/arrange_manual_guard.rs` asserting `replace_manual: false` preserves manual placements and reports `replaced_manual`, and that `set_placement` changes exactly one placement

### Implementation for User Story 5

- [X] T092 [US5] Implement photo distribution across paragraph boundaries in `crates/core/src/arrange/distribute.rs`
- [X] T093 [US5] Implement shape-based layout selection and row grouping in `crates/core/src/arrange/layout.rs`
- [X] T094 [US5] Implement `arrange_auto` and `set_placement` in `crates/core/src/arrange/mod.rs`, returning `ArrangeReport { placed, replaced_manual }`
- [X] T095 [US5] Implement the Tauri commands `arrange_auto` and `set_placement` in `crates/desktop/src/commands/arrange.rs`
- [X] T096 [US5] Add the arrange action and the overwrite confirmation to `crates/desktop/ui/src/lib/Editor.svelte`, calling with `replace_manual: false` first and asking before retrying with `true` (FR-020)
- [X] T097 [US5] Verify that per-placement override after automatic arrangement works through the existing placement card controls from T107, adding nothing new to the UI (FR-021)

**Checkpoint**: All five stories are independently functional. Quickstart scenario 5 passes.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T098 [P] Write `README.md` documenting every CLI flag, the four markers, and the appearance configuration keys, as the constitution's workflow section requires before release
- [X] T099 [P] Add the opt-in end-to-end publish test in `crates/core/tests/e2e_sshd.rs`, gated on an environment variable, running against a local `sshd` (research R10) — found and fixed a real upload defect, see below
- [X] T100 [P] Add the interface-parity test in `crates/cli/tests/parity_with_desktop.rs` publishing one prepared item through both entry points and asserting identical fragments with the slug normalised (FR-039, SC-011)
- [X] T101 Audit every `Error` and `Warning` message against SC-007, confirming each names the specific offending file, marker, or photo, in `crates/core/src/error.rs` — closed one gap (`DocumentUnreadable` named no file) and left the audit behind as an exhaustive test
- [X] T102 [P] Profile and tune preview rebuild latency in `crates/core/src/build.rs` and `crates/desktop/src/state.rs` against the SC-008 targets: 150 ms after a text edit, under 1 s for a 30-photo rebuild — both were missed by 4x on the first measurement; now 393 µs and 513 ms
- [X] T103 [P] Add unsaved-changes protection on window close in `crates/desktop/src/main.rs` (edge case)
- [X] T104 [P] Configure Tauri capabilities in `crates/desktop/capabilities/default.json` granting only filesystem read for chosen paths and the thumbnail cache, clipboard, and dialog — no HTTP client, no shell
- [X] T105 [P] Configure packaging in `crates/desktop/tauri.conf.json` for Windows and Linux installers, with icons in `crates/desktop/icons/`
- [X] T106 Run every scenario in [quickstart.md](./quickstart.md) end to end and record the results, in [quickstart-results.md](./quickstart-results.md)
- [ ] T106a **The SC-005 observation session — outstanding.** An editor who has not seen the application, publishing their first item unaided and unhelped. Not a command anyone can run; it needs a person and a scheduled sitting. Record the result in [quickstart-results.md](./quickstart-results.md)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: Depends on Setup — **blocks all user stories**. T007→T008→T009 is a strict chain; the goldens must exist before any domain code is written (constitution III)
- **User Stories (Phases 3–7)**: All depend on Foundational. In priority order P1 → P5, or in parallel with enough people
- **Polish (Phase 8)**: Depends on the stories you intend to ship

### User Story Dependencies

- **US1 (P1)**: Depends only on Foundational. No dependency on any other story
- **US2 (P2)**: Depends only on Foundational. Independently testable — it can publish an item built by any means, including a fixture item constructed in a test
- **US3 (P3)**: Depends only on Foundational. The crop pipeline is testable without import or publish
- **US4 (P4)**: Depends only on Foundational. Shares `PhotoList.svelte` with US3, so those two conflict on that one file if worked simultaneously
- **US5 (P5)**: Depends only on Foundational. Shares `Editor.svelte` with US1

The two file conflicts above are the only cross-story coupling. Everything else is independent.

### Within Each User Story

- Tests are written first and must fail before implementation begins
- Model changes before services, services before commands, commands before UI
- The story's checkpoint is reached when its quickstart scenario passes

### Parallel Opportunities

- Phase 1: T002–T006 all run in parallel after T001
- Phase 2: T010–T013 in parallel; T014–T017 in parallel; T007/T008 in parallel with all of them
- Phase 3: T021–T027 (seven tests) in parallel; then T028–T031 and T036 in parallel
- Phase 4: T047–T054 (eight tests) in parallel; then T055, T056 in parallel
- Phase 5: T069–T071 in parallel
- Phase 6: T079–T082 in parallel
- Phase 7: T089–T091 in parallel
- Phase 8: everything except T101 and T106 in parallel. In the event T101 and T102 both touched `core`, and T099 changed `adapters/transport.rs`, so they were run in sequence

---

## Parallel Example: User Story 1

```bash
# Write all seven US1 tests together, then confirm every one fails:
Task: "Golden test for the four layouts in crates/core/tests/parity_render.rs"
Task: "Golden test for natural-sort ordering in crates/core/tests/parity_ordering.rs"
Task: "Marker parsing tests in crates/core/tests/markers.rs"
Task: "Title detection tests in crates/core/tests/title_detection.rs"
Task: "DOCX import test in crates/core/tests/import_docx.rs"
Task: "Inline-styles-only contract test in crates/core/tests/html_contract.rs"
Task: "Determinism test in crates/core/tests/determinism.rs"

# Then the independent implementation pieces:
Task: "Marker parser in crates/core/src/markers/mod.rs"
Task: "Natural-sort keys in crates/core/src/model/photo.rs"
Task: "Plain-text and Markdown import in crates/core/src/import/plain.rs"
Task: "Title detection in crates/core/src/import/title.rs"
Task: "EXIF orientation in crates/core/src/photo/orient.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 Setup — T001–T006
2. Phase 2 Foundational — T007–T020. Do not skip the fixture capture; it is what makes every
   later parity claim checkable
3. Phase 3 User Story 1 — T021–T046
4. **Stop and validate**: run quickstart scenario 1

At that point the tool already removes the biggest time sink in the current workflow — manually
exporting photos out of Word — even though it cannot publish yet.

### Incremental Delivery

1. Setup + Foundational → the parity gate exists
2. US1 → Word import and preview → **MVP**
3. US2 → publishing → the tool fully replaces the Python script
4. US3 → portrait framing → the second image editor goes away
5. US4 → loose-photo intake → the last folder dependency goes away
6. US5 → automatic arrangement → convenience on top

Stories 3, 4, and 5 are the new value from `task.md`; 1 and 2 are the port. Shipping in this
order means the Python tool can be retired after US2, with everything after that a pure gain.

### Parallel Team Strategy

Setup and Foundational are shared work. After that: one person on US1, one on US2, one on
US3 + US4 (they share `PhotoList.svelte`). US5 last, since it depends on the layouts US1 proved.

---

## Notes

- Every task names an exact file path
- `[P]` means a different file with no incomplete dependency
- **Task IDs are labels, not execution order — the file order is.** T107–T114 were added after the
  initial pass, when marker typing was dropped in favour of inline placement cards (deviation
  D-9). They sit physically in the phase they belong to; their numbers are simply the next free
  ones, so the workspace was not churned by renumbering a hundred tasks
- Constitution III makes the test tasks mandatory, not optional — verify each fails before
  implementing
- Commit after each task or logical group; the four merge gates in the `justfile` must be green
- Any behaviour that diverges from the Python reference must be added to the plan's declared
  deviations list before it is merged (constitution I)

## Implementation notes (2026-09-01)

**Where the tests actually live.** T023, T024, T025, T069 and T070 named dedicated files under
`crates/core/tests/`. Their assertions are delivered, but co-located with the code they cover
(`markers/mod.rs`, `import/title.rs`, `photo/frame.rs`) or folded into the parity suite, which
compares against the captured goldens rather than against hand-written expectations —
constitution III prefers the latter wherever a golden exists. The integration files that do
exist are:

| File | Covers |
|------|--------|
| `tests/parity_render.rs` | T021, T026, T027 — fragment bytes, inline-styles-only, determinism |
| `tests/parity_tables.rs` | T022, T047 — natural sort, transliteration, normalisation, the built-in appearance, preset loading |
| `tests/parity_text.rs` | T024, T025 — title detection and body normalisation for every case, plus the two real Word documents |
| `tests/parity_paths.rs` | T048 |
| `tests/parity_encode.rs` | T049 |
| `tests/model_invariants.rs` | T018 |

**T020 is only half done.** Module declarations are wired and the crate compiles; the `tracing`
subscriber is not initialised. That belongs in the two frontends rather than in a library, which
should not install a global subscriber on its users' behalf — worth confirming before closing.

**T059 is written but unproven.** `core::publish` implements resolution, early refusal,
convergence and slug suffixing, but T013's in-memory fakes do not exist yet, so T050–T054 have
not run against it. Treat the publish path as unverified until they do.

## Implementation notes (2026-09-02)

**T059 is now proven.** T013's fakes exist in `crates/core/tests/support/fakes.rs`
(`FakeFileStore`, `RecordingTransport`, `FakeSecretStore`, `FixedClock`, plus a `server_config`
helper pointing at the same base path and public URL the goldens were captured under), and
T050–T054 run against them — 14 tests, all green. `core::publish` needed no changes to pass
them, which is what "unverified" meant rather than "wrong".

| File | Covers |
|------|--------|
| `tests/publish_converge.rs` | T050 — first publish, zero-write re-publish, orphan left in place |
| `tests/publish_dryrun.rs` | T051 — no mutating call, full planned URL set, convergence in the plan |
| `tests/publish_refusals.rs` | T052 — no credential, empty credential, unreachable store, unusable base path, collision |
| `tests/no_secret_leak.rs` | T053 — success path, failing-upload path, refusal path, plus a control |
| `tests/publish_warnings.rs` | T054 — `missing-photo` and `unused-photo` |

Three things worth recording about how the tests had to be written:

- **The dropped photo in T050 is the *last* one.** Published names carry the photo's position in
  the item, so dropping one from the middle renumbers everything after it, and the test would
  measure renaming rather than deletion. The comment in the test says so.
- **T052's third case is not a refusal.** The task predates deviation D-8: a slug colliding with
  a different item's folder is *suffixed*, not rejected. The test asserts the suffixing, the
  `SlugSuffixed` warning, and that the other item's folder is neither added to nor overwritten.
- **`support::item(case)`** was extracted from `build_case` so the publish tests can get at the
  item itself. `build_case` now calls it; the parity suite is unchanged and still green.

`T013`'s remaining consumer is the CLI/desktop adapter work. T100's interface-parity test ended
up carrying its own small fakes rather than reusing these: it lives in `crates/cli/tests/`, which
cannot see `crates/core/tests/support/`.

## Implementation notes (2026-09-03)

**All eight phases are complete.** 114 of 114 tasks are done. One acceptance criterion remains
open and is tracked as T106a: SC-005 needs an editor who has not seen the application, sat in
front of it and watched, which is not something any test run can supply.

Phase 8 found two real defects, both in code that every test until then had been passing:

- **Every first publish would have failed.** `russh_sftp`'s `SftpSession::write` opens with
  `OpenFlags::WRITE` alone, so it cannot write a file that does not already exist — and would
  have left the tail of a longer file behind when overwriting a shorter one. `SftpTransport::put`
  now uses `create` (`CREATE | TRUNCATE | WRITE`) and closes the handle. Found by T099 on its
  first run; `RecordingTransport` cannot see this class of bug, which is the whole argument for
  keeping an end-to-end test around.
- **The preview was roughly four times slower than SC-008 allows**, in both directions: a text
  edit cost 3.9 s and a cold thirty-photo rebuild 3.88 s, because each rebuild re-encoded every
  placed photo. T102 added `core::build_with_cache` (a text edit now re-encodes nothing: 393 µs)
  and spread the photos that genuinely need encoding across threads (cold rebuild: 513 ms).
  `preview_latency.rs` asserts a cached rebuild is byte-for-byte an uncached one, so the
  optimisation cannot change output without the test noticing.

T101 also closed an SC-007 gap: `Error::DocumentUnreadable` never named the offending file,
because import is handed bytes and does not know it. `Error::in_file` attaches the name at the
frontends, which do, and `error.rs` now carries an exhaustive audit that will not compile when
a variant is added without stating what that variant's offending item is.

Two errors in quickstart.md itself were found by running it and were corrected: it named a
fixture `news.docx` that is called `input.docx`, and it expected exit 4 for a photo over budget
at the quality floor, which FR-034 makes a warning the build survives. The full run-through is
recorded in [quickstart-results.md](./quickstart-results.md).

**The domain crate is finished and proven.** US1–US5 all have their rules in `crates/core`,
with 344 tests green across the workspace and all four merge gates clean. `arrange` was the
last piece: `distribute.rs` and `layout.rs` existed, `mod.rs` did not, so T089–T091 had been
written and could not compile.

**What is built but still not exercised through the window.** The desktop application compiles,
links, starts without error, and its frontend type-checks and bundles (402 files, 0 errors; 12
frontend tests green). It has still **not** been driven by hand — no document opened through the
window, no crop dragged, nothing published from it — because doing so needs a display. Every
rule underneath those interactions is covered by tests that were run, and the mapping from each
quickstart scenario to the tests standing in for it is in
[quickstart-results.md](./quickstart-results.md). The CLI, by contrast, has been
run against the fixtures: `newsbuilder build` reproduces `fixtures/reference/markers/fragment.html`
byte for byte, and exit codes 0, 1, 2 and 3 were each observed on the path that should produce
them.

**Where the adapters went.** `core::adapters` now holds the three real port implementations —
`files` (a `FileStore` over `std::fs`), `keyring_store`, and `transport` behind the non-default
`sftp` feature. The reasoning, and the four other decisions this phase forced, are recorded as
deviations D-15 to D-20 in plan.md. The parity suite still compiles with no `tokio` and no
network stack in its dependency graph, which was the point.

| File | Covers |
|------|--------|
| `crates/core/src/arrange/mod.rs` | T094 — `arrange_auto`, `set_placement` |
| `crates/core/src/adapters/` | T060, T061, and the `FileStore` no task named |
| `crates/core/src/model/server_store.rs` | T062 |
| `crates/cli/src/exit.rs` | T064 — the six exit codes and the `--json` documents |
| `crates/cli/src/main.rs` | T040, T046, T063 |
| `crates/desktop/src/` | T005, T041, T042, T065, T068, T076, T085, T095, T114 |
| `crates/desktop/ui/src/lib/body.ts` + `body.test.ts` | T108 — the round trip, 12 tests |
| `crates/desktop/ui/src/` | T043–T045, T066, T067, T077, T078, T086–T088, T096, T097, T107, T109–T111 |
| `crates/core/src/error.rs` | T010, T101 — the SC-007 audit and `in_file` |
| `crates/core/src/build.rs` + `crates/desktop/src/state.rs` | T102 — `ProcessCache` and the parallel encode |
| `crates/core/tests/preview_latency.rs` | T102 — the SC-008 property and the measurements |
| `crates/core/tests/e2e_sshd.rs` | T099 — the opt-in publish against a real `sshd` |
| `crates/cli/tests/parity_with_desktop.rs` | T100 — FR-039 and SC-011 |
| `README.md` | T098 |
| `quickstart-results.md` | T106 |
