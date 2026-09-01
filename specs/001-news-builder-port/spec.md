# Feature Specification: News Builder Port with Word Import and In-App Photo Editing

**Feature Branch**: `001-news-builder-port`

**Created**: 2026-09-01

**Status**: Draft

**Input**: User description: "добавил task.md сделай его красиввее и ориентируйся на него + репо который я скинул"

Source material: `.specify/task.md` and the reference implementation at
`https://github.com/LapunovRodion/news-builder`.

## User Scenarios & Testing *(mandatory)*

The actor throughout is an **editor**: a non-technical person who receives a news item
(usually a Word file plus photos) and must publish it into a CMS that accepts only an HTML
fragment with inline styles.

### User Story 1 - Word file in, laid-out news page out (Priority: P1)

**Reference behaviour**: Ports document reading, title detection, marker handling, and page
rendering from the reference. Extracting photos embedded in a `.docx` and deriving placements
from their document positions is **new** — the reference required a separately prepared folder.

An editor receives a `.docx` containing the news text with photos already embedded at the
right places. They open the file in the app and immediately see the finished news page —
text formatted, every photo placed where it stood in the document. No photo is manually
exported, no folder is prepared, no marker is typed by hand.

**Why this priority**: This is the single biggest time sink in the current tool. Today the
editor must save each photo out of Word by hand, collect them into a folder, and then type
placement markers to rebuild an arrangement the document already described. Removing that
step alone makes the new tool worth using, even before publishing works.

**Independent Test**: Open a `.docx` with text and embedded photos; confirm the preview shows
the same text and the same photo order and positions as the source document, with no manual
photo export and no folder selection at any point.

**Acceptance Scenarios**:

1. **Given** a `.docx` with a title, several paragraphs, and photos embedded between them,
   **When** the editor opens the file, **Then** the preview shows the title, the paragraphs,
   and each photo at the position it occupied in the document.
2. **Given** the same file, **When** the editor opens it, **Then** the photo list is populated
   from the embedded images in document order, with no folder chosen by the editor.
3. **Given** a `.docx` whose first line is the headline, **When** the file is opened,
   **Then** that line is used as the news title and is not repeated in the body text.
4. **Given** a `.txt` or `.md` file with placement markers in the text, **When** it is opened,
   **Then** the markers are honoured exactly as the current tool honours them.
5. **Given** any opened news item, **When** the editor edits the text or moves a photo,
   **Then** the preview reflects the change without an explicit rebuild step.

---

### User Story 2 - Publish photos and hand a fragment to the CMS (Priority: P2)

**Reference behaviour**: Ports image processing, SSH upload, per-item remote folders, slug
transliteration, and public URL construction from the reference. Skipping unchanged uploads
(D-7) and slug collision handling (D-8) are **new**.

The editor presses Publish. The app uploads the processed photos into a folder of their own
on the web server, produces the HTML fragment with inline styles and public photo URLs, and
gives the editor that fragment to paste into the CMS.

**Why this priority**: This closes the loop. Story 1 produces a page the editor can look at;
this makes it a page the audience can see. It is second only because it can be demonstrated
and tested on top of Story 1, and because a locally previewed item still has value.

**Independent Test**: Publish a prepared news item to a test server; confirm the remote folder
is created, all photos land in it, and the exported fragment references them by public URL and
renders correctly when pasted into the CMS.

**Acceptance Scenarios**:

1. **Given** a news item titled in Russian, **When** the editor publishes, **Then** a remote
   folder named from a transliterated slug of the title is created under the configured base
   path, and every used photo is uploaded into it.
2. **Given** a successful publish, **When** the fragment is exported, **Then** every image URL
   is the configured public base URL joined with the item folder and the photo file name.
3. **Given** an exported fragment, **When** it is inspected, **Then** it carries inline styles
   only — no stylesheet links, no `<style>` blocks, no styling classes.
4. **Given** an item that was already published and not since changed, **When** the editor
   publishes it again, **Then** nothing is uploaded and nothing on the server changes.
5. **Given** an item from which a photo was removed, **When** the editor publishes it again,
   **Then** the fragment no longer references that photo, and the photo's file is left on the
   server untouched rather than deleted.
