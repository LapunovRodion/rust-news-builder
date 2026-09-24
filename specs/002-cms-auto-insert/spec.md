# Feature Specification: Insert the News Item into the Site Automatically

**Feature Branch**: `002-cms-auto-insert`

**Created**: 2026-09-17

**Status**: Draft

**Input**: User description: "фитча чтобы не надо было вручнуюю вставлять в html код на сайт, что это приложение само заходил по ssh и вставляло новость, чтобы были настройки те все который чесйчас только ui"

**Reference behaviour**: None. This feature is **new** — the reference tool, like feature 001,
stops at handing the editor an HTML fragment to paste into the CMS by hand.

## Clarifications

### Session 2026-09-17

- Q: What does inserting the news mean? → A: The site is Joomla; a new article is created in the news category.
- Q: Which settings are "currently only UI"? → A: The article options the editor now sets by hand in the CMS admin interface move into the application.
- Q: Publication state of a new article? → A: Published immediately.

## User Scenarios & Testing *(mandatory)*

The actor is the same **editor** as in feature 001. Today, after Publish, they copy the fragment,
open the site's administration pages in a browser, create a news article, paste the fragment,
fill in the article's options (category, publication state, dates and so on) and save. This
feature removes that browser step: the application connects to the site over the same SSH
connection it already uses for photos and creates the article itself.

### User Story 1 - Publish puts the article on the site (Priority: P1)

The editor presses Publish. The photos upload as they do today, and then the application
creates the news article on the site with the title and the fragment as its content. When the
dialog finishes, it shows a link to the article — nothing is left to paste.

**Why this priority**: This is the feature. Everything else refines it.

**Independent Test**: With a test site configured, publish a prepared item; confirm the article
exists on the site with the item's title and exactly the fragment the application produced, and
that its photos display.

**Acceptance Scenarios**:

1. **Given** a configured server with site insertion enabled, **When** the editor publishes an
   item, **Then** a new article exists on the site whose title is the item's title and whose
   content is byte-for-byte the fragment the application produced.
2. **Given** a successful insertion, **When** the publish dialog finishes, **Then** it shows the
   article's address on the site and the article's identifier.
3. **Given** a dry run, **When** the editor runs it, **Then** the application reports the article
   it would create or update, with all its settings, and changes nothing on the site.
4. **Given** the photo upload fails, **When** the editor publishes, **Then** no article is created
   or changed.
5. **Given** the photos uploaded but the article could not be written, **When** the publish ends,
   **Then** the editor is told plainly that the photos are on the server and the article is not,
   and the fragment is still offered for copying so the work is not lost.
6. **Given** a server configured without site insertion, **When** the editor publishes, **Then**
   behaviour is exactly as in feature 001: photos upload and the fragment is offered for copying.

---

### User Story 2 - Re-publishing updates the same article (Priority: P2)

The editor spots a typo after publishing, fixes it, and publishes again. The existing article is
updated in place; a second copy is never created.

**Why this priority**: Without it every correction creates a duplicate the editor has to clean up
by hand in the CMS — which reintroduces the browser step this feature exists to remove.

**Independent Test**: Publish an item, change one word, publish again; confirm the site holds one
article for the item and it shows the corrected text.

**Acceptance Scenarios**:

1. **Given** an item already inserted into the site, **When** the editor publishes it again,
   **Then** the same article is updated and no new article is created.
2. **Given** an item already inserted and unchanged since, **When** the editor publishes it again,
   **Then** the article is left untouched.
3. **Given** an item whose article was deleted on the site in the meantime, **When** the editor
   publishes it again, **Then** the application says the article is gone and asks whether to
   create a new one rather than silently doing either.
4. **Given** an article edited by hand in the CMS after it was inserted, **When** the editor
   publishes the item again, **Then** the application warns that the article was changed on the
   site and asks before overwriting it.

---

### User Story 3 - Article settings live in the application (Priority: P3)

Everything the editor currently sets by hand in the CMS's administration pages when creating a
news article is available as a setting in the application: a per-server default, and an override
for the item being published.

**Why this priority**: The article can be created with per-server defaults alone (Story 1), so
this is a refinement — but without it any non-default option still sends the editor back to the
browser.

**Independent Test**: Set a non-default category and publication date for one item, publish, and
confirm the article on the site carries exactly those values, while an item published without
overrides carries the server defaults.

**Acceptance Scenarios**:

1. **Given** a server with default article settings, **When** an item is published with no
   overrides, **Then** the article carries the server defaults.
2. **Given** an item with an overridden setting, **When** it is published, **Then** the article
   carries the override and every other setting keeps its server default.
3. **Given** the settings screen, **When** the editor chooses a category, **Then** the choices are
   the site's actual categories, read from the site, not typed by hand.
4. **Given** a newly inserted article, **When** it is created, **Then** its publication state is
   published, so the item is live on the site without opening the CMS.
5. **Given** a setting the editor never changes, **When** an article is created, **Then** that
   setting receives the same value the CMS would give a new article created by hand.

---

### User Story 4 - The same from the command line (Priority: P4)

A technician publishes a prepared item from the command line, and the article is inserted with
the same result as from the desktop application.

**Why this priority**: Required by the project's rule that CLI and desktop publish identically;
the editor persona does not use it.

**Independent Test**: Publish the same item once from each interface to two clean test sites;
compare the resulting articles.

**Acceptance Scenarios**:

