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

- [ ] T001 Create workspace `Cargo.toml` at repository root with members `crates/core`, `crates/cli`, `crates/desktop`, `tools/capture-reference`, and a `[workspace.dependencies]` table pinning the versions from plan.md
- [ ] T002 [P] Pin the toolchain in `rust-toolchain.toml` (stable channel, `rustfmt` and `clippy` components, edition 2024)
- [ ] T003 [P] Add crate-level lint denials to `crates/core/src/lib.rs`: `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::exit)]` per constitution principle V
- [ ] T004 [P] Add `rustfmt.toml` and a `justfile` at repository root exposing the four merge gates: `fmt --check`, `clippy --all-targets -- -D warnings`, `test --workspace`, and the parity fixture suite
- [ ] T005 [P] Scaffold the Tauri application in `crates/desktop/`: `tauri.conf.json` (window with `dragDropEnabled: true`), `src/main.rs`, and `ui/` with Vite + Svelte 5 + TypeScript
- [ ] T006 [P] Create `.gitignore`, `fixtures/inputs/`, `fixtures/reference/`, and `tools/capture-reference/` directory skeletons

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The parity fixtures, the port traits, and the shared model. Constitution III forbids
writing domain code before the fixtures exist, so T007–T009 genuinely block everything.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Parity fixtures (constitution III — must precede all domain code)

- [ ] T007 Assemble the input corpus in `fixtures/inputs/`: `word-with-photos/` (a `.docx` with inline images), `word-anchored/` (floating images), `markers/` (a `.txt` using all four markers), `cyrillic-title/`, `natural-sort/` (photos named `photo1..photo12`), `missing-photo/`, `unused-photo/`, `oversized/`, `portraits/` (20 portrait photographs for the SC-003 benchmark), `mixed-orientation/`
- [ ] T008 Implement `tools/capture-reference/src/main.rs`: clone `LapunovRodion/news-builder` at a pinned commit into a cache directory, create a virtualenv, install `requirements-news-builder.txt`, run `news_builder.py` over each `fixtures/inputs/*` case with the upload step stubbed, and write fragments plus warning text into `fixtures/reference/<case>/`
- [ ] T009 Run the harness and commit the generated goldens under `fixtures/reference/`, recording the pinned reference commit in `fixtures/reference/PINNED_COMMIT`

### Core scaffolding

- [ ] T010 [P] Define typed errors and warnings in `crates/core/src/error.rs` per [contracts/core-api.md](./contracts/core-api.md): `Error` with all nine variants and `Warning` with all five, each naming the offending item
- [ ] T011 [P] Implement the redacting `Secret` newtype in `crates/core/src/secret.rs` with `Debug`/`Display` printing `[redacted]` and a single `expose()` accessor, plus tests in the same file asserting both formatters redact
- [ ] T012 [P] Define the port traits `FileStore`, `Transport`, `SecretStore`, and `Clock` in `crates/core/src/ports.rs` exactly as specified in [contracts/core-api.md](./contracts/core-api.md)
- [ ] T013 [P] Implement in-memory fakes for all four ports in `crates/core/tests/support/fakes.rs`, with `RecordingTransport` capturing an ordered log of every operation for the convergence and dry-run assertions

### Model

- [ ] T014 [P] Implement `NewsItem`, `Block`, `ParagraphKind`, and `Layout` in `crates/core/src/model/item.rs` per [data-model.md](./data-model.md)
- [ ] T015 [P] Implement `Photo`, `PhotoId`, `PhotoOrigin`, `PhotoSource`, `Adjustments`, `CropRect`, and `NaturalKey` in `crates/core/src/model/photo.rs`
- [ ] T016 [P] Implement `Appearance` and `StyleSet` in `crates/core/src/model/appearance.rs`, with the built-in appearance reproducing the reference's `DEFAULT_STYLES` and `DEFAULT_CONFIG` values exactly
- [ ] T017 [P] Implement `ServerConfig`, `ServerConfigRef`, `CredentialRef`, and the `Slug` newtype in `crates/core/src/model/server.rs`
- [ ] T018 Write invariant tests in `crates/core/tests/model_invariants.rs` covering INV-1 through INV-8 from data-model.md (placement references resolve, slug shape, unique photo ids, layout/count agreement, unique file names, crop within bounds, quality bounds, credential presence)
- [ ] T019 Implement the appearance configuration loader and validation in `crates/core/src/model/appearance_config.rs` per [contracts/appearance-config.md](./contracts/appearance-config.md), including a test that every `style-presets/*.json` file from the reference loads unmodified
- [ ] T020 [P] Initialise `tracing` in `crates/core/src/lib.rs` with a subscriber configuration that cannot format a `Secret`, and wire module declarations so the crate compiles