6. **Given** an unreachable or rejecting server, **When** the editor publishes, **Then** the
   app reports which step failed in plain language and leaves the local work intact.
7. **Given** any publish, **When** the editor inspects logs or error messages, **Then** no
   password or private key material appears in them.

---

### User Story 3 - Portrait photos that keep their heads (Priority: P3)

**Reference behaviour**: Entirely **new**. The reference applied EXIF orientation and scaling
but offered no cropping, no rotation, and no framing rule; editors fixed portrait photos in a
separate image editor before importing them.

A vertical photo of a person is added to the item. The editor sees it framed sensibly, and can
adjust the framing with a visual crop control until it looks right — without opening a
separate image editor.

**Why this priority**: The named pain point from the current tool, and the reason editors
currently keep a second application open. It is third because the item can be published
without it, just less attractively.

**Independent Test**: Add a portrait photo of a person to an item, confirm the default framing
retains the subject's head, then adjust the crop in the app and confirm the published photo
matches the adjusted framing.

**Acceptance Scenarios**:

1. **Given** a portrait photo placed in a layout that would otherwise crop it, **When** the
   default framing is applied, **Then** the upper region of the photo is preserved rather than
   trimmed away.
2. **Given** any photo, **When** the editor opens the crop control, **Then** they can move and
   resize the crop frame and see the result live before confirming.
3. **Given** a cropped photo, **When** the editor reverts the crop, **Then** the original image
   is restored in full; the source file on disk is never modified.
4. **Given** a photo with rotation recorded by the camera, **When** it is displayed and
   published, **Then** it appears in its intended orientation.
5. **Given** a cropped photo, **When** the item is published, **Then** the uploaded file
   reflects the crop.

---

### User Story 4 - Build an item from loose photos, no folder required (Priority: P4)

**Reference behaviour**: Ports photo ordering (natural sort), renaming, and used/unused
indication. Drag-and-drop intake, clipboard paste, and working without any folder at all are
**new** — the reference required `--images-dir`.

Photos arrive by messenger, email, or as a scattered selection. The editor drags them onto the
app window, or pastes them from the clipboard, and arranges them — never nominating a
directory.

**Why this priority**: Covers the intake paths Story 1 does not, and removes the last hard
dependency on a prepared folder. Fourth because the Word path already covers the most common
intake.

**Independent Test**: Build and publish a complete news item using only drag-and-drop and
clipboard paste, without selecting a folder anywhere in the flow.

**Acceptance Scenarios**:

1. **Given** an open news item, **When** the editor drags image files onto the window,
   **Then** they are appended to the photo list in the order they were dropped.
2. **Given** an image copied to the clipboard, **When** the editor pastes it into the app,
   **Then** it is added to the photo list as a new photo.
3. **Given** a photo list, **When** the editor reorders or removes entries, **Then** existing
   placements follow the change and never silently point at a different photo.
4. **Given** a dropped file that is not a supported image, **When** it is dropped, **Then** the
   app says so and adds nothing.
5. **Given** dropped photos that share a file name, **When** they are added, **Then** both are
   kept and published under distinct names.

---

### User Story 5 - Arrange the photos for me (Priority: P5)

**Reference behaviour**: Ports the four layouts and their rendering. Automatic distribution and
layout selection are **new** — in the reference every placement was typed by hand as a marker.

The editor has text and photos but no arrangement. One action distributes the photos through
the text and picks a layout for each — full width, a row of several, or a floated image — with
every choice open to manual override.

**Why this priority**: A convenience on top of a working manual arrangement. It has the widest
range of acceptable outcomes, so it benefits most from being built last, once the layouts it
selects among are proven.

**Independent Test**: Take an item with unplaced photos, trigger automatic arrangement, and
confirm every photo is placed, the result respects paragraph boundaries, and any individual
placement can be changed afterwards.

**Acceptance Scenarios**:

1. **Given** an item with text and unplaced photos, **When** automatic arrangement runs,
   **Then** every photo receives a placement and no two placements land between the same pair
   of paragraphs unless grouped into one row.
