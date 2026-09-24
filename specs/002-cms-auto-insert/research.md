# Research: Insert the News Item into the Site Automatically

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-09-17

Each entry: Decision, Rationale, Alternatives considered. Entries marked **VERIFY** rest on an
assumption about the target server that could not be checked while planning (the planning
session was not permitted to log in to the test server); the first implementation task checks them, and a
failed check reopens the entry before any other work starts.

---

## R1: How an article reaches the site

**Decision**: Over the existing SSH session, open an **exec channel** that runs the server's PHP
command-line interpreter with a small PHP program — the *bridge* — sent on stdin. The bridge
boots Joomla from its install directory and saves the article through com_content's own
administrator `Article` model (`bootComponent('com_content')->getMVCFactory()->createModel(
'Article', 'Administrator', ['ignore_request' => true])`). It answers with one JSON document.

**Rationale**:

- Saving through Joomla's model is the only route that keeps everything a hand-made article
  gets: the `#__assets` row, the workflow association (Joomla 4+), the readmore split of
  `articletext` into `introtext`/`fulltext`, alias checks, content-history entries, and the
  `onContentBeforeSave`/`AfterSave` plugins (search index, cache clearing). FR-012 — "the values
  the CMS would give a new article created by hand" — holds by construction.
- The bridge reads the database settings from the site's own `configuration.php`. The
  application never sees a database password, so FR-014 and SC-006 hold with **no new secret at
  all**.
- `russh` already opens exec channels on the session the photos use. No new crate enters the
  network path (constitution: Technology constraints).
- Only the SSH account needs anything; no Joomla administrator login, API token, or open
  database port (FR-002).

**Alternatives considered**:

- *SQL straight into `#__content` through an SSH tunnel with a Rust MySQL client.* Rejected:
  a new network-path crate, a database credential to store, and every side table (`#__assets`,
  `#__workflow_associations`, `#__ucm_*`, content history) reimplemented and kept in step with
  Joomla releases. An article missing its asset or workflow row is invisible in the admin list —
  silent damage.
- *Joomla Web Services API (`POST /api/index.php/v1/content/articles`).* Clean, but it is HTTP
  with a super-user API token, not SSH, and needs the Web Services plugins enabled. It
  contradicts the user's stated model ("само заходило по ssh") and adds a second credential.
  Kept as the fallback if the first task finds no PHP CLI on the server.
- *Uploading the bridge as a file and running it.* Leaves an executable PHP file in the web root
  — a security hole if it lands anywhere web-reachable. stdin leaves nothing on disk.

## R2: Framing the bridge and its request on one stdin

**Decision**: The exec command is a fixed constant:

```sh
cd -- <joomla_root> && <php> -r '$n=(int)fgets(STDIN);eval("?>".stream_get_contents(STDIN,$n));'
```

stdin carries a decimal byte length and newline, then the bridge source, then the request JSON.
The bridge reads the rest of stdin as the request. The bridge prints diagnostics to stderr and
its answer to stdout as one line prefixed `NEWSBUILDER-BRIDGE:`; any other stdout (a PHP notice,
a plugin echo) is ignored.

`joomla_root` and `php` are single-quoted for POSIX `sh` by a quoting function with its own unit
tests; they are the only variable parts of the command.

**Rationale**: `php` reading a script from stdin consumes stdin to EOF, leaving nowhere for the
request; passing the request in `argv` exposes article text in the server's process list and
hits `ARG_MAX` on long items. The length prefix is the smallest framing that works with a plain
`php` binary. The sentinel line makes the answer robust against Joomla extensions that print.

**Alternatives considered**: `__halt_compiler()` data section (not readable when the script
itself came from stdin); base64 request in an environment variable (`SendEnv` is usually
refused by `sshd`).

## R3: Which Joomla versions — VERIFY