**Checkpoint**: Goldens committed, ports faked, model in place — user stories can begin

---

## Phase 3: User Story 1 - Word file in, laid-out page out (Priority: P1) 🎯 MVP

**Goal**: Open a `.docx` and immediately see the finished news page — text formatted, every photo
placed where it stood in the document, with no manual photo export and no folder selection.

**Independent Test**: Open a `.docx` with text and embedded photos; the preview shows the same
text and the same photo order and positions as the source, and no step asked for a folder.

### Tests for User Story 1 ⚠️ Write first, confirm they fail

- [ ] T021 [P] [US1] Golden test in `crates/core/tests/parity_render.rs` comparing rendered fragments against `fixtures/reference/markers/` for each of the four layouts, byte for byte
- [ ] T022 [P] [US1] Golden test in `crates/core/tests/parity_ordering.rs` asserting natural-sort photo ordering against `fixtures/reference/natural-sort/`
- [ ] T023 [P] [US1] Unit tests in `crates/core/tests/markers.rs` for marker parsing: all four forms, whitespace variants, multi-index lists, malformed markers, and a marker naming a missing photo producing `Warning::MissingPhoto`
- [ ] T024 [P] [US1] Unit tests in `crates/core/tests/title_detection.rs` covering a headline as first line, a Markdown `#` heading, a `.docx` Heading-styled paragraph, and a document with no detectable title yielding `None`
- [ ] T025 [P] [US1] Integration test in `crates/core/tests/import_docx.rs` asserting that `fixtures/inputs/word-with-photos/news.docx` yields paragraphs in document order, one `Photo` per embedded image with `origin: Embedded`, and a `Placement` at each image's document position
- [ ] T026 [P] [US1] Contract test in `crates/core/tests/html_contract.rs` asserting the rendered fragment contains no `<style>`, no `<link>`, no `class=`, no `<script>`, and no `<!doctype>`/`<html>`/`<body>`
- [ ] T027 [P] [US1] Determinism test in `crates/core/tests/determinism.rs` building the same item twice in one process and once after a shuffle of internal insertion order, asserting byte-identical fragments

### Implementation for User Story 1