2. **Given** automatic arrangement has run, **When** the editor changes one placement,
   **Then** only that placement changes.
3. **Given** photos of mixed orientation, **When** arrangement runs, **Then** the chosen layout
   suits each photo's shape rather than being uniform.
4. **Given** more photos than natural placement points, **When** arrangement runs, **Then**
   surplus photos are grouped into rows rather than dropped.
5. **Given** an item the editor has already arranged by hand, **When** they trigger automatic
   arrangement, **Then** they are warned before existing placements are replaced.

---

### Edge Cases

**Input documents**

- A `.docx` with no embedded photos: the text is imported and the item is usable with photos
  added by other means.
- A `.docx` with no discernible headline: the editor is asked for a title rather than being
  given a blank or invented one.
- A document with placement markers referencing photo numbers that do not exist: reported as a
  warning naming the offending marker, and the rest of the item still builds.
- Photos present but never referenced by any placement: reported as a warning; they are not
  uploaded.

**Photos**

- A photo larger than the size budget: it is re-encoded at reducing quality until it fits; if
  it cannot fit at the lowest permitted quality, the editor is told which photo and why.
- An unsupported or corrupt image file: reported per photo; the remaining photos still process.
- A photo already smaller than the budget: it is not enlarged or needlessly re-encoded.
- An extremely wide or extremely tall photo in a multi-photo row: the row stays within the
  page width and no photo in it collapses to an unreadable size.

**Publishing**

- Two different news items whose titles transliterate to the same slug: the second does not
  overwrite the first's folder.
- The connection drops midway through an upload: the failure names the photo that was in
  flight, and re-publishing completes the item.
- The remote base path does not exist or is not writable: reported before any photo is
  processed, not after.
- Neither a key nor a password is configured: publishing is blocked with a clear message.
- A saved server configuration whose credential has been removed or revoked in the operating
  system's secret storage: the editor is asked for it again rather than meeting an obscure
  authentication failure.
- The operating system's secret storage is locked or unavailable: the editor is told why and
  asked for the credential for this session only.
- A command-line run on an item that still needs a human arrangement decision: refused with a
  message pointing to the desktop application, rather than guessing a layout.

**Work in progress**

- The app is closed with unsaved changes: the editor is warned; work is not silently lost.
- The same photo is used in more than one placement: allowed, and uploaded once.

## Requirements *(mandatory)*

### Functional Requirements

**Document import**

- **FR-001**: System MUST import news text from `.docx`, `.txt`, and `.md` files.
- **FR-002**: System MUST extract photos embedded in a `.docx` without the editor exporting or
  locating any file, and MUST record each photo's position in the document.
- **FR-003**: System MUST convert embedded photo positions from a `.docx` into placements in
  the built page, preserving document order.
- **FR-004**: System MUST detect the news title from the source document and exclude it from
  the body text; where no title can be detected, the system MUST prompt for one.
- **FR-005**: System MUST continue to accept the existing placement marker language — a single
  full-width photo, a row of several photos, a left-floated photo, and a right-floated photo —
  so that documents prepared for the current tool still work.

**Photo intake and management**

- **FR-006**: Users MUST be able to add photos by dragging files onto the application window.
- **FR-007**: Users MUST be able to add a photo by pasting an image from the clipboard.
- **FR-008**: Users MUST be able to add photos by choosing files or a folder, but the system
  MUST NOT require a folder for any workflow.
- **FR-009**: Users MUST be able to reorder, rename, and remove photos, and existing placements
  MUST follow those changes rather than silently re-pointing.
- **FR-010**: System MUST show which photos are currently used in the page and which are not.
- **FR-011**: System MUST accept photos in the common camera and web formats used today
  (JPEG, PNG, WebP, GIF, BMP, TIFF) and MUST reject anything else with a named reason.

**Photo editing**

- **FR-012**: Users MUST be able to crop a photo with a visual, movable, resizable frame and
  see the result before confirming.
- **FR-013**: System MUST choose a default framing for portrait photos that preserves the upper
  region of the image, so that a person's head is not cut off without the editor's action.