1. **Given** a server with insertion configured, **When** an item is published from the command
   line, **Then** the article on the site is identical to one published from the desktop
   application, and the machine-readable result names the article's identifier and address.
2. **Given** a setting override passed on the command line, **When** the item is published,
   **Then** the article carries it.

---

### Edge Cases

- The site's database or CMS is unreachable from the SSH account even though the photo upload
  works: the failure names that step and the photos are still reported as uploaded.
- The configured category no longer exists on the site: insertion is refused before anything is
  written, naming the category.
- The title contains quotes, apostrophes, Cyrillic or characters special to the CMS: the article
  title shows exactly as typed.
- Two items share a title: they become two articles; the second item's article address is
  disambiguated the same way its photo folder is (D-8 of feature 001).
- The connection drops between writing the article and confirming it: the next publish finds the
  article rather than creating a duplicate.
- The fragment contains nothing the CMS would strip or rewrite on save through its own editor;
  where the CMS would, the application's insertion does not, and this difference is recorded.
- The SSH account lacks permission to change the site: the error says so and names the account.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST, when site insertion is enabled for a server, create a news article on
  the site as part of Publish, after the photos have uploaded successfully.
- **FR-002**: System MUST reach the site only through the SSH connection already configured for
  that server; no additional network access, browser session or CMS administrator login is
  required of the editor.
- **FR-003**: The article's title MUST be the item's title and its content MUST be the fragment
  exactly as rendered, including the announcement cut marker, so the site shows the announcement
  and the full text as it would for an article pasted by hand.
- **FR-004**: System MUST NOT write the article if any photo upload failed.
- **FR-005**: System MUST remember which article belongs to which item and update that article on
  re-publish instead of creating another.
- **FR-006**: System MUST leave an unchanged article untouched on re-publish.
- **FR-007**: System MUST detect that an article was changed on the site since the application
  last wrote it, and ask before overwriting it.
- **FR-008**: System MUST NOT delete articles. Removing an article from the site stays a manual
  action in the CMS.
- **FR-009**: Dry run MUST report the article that would be created or updated, including every
  setting, and write nothing to the site.
- **FR-010**: System MUST offer per-server default article settings and per-item overrides for the
  options an editor sets when creating a news article by hand in the CMS — at minimum: category,
  publication state, featured flag, access level, language, author name shown, publication start
  and finish dates, meta description, and tags (chosen from the site's existing tags).
- **FR-010a**: Users MUST be able to choose the article's cover (the CMS's intro image) from the
  item's placed photos — by default the first photo in the text — or choose no cover. Other image
  settings the editor made in the CMS MUST be kept.
- **FR-011**: System MUST read the list of categories (and other site-defined choices such as
  access levels and languages) from the site rather than requiring them to be typed.
- **FR-012**: Settings the editor does not set MUST receive the values the CMS gives a new article
  created through its own interface.
- **FR-013**: Site insertion MUST be optional per server; with it disabled, Publish MUST behave
  exactly as in feature 001.
- **FR-014**: Any database or CMS credential the insertion needs MUST be held like the SSH
  credential — in the operating system's secret store, never in a file, a log, an error message
  or machine-readable output.
- **FR-015**: On success, System MUST show the article's address and identifier; on failure it MUST
  name the failing step and still offer the fragment for copying.
- **FR-016**: The command line MUST support site insertion and every setting override, and MUST
  produce the same article as the desktop application for the same item and settings.
- **FR-017**: System MUST provide a connection check that verifies, without writing anything, that
  the configured account can reach the site's articles and that the configured category exists.

### Key Entities

- **Site target**: the part of a server configuration describing where articles go — whether
  insertion is enabled, how the site is reached through the SSH connection, the public address
  articles appear under, and a reference (never the value) to any credential it needs.
- **Article settings**: category, publication state, featured flag, access level, language,
  author name shown, publication window, meta description. Exists as per-server defaults and as
  per-item overrides.
- **Article link**: the record that ties a news item to the article created for it on a given
  server — the article's identifier, its address, and a fingerprint of what the application last
  wrote, used to detect edits made on the site.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An editor goes from opening a Word file to the article being live on the site without
  opening a browser, in under three minutes for a typical ten-photo item.
- **SC-002**: The article inserted by the application is visually indistinguishable on the site —
  announcement and full text — from the same fragment pasted by hand into the CMS.
- **SC-003**: Publishing the same item five times in a row leaves exactly one article on the site.
- **SC-004**: Zero articles are created or modified by a dry run, verified by comparing the site's
  articles before and after.
- **SC-005**: After any failed publish, the editor can still complete the job by hand with the
  fragment the application offers — no failure leaves the work unrecoverable.
- **SC-006**: Inspecting every file the application writes, its logs and its machine-readable
  output reveals no password or credential for the site.

## Assumptions

- The site runs Joomla, and "inserting the news" means creating a new article in a category: the fragment's `system-readmore` cut marker is that CMS's
  announcement marker, and the article options in FR-010 are that CMS's standard article options.
- The SSH account used for photo uploads can also reach the site's article storage (for example,
  through the site's own configuration on that host). If it cannot, insertion is unavailable for
  that server and Publish falls back to feature-001 behaviour.
- One item maps to one article per server. Publishing the same item to several sites is out of
  scope, as in feature 001.
- Photos keep their existing layout and addresses; this feature changes nothing about the upload.
- Menu items, tags, custom fields, versions history and multilingual associations are out of scope
  for this feature.
- Editing or deleting existing articles not created by this application is out of scope.