- [ ] T028 [P] [US1] Implement the marker language parser and emitter in `crates/core/src/markers/mod.rs`, reproducing the reference's `MARKER_PATTERN` semantics including single-photo `Row` normalisation to `FullWidth`
- [ ] T029 [P] [US1] Implement natural-sort key construction in `crates/core/src/model/photo.rs`, reproducing the reference's `natural_sort_key` so `photo2` precedes `photo10`
- [ ] T030 [P] [US1] Implement plain-text and Markdown import in `crates/core/src/import/plain.rs`, including paragraph splitting and body normalisation
- [ ] T031 [P] [US1] Implement title detection in `crates/core/src/import/title.rs` for all three source formats, returning `None` rather than inventing a title
- [ ] T032 [US1] Implement the DOCX package reader in `crates/core/src/import/docx.rs` using `zip` and `quick-xml`: walk `word/document.xml` for paragraph text in order, resolve `w:drawing` → `a:blip/@r:embed` against `word/_rels/document.xml.rels`, and extract the referenced `word/media/*` bytes
- [ ] T033 [US1] Map extracted media to `EmbeddedMedia { after_paragraph }` and convert those positions into `Placement` blocks in `crates/core/src/import/docx.rs` (FR-003)
- [ ] T034 [US1] Emit `Warning::UnsupportedDocumentFeature` for out-of-scope Word constructs (tables, footnotes, comments, tracked changes) in `crates/core/src/import/docx.rs` rather than failing (FR-034)
- [ ] T035 [US1] Implement `import_document` in `crates/core/src/import/mod.rs` dispatching on `SourceFormat` and returning `Imported { title, item, warnings }`
- [ ] T036 [P] [US1] Implement EXIF orientation reading and application in `crates/core/src/photo/orient.rs` using `kamadak-exif`, as an explicit transform over `image` types
- [ ] T037 [US1] Implement decode, scale-down, and thumbnail generation in `crates/core/src/photo/mod.rs` — never scaling up, and caching thumbnails by photo id and adjustment hash
- [ ] T038 [US1] Implement the inline-style HTML renderer in `crates/core/src/render/mod.rs` per [contracts/html-output.md](./contracts/html-output.md): container, title, lead and body paragraphs, the four placement forms, clearing elements, and HTML escaping
- [ ] T039 [US1] Implement the `build` pipeline in `crates/core/src/build.rs` returning `BuildOutput { fragment, processed, warnings }`, consulting no port so the build is offline
- [ ] T040 [US1] Implement `newsbuilder build` in `crates/cli/src/main.rs` with `--input`, `--images-dir`, `--output`, `--title`, `--news-slug`, `--public-base-url`, `--appearance`, and `--json` per [contracts/cli.md](./contracts/cli.md)
- [ ] T041 [US1] Implement the Tauri commands `open_document`, `new_item`, `set_title`, `set_body_text`, `set_slug`, and `build_preview` in `crates/desktop/src/commands/item.rs` per [contracts/desktop-commands.md](./contracts/desktop-commands.md)
- [ ] T042 [US1] Implement `ItemView`/`PhotoView` projection and the managed session state holding the open `NewsItem` in `crates/desktop/src/state.rs`, serving thumbnails over the asset protocol so full-size bytes never cross the IPC boundary
- [ ] T043 [P] [US1] Build the editor screen in `crates/desktop/ui/src/lib/Editor.svelte`: text area, title field, and the photo list showing document order
- [ ] T044 [US1] Build the preview pane in `crates/desktop/ui/src/lib/Preview.svelte`, injecting the exact string returned by `build_preview` into a sandboxed iframe with no templating, post-processing, or re-styling
- [ ] T045 [US1] Wire live rebuild on edit in `crates/desktop/ui/src/routes/+page.svelte`, debounced so a text edit refreshes the preview within 150 ms
- [ ] T046 [US1] Add the CLI refusal path in `crates/cli/src/main.rs`: exit 3 with a message pointing at the desktop application when an item has photos but no placements and no markers to derive them from

**Checkpoint**: A Word file opens, lays out, previews, and exports a CMS-ready fragment. Quickstart scenario 1 passes. This is the MVP.

---

## Phase 4: User Story 2 - Publish photos and hand a fragment to the CMS (Priority: P2)

**Goal**: Upload processed photos into a per-item remote folder and produce the fragment with
public URLs.

**Independent Test**: Publish a prepared item to a test server; the remote folder is created, all
photos land in it, and the fragment references them by public URL.

### Tests for User Story 2 ⚠️ Write first, confirm they fail

- [ ] T047 [P] [US2] Golden test in `crates/core/tests/parity_slug.rs` covering the full Cyrillic transliteration table including `і ї є ў`, against `fixtures/reference/cyrillic-title/`
- [ ] T048 [P] [US2] Golden test in `crates/core/tests/parity_paths.rs` asserting remote path and public URL construction, including base values with and without trailing slashes
- [ ] T049 [P] [US2] Golden test in `crates/core/tests/parity_encode.rs` asserting the quality-step search reaches the same final size and quality as `fixtures/reference/oversized/`
- [ ] T050 [P] [US2] Convergence test in `crates/core/tests/publish_converge.rs` using `RecordingTransport`: publish twice unchanged and assert the second run issues zero `put` calls; then remove a photo from the item, publish again, and assert its remote file is still listed and untouched (FR-030, deviation D-7, SC-009)
- [ ] T051 [P] [US2] Dry-run test in `crates/core/tests/publish_dryrun.rs` asserting `PublishMode::DryRun` calls no mutating `Transport` method and returns `dry_run: true` with the full planned URL set
- [ ] T052 [P] [US2] Refusal tests in `crates/core/tests/publish_refusals.rs`: no credential, unwritable remote base path, and a slug colliding with a different item's folder — each rejected before any photo is processed
- [ ] T053 [P] [US2] Secret-leak test in `crates/core/tests/no_secret_leak.rs` running a full publish against fakes with a sentinel password, then asserting the sentinel appears in no log line, no error `Display`, no `Debug` output, and no written file (SC-010)
- [ ] T054 [P] [US2] Warning tests in `crates/core/tests/publish_warnings.rs` for `fixtures/inputs/missing-photo/` and `fixtures/inputs/unused-photo/`, asserting the build completes and unused photos are not uploaded