- **FR-014**: Users MUST be able to revert any edit and recover the original image.
- **FR-015**: System MUST NOT modify the editor's source image files; all edits apply to the
  published copies only.
- **FR-016**: System MUST apply camera-recorded orientation so photos appear upright in both
  the preview and the published page.
- **FR-017**: Users MUST be able to rotate a photo in 90-degree steps.

**Layout and arrangement**

- **FR-018**: Users MUST be able to place a photo as full width, as one of a row, floated left,
  or floated right.
- **FR-019**: Users MUST be able to arrange all unplaced photos in one action, with the system
  distributing them through the text and selecting a layout suited to each photo's shape.
- **FR-020**: System MUST warn before automatic arrangement replaces placements the editor made
  by hand.
- **FR-021**: Users MUST be able to override any automatically chosen placement individually.

**Preview and output**

- **FR-022**: System MUST show a live rendered preview of the finished page that matches the
  exported result.
- **FR-023**: System MUST export an HTML fragment for pasting into the CMS that uses inline
  styles exclusively — no external stylesheets, no embedded style blocks, no styling classes.
- **FR-024**: System MUST produce identical output for identical input and settings.
- **FR-025**: System MUST ship exactly one built-in appearance that requires no configuration,
  and MUST allow that appearance to be overridden by configuration for editors who need it.

**Publishing**

- **FR-026**: System MUST upload processed photos over SSH into a folder created for that news
  item beneath a configured base path, with the folder named from a transliterated slug of the
  title, overridable by the editor.
- **FR-027**: System MUST build every image URL in the exported fragment from the configured
  public base URL, the item folder, and the photo file name.
- **FR-028**: System MUST reduce each photo to fit a configured maximum width and maximum file
  size before upload, degrading quality within configured bounds rather than failing outright.
- **FR-029**: System MUST support authentication by SSH key or password and MUST refuse to
  publish when neither is available.
- **FR-030**: System MUST make re-publishing an item leave the remote folder holding exactly the
  item's current photos, uploading only what differs from what is already there. System MUST NOT
  delete any remote file: a photo dropped from the item remains on the server as an orphan.
- **FR-031**: System MUST offer a way to perform a full build with no remote changes, so the
  editor can check the result before touching the server.
- **FR-032**: System MUST NOT include passwords or private key material in any log, error
  message, exported file, or preview.
- **FR-033**: Users MUST be able to save and reuse named server configurations.
- **FR-034**: System MUST report per-photo and per-placement problems as warnings that identify
  the offending item, and MUST complete the rest of the build rather than aborting.

**Scope reduction**

- **FR-035**: System MUST NOT carry over the following reference-tool capabilities: the library
  of six selectable appearance presets (superseded by the single built-in appearance of
  FR-025), the export of a standalone full HTML page (superseded by the live preview of
  FR-022), and the fallback upload path through an external command-line SSH client (one
  upload mechanism only).
- **FR-036**: The system MUST expose exactly two interfaces: a desktop application carrying the
  full editing workflow, and a command-line interface limited to building and publishing an
  already-prepared news item.
- **FR-037**: The command-line interface MUST take a news item that needs no human arrangement
  decisions — a source document with its placements already determined, plus a saved server
  configuration — and produce the same fragment and the same remote result the desktop
  application would produce for that item.
- **FR-038**: The command-line interface MUST NOT offer photo cropping, manual placement, or
  automatic arrangement; those decisions belong to the desktop application.
- **FR-039**: No behaviour MUST exist in one interface alone: anything the command line can do,
  the desktop application can also do.

**Credentials**

- **FR-040**: System MUST store publishing passwords and private key material in the operating
  system's secret storage, and MUST NOT write them to its own configuration files.
- **FR-041**: Where operating system secret storage is unavailable, the system MUST say so and
  fall back to asking for the credential each session rather than writing it to disk.
- **FR-042**: Users MUST be able to remove a stored credential from within the application.

### Key Entities

- **News Item**: One publishable story. Holds the title, the body text, the ordered photo set,
  the placements, the chosen appearance, and the target server configuration. Owns the slug
  that names its remote folder.