**Decision**: Joomla **4.x, 5.x and 6.x** (6 added during implementation: its boot path is
Joomla 5's, and `createQuery()` is used where it exists). The bridge reads `JVERSION` after boot and refuses any
other major version with `unsupported_joomla`, naming the version found (spec edge case: fail
naming the step, fall back to the fragment).

**Rationale**: Joomla 3 reached end of life in August 2023; the boot sequence and the
`Article` model's namespace differ between 3 and 4, and supporting both doubles the bridge.

**VERIFY (first task)**: the test server's Joomla version. If it is 3.x, this entry is reopened: the
bridge gains a Joomla 3 boot path (`JPATH_BASE`, `JModelLegacy::getInstance('Article',
'ContentModel')`) behind the same JSON contract, and nothing outside the bridge changes.

## R4: Booting Joomla from the command line

**Decision**: The bridge follows the documented custom-script boot (Joomla manual, "Custom PHP
script"), with the **administrator** application, because `ArticleModel` lives there:

1. `define('_JEXEC', 1)`, `JPATH_BASE = <root>/administrator`, require `includes/defines.php`
   and `includes/framework.php`.
2. Alias `SessionInterface` to `session.cli`, get `AdministratorApplication` from the container,
   set `Factory::$application`, `createExtensionNamespaceMap()`, load `lib_joomla` and
   `com_content` language strings.
3. Load the configured author as the current identity when one is set, so `created_by` and
   `modified_by` are that user; otherwise leave the identity empty and set `created_by` from the
   request.
4. Import the `content`, `finder`, `extension` and `workflow` plugin groups before saving, as the
   administrator controller does.

**Rationale**: The manual flags this boot as version-sensitive, so it is isolated in one file
that the opt-in end-to-end suite (R9) runs against each supported major version.

**Alternatives considered**: a Joomla console plugin (`php cli/joomla.php newsbuilder:save`) —
proper and stable, but it must be *installed* on the site as an extension, which is an
administrator task the editor cannot do and a deployment step for every site. Rejected for the
first version; noted as the upgrade path if the boot proves brittle.

## R5: Which article belongs to which item (FR-005)

**Decision**: No local bookkeeping. The article is identified on the site by evidence, the same
way the photo folder already is (feature 001, `resolve_folder`):

- **alias** = the folder slug actually published into (so a suffixed folder gives a suffixed
  alias — spec edge case "two items share a title");
- **ownership mark** = the article's `note` field holds `newsbuilder:<sha256>`, where the hash
  covers the title and `articletext` the application last wrote.

`find` searches **all categories** for articles with that alias and the `newsbuilder:` mark, so
changing the category between publishes moves the article instead of creating a second one.

**Rationale**: The CLI and the desktop publish the same item identically (SC-011 of 001, FR-016
here) only if neither holds state the other lacks. Local state would also be lost on a second
machine. The folder slug is already stable across re-publishes.

**Alternatives considered**: a local `item → article id` map (breaks across machines and between
frontends); storing the id inside the item file (items are not persisted as files); using
`metadata` JSON instead of `note` (`note` is a plain column, simplest to query; it is visible to
administrators as "Note", which is acceptable and even useful).

## R6: Decision matrix on re-publish (FR-006, FR-007, US2)

The bridge's `find` answers facts; `core` decides. Hashing happens in PHP (`hash('sha256', …)`)
so `core` takes no new crate.

| Found on site | Evidence of an earlier publish | Outcome |
|---------------|--------------------------------|---------|
| none | photo folder had none of our files | **create** |
| none | photo folder already held our files | **needs confirmation: gone** → create on confirm |
| one, trashed | — | **needs confirmation: trashed** → restore and update on confirm |
| one, mark matches current content, new content identical | — | **unchanged**, no write |
| one, mark matches current content, new content differs | — | **update** |
| one, mark does not match current content | — | **needs confirmation: edited on site** → update on confirm |
| more than one | — | **refused**, naming the article ids |
| none of ours, but the alias is taken in the target category by another article | — | **refused**, naming that article id |

Settings (category, state, …) that differ from the article's current values also make an
unchanged-content article an **update**. A dry run evaluates the matrix and writes nothing.

## R7: Settings and their defaults (FR-010, FR-012, Q3)

**Decision**: Every article setting is optional at both levels. Resolution per field: item
override → server default → *omitted from the request*, so Joomla's model applies the value a
hand-made article would get. Two exceptions:

- **category** has no Joomla default worth taking ("Uncategorised"), so a site target without a
  default category is incomplete and publish refuses before uploading anything.
- **state** defaults to *published* at the server level when the editor never touched it — the
  clarified answer to Q3.

Dates cross the wire as RFC 3339 with offset; the bridge converts to UTC as Joomla stores them.

## R8: Where the choice lists come from (FR-011, FR-017)

**Decision**: A bridge `describe` operation returns categories (id, title, level, published,
language), view levels, content languages, authors (users who have created at least one article,
not blocked), and the Joomla version. The desktop "Проверить подключение" button and the CLI
`server site check` both call it; it writes nothing.

**Rationale**: "All users" on a university site may be thousands of registered readers; people
who have authored articles is the list an editor actually picks from. An author not in the list
is still settable by id from the CLI.

## R9: Testing the bridge

**Decision**:

- `core` decision logic (R6, R7) — unit tests against an in-memory `Site` fake, test-first.
- The JSON contract — golden request/response files under `fixtures/bridge/`, deserialised by
  `core` tests, so a contract change is a reviewed diff.
- The bridge itself — `php -l` in `just gates` when `php` is on PATH (added to the Nix dev shell),
  plus an opt-in `just e2e-joomla` that brings up Joomla 5 + MariaDB + `sshd` in containers and
  runs create / unchanged / update / edited-on-site / dry run end to end.
- A static test asserts the bridge source contains no deleting call (`->delete(`, `unlink`,
  `DELETE FROM`, `state = -2` writes) — the exec channel's counterpart of "Transport has no
  removal method".

**Rationale**: Mirrors how 001 split pure logic (fakes), wire format (goldens) and the network
(opt-in `just e2e`).

## R10: Server prerequisites — VERIFY

The first implementation task confirms on the test server, read-only, before implementation starts:

1. `php` (CLI) is on the SSH account's PATH, version ≥ 8.1 (Joomla 5 minimum) or ≥ 7.2.5
   (Joomla 4).
2. The Joomla install directory, and that the SSH account can read `configuration.php`.
3. The Joomla major version (R3).
4. The database user in `configuration.php` can write `#__content` (it always can on a working
   site; checked by a dry-run `describe`).

If (1) fails, R1 falls back to the Web Services API alternative and this plan is revised. If (2)
fails, insertion is unavailable for that server, as the spec's assumptions already state.