### Implementation for User Story 2

- [ ] T055 [P] [US2] Implement slug transliteration in `crates/core/src/publish/slug.rs`, reproducing the reference's `CYRILLIC_TRANSLIT` table exactly, then lowercasing, collapsing non-alphanumerics to single hyphens, and trimming
- [ ] T056 [P] [US2] Implement remote path and public URL construction in `crates/core/src/publish/paths.rs`, joining with exactly one slash regardless of trailing slashes
- [ ] T057 [US2] Implement quality-search encoding in `crates/core/src/photo/encode.rs`: encode, measure, step quality down toward the floor until the result fits `max_bytes`, emitting `SizeBudgetUnreachable` at the floor
- [ ] T058 [US2] Add lossy WebP encoding behind the `webp` cargo feature in `crates/core/src/photo/encode.rs`, preserving the source extension so published URLs match the reference
- [ ] T059 [US2] Implement publish orchestration in `crates/core/src/publish/mod.rs`: resolve credential, verify the remote base path, compute the desired remote file set, upload only what differs, and suffix a colliding slug — never deleting anything from the server (deviation D-7)
- [ ] T060 [US2] Implement the `russh` + `russh-sftp` transport adapter in `crates/desktop/src/adapters/transport.rs` and `crates/cli/src/adapters/transport.rs`, supporting both key and password authentication, sharing one implementation module
- [ ] T061 [US2] Implement the `keyring` secret-store adapter in `crates/core/src/adapters/keyring_store.rs`, reporting `available() == false` when no Secret Service is reachable
- [ ] T062 [US2] Implement server configuration persistence in `crates/core/src/model/server_store.rs`, writing connection settings to the OS config directory with the credential held only as a `CredentialRef`
- [ ] T063 [US2] Implement `newsbuilder publish` and the `newsbuilder server` subcommands in `crates/cli/src/main.rs`, reading secrets from a no-echo prompt or stdin and never from an argument
- [ ] T064 [US2] Implement the CLI exit-code mapping in `crates/cli/src/exit.rs` (0/1/2/3/4/5) and the `--json` result document per [contracts/cli.md](./contracts/cli.md)
- [ ] T065 [US2] Implement the Tauri commands `list_servers`, `save_server`, `set_credential`, `delete_credential`, `secret_store_available`, and `publish` in `crates/desktop/src/commands/publish.rs`, emitting `publish-progress` events off the UI thread
- [ ] T066 [P] [US2] Build the server settings screen in `crates/desktop/ui/src/lib/ServerSettings.svelte`, which never receives a secret back from the backend
- [ ] T067 [US2] Build the publish dialog in `crates/desktop/ui/src/lib/PublishDialog.svelte` with a dry-run option, per-file progress, and the resulting fragment offered for copying
- [ ] T068 [US2] Implement the per-session credential fallback in `crates/desktop/src/commands/publish.rs` and its prompt in the UI, used when `secret_store_available` is false, writing nothing to disk (FR-041)

**Checkpoint**: The full publish loop works from both interfaces. Quickstart scenario 2 passes.

---

## Phase 5: User Story 3 - Portrait photos that keep their heads (Priority: P3)

**Goal**: Portrait photos are framed sensibly by default and adjustable with a visual crop
control, with no separate image editor.

**Independent Test**: Add a portrait photo of a person, confirm the default framing retains the
head, adjust the crop in the app, and confirm the published photo matches.

### Tests for User Story 3 ⚠️ Write first, confirm they fail