- **Source Document**: The imported `.docx`, `.txt`, or `.md` file. Contributes the title, the
  body text, and — for Word files — the embedded photos and their positions.
- **Photo**: One image belonging to a news item. Carries its origin (embedded, dropped, pasted,
  or chosen), its display name, its editor-applied adjustments (crop, rotation), and its
  used/unused status. Never mutates the file it came from.
- **Placement**: The binding of one or more photos to a position in the body text, together
  with a layout: full width, row, floated left, or floated right.
- **Appearance**: The set of visual choices applied when rendering the page to inline-styled
  HTML, plus the photo size and quality limits. One built-in appearance ships with the product;
  configuration may override it.
- **Server Configuration**: A reusable, named set of connection and path settings — host, user,
  port, remote base path, and public base URL — together with a reference to the credential
  held in the operating system's secret storage. The credential itself is never part of this
  record.
- **Publication**: The record of one publish attempt for a news item: the remote folder used,
  the photos uploaded, the resulting public URLs, and any warnings raised.

## Deviations from reference

Constitution principle I makes the Python tool at `https://github.com/LapunovRodion/news-builder`
the normative reference for observable behaviour, and requires every divergence to be recorded
here with a rationale. Anything not listed below is expected to reproduce the reference exactly.

New capabilities that add behaviour without changing anything the reference already did — Word
photo extraction, cropping, drag-and-drop intake, automatic arrangement — are marked per user
story above and are not repeated here. This section covers changes to behaviour the reference
*had*.

### D-1: Six appearance presets reduced to one built-in appearance

**Changes**: The reference shipped six selectable style presets. The product ships one.

**Rationale**: The CMS applies no styling of its own and editors have not needed to switch
looks. Configuration override remains, and the reference's preset file schema is unchanged, so
existing `style-presets/*.json` files still load — only the menu is gone (FR-025, FR-035).

### D-2: Standalone full-page HTML export dropped

**Changes**: The reference's `--full-output` wrote a complete standalone HTML page. It is gone.

**Rationale**: The live preview serves the same purpose — checking the result before publishing
— without leaving a second file to manage (FR-022, FR-035).

### D-3: Fallback upload through an external SSH client dropped

**Changes**: The reference could upload either through its SSH library or by invoking the system
`ssh` binary. Only one upload path remains.

**Rationale**: Two transports meant two behaviours to keep identical for no user-visible gain
(FR-035).

### D-4: Command-line surface reduced

**Changes**: The reference's CLI accepted the full set of build options. The new CLI builds and
publishes an already-arranged item and offers no cropping, manual placement, or arrangement.

**Rationale**: Those decisions need a human looking at the page. Keeping them out of the CLI
prevents a second, divergent authoring surface (FR-036 – FR-039).

### D-5: Credentials move to the operating system secret store

**Changes**: The reference wrote the SSH password in plain text to `~/.news_builder_gui.json`.
The product stores passwords and key material in the OS secret store and writes neither to its
own files.

**Rationale**: A publishing credential readable by anything with filesystem access is a real
exposure. This is stricter than the reference, not looser (FR-040 – FR-042, SC-010).

### D-6: Photo edits applied before scaling

**Changes**: Editor-applied rotation and cropping are new steps in a pipeline the reference
already had, applied after EXIF orientation and before scaling to the size budget.

**Rationale**: The published photo must reflect what the editor saw. For photos with no
adjustments the pipeline is byte-identical to the reference (FR-012, FR-015, FR-017).

### D-7: Unchanged photos are not re-uploaded

**Changes**: The reference re-uploaded every photo on every run, overwriting files of the same
name. The product uploads only what differs from what is already on the server.

**The product never deletes anything from the server.** A photo removed from a news item stays
on the server as an orphan, exactly as it would under the reference. This was decided
deliberately: deleting the wrong file from a live web server is worse than leaving an unused one,
and no orphan is ever visible to a reader.

**Rationale**: Skipping unchanged uploads makes re-publishing cheap and predictable, and is what
makes SC-009 checkable at all — under the reference's unconditional re-upload the server is never
byte-for-byte unchanged, because every file's modification time moves. File *contents* on the
server are identical either way; only upload traffic and timestamps differ.