- [ ] T069 [P] [US3] Unit tests in `crates/core/tests/frame.rs` for `default_frame`: no crop when the aspect already matches, a 1:3 top/bottom split when cropping a portrait vertically, an even split when cropping horizontally, and a result always inside the image bounds (INV-6)
- [ ] T070 [P] [US3] Test in `crates/core/tests/adjustments.rs` asserting `set_crop(None)` restores full frame and that rotation composes correctly with EXIF orientation
- [ ] T071 [P] [US3] Source-immutability test in `crates/core/tests/source_untouched.rs` checksumming every file in `fixtures/inputs/portraits/` before and after a full crop-rotate-build cycle and asserting no change (FR-015)
- [ ] T072 [US3] Benchmark harness in `crates/core/tests/portrait_benchmark.rs` running `default_frame` over all twenty photos in `fixtures/inputs/portraits/` with the expected head region annotated per photo, asserting the head survives in every case (SC-003)

### Implementation for User Story 3

- [ ] T073 [US3] Implement crop geometry and the headroom-bias rule in `crates/core/src/photo/frame.rs` as pure arithmetic over dimensions, with no image decoding
- [ ] T074 [US3] Implement `set_crop`, `rotate`, and `default_frame` in `crates/core/src/photo/mod.rs`, recording adjustments on the `Photo` without touching the source
- [ ] T075 [US3] Apply rotation and crop in the processing pipeline in `crates/core/src/photo/mod.rs`, ordered after EXIF orientation and before scaling per [contracts/html-output.md](./contracts/html-output.md)
- [ ] T076 [US3] Implement the Tauri commands `set_crop`, `rotate_photo`, and `suggest_crop` in `crates/desktop/src/commands/photo.rs`
- [ ] T077 [US3] Build the crop overlay in `crates/desktop/ui/src/lib/CropOverlay.svelte`: a movable, resizable frame over the photo with a live result and confirm/revert actions
- [ ] T078 [US3] Add rotate controls and a revert action to `crates/desktop/ui/src/lib/PhotoList.svelte`

**Checkpoint**: Portrait framing and in-app editing work. Quickstart scenario 3 passes.

---

## Phase 6: User Story 4 - Build an item from loose photos, no folder required (Priority: P4)

**Goal**: Photos arrive by drag-and-drop or clipboard paste and are arranged without nominating
a directory.

**Independent Test**: Build and publish a complete item using only drag-and-drop and clipboard
paste, selecting no folder anywhere.

### Tests for User Story 4 ⚠️ Write first, confirm they fail

- [ ] T079 [P] [US4] Tests in `crates/core/tests/photo_management.rs` asserting that reordering and removal carry placements with them and never re-point a placement at a different photo (FR-009, INV-1)
- [ ] T080 [P] [US4] Test in `crates/core/tests/duplicate_names.rs` asserting two photos with identical file names are both kept and published under distinct names (INV-5)
- [ ] T081 [P] [US4] Test in `crates/core/tests/unsupported_input.rs` asserting a non-image file is rejected by name and adds nothing to the item (FR-011)
- [ ] T082 [P] [US4] Test in `crates/core/tests/photo_sources.rs` asserting `PhotoSource::Path` and `PhotoSource::Bytes` converge on the same `add_photos` behaviour

### Implementation for User Story 4

- [ ] T083 [US4] Implement `add_photos`, `remove_photo`, `reorder_photos`, and `rename_photo` in `crates/core/src/model/item.rs`, maintaining placement integrity and file-name uniqueness
- [ ] T084 [US4] Implement format sniffing and rejection with a named reason in `crates/core/src/photo/mod.rs` for the formats listed in FR-011
- [ ] T085 [US4] Implement the Tauri commands `add_photos_from_paths`, `add_photo_from_clipboard`, `remove_photo`, `reorder_photos`, and `rename_photo` in `crates/desktop/src/commands/photo.rs`
- [ ] T086 [US4] Wire Tauri's `onDragDropEvent` in `crates/desktop/ui/src/routes/+page.svelte`, forwarding the supplied paths — HTML5 drag-and-drop is not used (research R8)
- [ ] T087 [US4] Wire clipboard image paste via `tauri-plugin-clipboard-manager` in `crates/desktop/ui/src/lib/PhotoList.svelte`
- [ ] T088 [US4] Add reorder, rename, remove, and used/unused indication to `crates/desktop/ui/src/lib/PhotoList.svelte` (FR-010)