**Affects**: FR-030, SC-009, and task T059.

### D-8: Slug collisions are suffixed rather than shared

**Changes**: The reference derived a remote folder name from the title and used it as-is. Two
different news items whose titles transliterate identically therefore published into the same
folder, where the second silently overwrote the first's photos. The product detects an existing
folder belonging to a different item and appends a numeric suffix.

**Rationale**: Silent cross-item overwrite is data loss that the editor never sees. The suffix
is visible in the published URLs, so the editor can tell the two items apart. Items whose slugs
do not collide are unaffected, so this changes no existing published URL.

**Affects**: FR-026 and the slug-collision edge case.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An editor publishes a ten-photo news item that arrived as a Word file in under
  five minutes, from opening the file to holding the CMS-ready fragment.
- **SC-002**: Preparing a Word-sourced news item requires zero manual photo exports and zero
  folder selections — down from one export per photo today.
- **SC-003**: Across a benchmark set of twenty portrait photographs of people, the default
  framing preserves the subject's head in every case, with no manual correction.
- **SC-004**: For every item in the parity fixture set, the published page renders in the CMS
  indistinguishably from the page the current tool produces for the same input.
- **SC-005**: An editor who has never seen the application publishes their first news item
  without written instructions and without assistance.
- **SC-006**: No editor needs a separate image editor at any point in the workflow.
- **SC-007**: Every failure an editor can cause — missing photo, unreachable server, oversized
  image, unsupported file — produces a message that names the specific item at fault.
- **SC-008**: The preview keeps up with editing: a text edit is reflected within 150 milliseconds,
  and a full rebuild of a thirty-photo item completes within one second.
- **SC-009**: Re-publishing an unchanged news item leaves the server byte-for-byte as it was.
- **SC-010**: A publishing password is recoverable from the application's own files by nobody:
  inspecting every file the application writes reveals no password and no private key.
- **SC-011**: A prepared news item published from the command line produces a fragment and a
  remote result identical to publishing the same item from the desktop application.

## Assumptions

- Editors are non-technical staff preparing news for a CMS that accepts a pasted HTML fragment
  and applies no stylesheet of its own — hence the inline-styles-only output contract.
- The application is a single-user desktop tool. There is no login, no multi-user access
  control, and no shared server-side state.
- Titles and body text are frequently Cyrillic, so transliteration to a Latin slug remains
  required for remote folder names.
- Photos come from phones and cameras: typically large JPEGs, frequently carrying orientation
  metadata, in both landscape and portrait shape.
- The web server is reachable over SSH, and the editor has an account with write access beneath
  the configured base path. The reference tool's SSH-based publishing model is kept.
- Photo processing defaults carry over from the reference implementation — a maximum width of
  1600 pixels, a maximum of 500 KB per photo, and JPEG/WebP quality reduced from 85 down to no
  lower than 50 — unless configuration overrides them.
- "Minimal editing" in the source task means crop and rotate. Colour correction, filters,
  retouching, and text overlays are out of scope.
- Automatic arrangement is a suggestion engine, not an authority: the editor always has the
  final say on every placement.
- Word import covers text, headings, paragraphs, and embedded images. Tables, footnotes,
  comments, tracked changes, and complex Word formatting are out of scope for this feature.
- Delivery is to one server per news item; publishing to several destinations at once is out of
  scope.
- The desktop application is the product; the command-line interface exists for automation and
  repeat publishing, not as a second way to author a news item. Its user is a technician, not
  the editor persona the user stories describe.
- Editors work on desktop operating systems that provide secret storage. Where a machine does
  not, per-session credential entry is an acceptable degradation rather than a blocker.
- Dropping the six appearance presets is safe because the CMS applies no styling of its own and
  editors have not needed to switch between looks; a single well-chosen appearance covers the
  workflow, and configuration remains available for exceptions.
- Dropping the standalone full-page export is safe because the in-app live preview serves the
  same purpose — checking the result before publishing — without producing a second file to
  manage.