**Checkpoint**: An item can be built end to end from loose photos. Quickstart scenario 4 passes.

---

## Phase 7: User Story 5 - Arrange the photos for me (Priority: P5)

**Goal**: One action distributes unplaced photos through the text and picks a layout for each,
with every choice overridable.

**Independent Test**: Trigger automatic arrangement on an item with unplaced photos; every photo
is placed, paragraph boundaries are respected, and any placement can be changed afterwards.

### Tests for User Story 5 ⚠️ Write first, confirm they fail

- [ ] T089 [P] [US5] Tests in `crates/core/tests/arrange.rs` asserting every photo receives a placement, that surplus photos group into `Row` placements rather than being dropped, and that layout varies with photo orientation
- [ ] T090 [P] [US5] Determinism test in `crates/core/tests/arrange_determinism.rs` asserting the same item and options always produce the identical arrangement
- [ ] T091 [P] [US5] Test in `crates/core/tests/arrange_manual_guard.rs` asserting `replace_manual: false` preserves manual placements and reports `replaced_manual`, and that `set_placement` changes exactly one placement

### Implementation for User Story 5

- [ ] T092 [US5] Implement photo distribution across paragraph boundaries in `crates/core/src/arrange/distribute.rs`
- [ ] T093 [US5] Implement shape-based layout selection and row grouping in `crates/core/src/arrange/layout.rs`
- [ ] T094 [US5] Implement `arrange_auto` and `set_placement` in `crates/core/src/arrange/mod.rs`, returning `ArrangeReport { placed, replaced_manual }`
- [ ] T095 [US5] Implement the Tauri commands `arrange_auto` and `set_placement` in `crates/desktop/src/commands/arrange.rs`
- [ ] T096 [US5] Add the arrange action and the overwrite confirmation to `crates/desktop/ui/src/lib/Editor.svelte`, calling with `replace_manual: false` first and asking before retrying with `true` (FR-020)
- [ ] T097 [US5] Add per-placement layout override controls to `crates/desktop/ui/src/lib/Editor.svelte` (FR-021)

**Checkpoint**: All five stories are independently functional. Quickstart scenario 5 passes.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T098 [P] Write `README.md` documenting every CLI flag, the four markers, and the appearance configuration keys, as the constitution's workflow section requires before release
- [ ] T099 [P] Add the opt-in end-to-end publish test in `crates/core/tests/e2e_sshd.rs`, gated on an environment variable, running against a local `sshd` (research R10)
- [ ] T100 [P] Add the interface-parity test in `crates/cli/tests/parity_with_desktop.rs` publishing one prepared item through both entry points and asserting identical fragments with the slug normalised (FR-039, SC-011)
- [ ] T101 Audit every `Error` and `Warning` message against SC-007, confirming each names the specific offending file, marker, or photo, in `crates/core/src/error.rs`
- [ ] T102 [P] Profile and tune preview rebuild latency in `crates/core/src/build.rs` and `crates/desktop/src/state.rs` against the SC-008 targets: 150 ms after a text edit, under 1 s for a 30-photo rebuild
- [ ] T103 [P] Add unsaved-changes protection on window close in `crates/desktop/src/main.rs` (edge case)
- [ ] T104 [P] Configure Tauri capabilities in `crates/desktop/capabilities/default.json` granting only filesystem read for chosen paths and the thumbnail cache, clipboard, and dialog — no HTTP client, no shell
- [ ] T105 [P] Configure packaging in `crates/desktop/tauri.conf.json` for Windows and Linux installers, with icons in `crates/desktop/icons/`
- [ ] T106 Run every scenario in [quickstart.md](./quickstart.md) end to end and record the results, including the SC-005 observation session with an editor who has not seen the application

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
- Phase 8: everything except T101 and T106 in parallel

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
- Constitution III makes the test tasks mandatory, not optional — verify each fails before
  implementing
- Commit after each task or logical group; the four merge gates in the `justfile` must be green
- Any behaviour that diverges from the Python reference must be added to the plan's declared
  deviations list before it is merged (constitution I)
