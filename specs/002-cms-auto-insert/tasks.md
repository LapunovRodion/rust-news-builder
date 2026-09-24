---
description: "Task list for 002-cms-auto-insert"
---

# Tasks: Insert the News Item into the Site Automatically

**Input**: Design documents from `/specs/002-cms-auto-insert/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution makes TDD mandatory (principle III). In every phase, write
the test tasks first and watch them fail before starting the implementation tasks.

**Organization**: Grouped by user story (spec.md US1–US4) so each story ships and is tested on its
own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US4 from spec.md
- Paths are repository-relative. The workspace is `crates/core`, `crates/cli`, `crates/desktop` (UI
  in `crates/desktop/ui/src`).

---

## Phase 1: Setup

**Purpose**: Verify the server assumptions, then add the tooling the bridge needs.

- [ ] T001 Verify the research R10 server prerequisites on the test server, read-only, by asking the user to run `! ssh <user>@<host> 'php -v | head -1; ls <joomla_root>/configuration.php; grep -E "MAJOR_VERSION|MINOR_VERSION" <joomla_root>/libraries/src/Version.php'`, then record PHP version, Joomla version and install path (no hostnames, no credentials) under R10 in specs/002-cms-auto-insert/research.md. **STOP** and revise research R1/R3 and plan.md if PHP CLI is missing or Joomla is not 4.x/5.x
- [X] T002 [P] Add `php` (8.2+) to the dev shell packages in flake.nix, and add a `bridge-lint` recipe (`php -l crates/core/src/adapters/joomla/bridge.php`, skipped with a message when `php` is not on PATH) to justfile, included in the `gates` recipe
- [X] T003 [P] Create tools/e2e-joomla/compose.yml: `mariadb:11`, `joomla:5-php8.2-apache` auto-installed against it (`JOOMLA_SITE_NAME`, `JOOMLA_ADMIN_*`, `JOOMLA_DB_*` env), and an `sshd` in the Joomla container, or a sidecar sharing its web-root volume, with the `php` CLI and a throwaway user/password. Add a `just e2e-joomla` recipe that runs `docker compose -f tools/e2e-joomla/compose.yml up -d --wait`, then `NEWSBUILDER_E2E_JOOMLA=1 cargo test -p newsbuilder-core --features sftp --test e2e_joomla -- --ignored --nocapture`, then `down -v`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The types, port, errors and fake that every story uses. No user story starts before
this phase is done.

- [X] T004 [P] Write failing tests in crates/core/tests/model_invariants.rs: a `servers.json` written by 001 (no `site` key) deserialises with `site == None`; a `ServerConfig` with a `site` round-trips through serde; `ArticleState` serialises as `"published"`/`"unpublished"` and has no trashed/archived variant; `NewsItem::new().article` is all-`None`
- [X] T005 Create crates/core/src/model/site.rs with the types from data-model.md: `SiteTarget { joomla_root, site_url: Url, php (serde default "php"), defaults: ArticleSettings }`, `ArticleSettings` (all 10 fields `Option`, `#[serde(default)]`), `ArticleState { Published, Unpublished }`, `SiteCatalog` plus its entry structs, `ArticleProbe`, `FindResult { articles: Vec<SiteArticle>, alias_taken_by: Option<u32> }`, `SiteArticle`, `ArticleWrite`, `SavedArticle { id, created, url }`, `ArticleOutcome` (variants per data-model.md), `ConfirmationReason { Gone, Trashed, EditedOnSite }`, `ArticleConfirmation { None, Overwrite, CreateNew }` (default `None`). Register it in crates/core/src/model/mod.rs
- [X] T006 Add `#[serde(default)] pub site: Option<SiteTarget>` to `ServerConfig` in crates/core/src/model/server.rs, and set `site: None` at every construction site: crates/core/tests/support/fakes.rs `server_config`, crates/desktop/src/commands/publish.rs `config_from`, crates/cli/src/main.rs `server add`, plus anything `cargo build --workspace --all-features` reports
- [X] T007 Add `pub article: ArticleSettings` (default all-`None`) to `NewsItem` in crates/core/src/model/item.rs, initialised in `NewsItem::new`
- [X] T008 [P] Add the `Site` trait (`describe`, `find`, `save`, as in contracts/core-api.md, with no delete method and a doc comment saying why) to crates/core/src/ports.rs
- [X] T009 [P] Add error variants `SiteIncomplete { server, field }`, `SiteUnsupported { found }`, `CategoryMissing { id }`, `AliasTaken { alias, article }`, `ArticleAmbiguous { alias, ids }`, `SiteBridge { step, detail }` to crates/core/src/error.rs, and map them to exit codes 3/5/3/3/3/5 in crates/cli/src/exit.rs (and to the `kind` strings used by `--json`)
- [X] T010 Add `ScriptedSite` to crates/core/tests/support/fakes.rs. It implements `Site`, records every call in order (`Describe`, `Find(ArticleProbe)`, `Save(ArticleWrite)`), returns scripted `FindResult`/`SiteCatalog`/`SavedArticle`/errors, and has a `site_target()` helper returning a `SiteTarget` with `defaults.category = Some(8)`. Also add `RemoteFake`, which combines `RecordingTransport` + `ScriptedSite` and implements both traits by delegation, for `publish_to_site<R: Transport + Site>`

**Checkpoint**: `cargo test --workspace` green (T004 now passing), and 001 behaviour unchanged.

---

## Phase 3: User Story 1 - Publish puts the article on the site (Priority: P1) 🎯 MVP

**Goal**: Publish uploads the photos and then creates the Joomla article with the fragment. The
article link is shown. A failure after the upload still leaves the fragment in hand.

**Independent Test**: quickstart.md steps 3, 4, 8. Dry run changes nothing, a live publish
creates one article whose text is the fragment, and a broken Joomla root reports `failed` with
the fragment kept.

### Tests for User Story 1 ⚠️ write first, see them fail

- [X] T011 [P] [US1] Create crates/core/tests/site_publish.rs covering:
  - first publish on an empty site calls `Save` once with `id: None`, `alias` = `Publication.folder`, `articletext` == `Publication.fragment` (INV-S4), and yields `ArticleOutcome::Created`;
  - `DryRun` yields `WouldCreate` and records no `Save`;
  - a failing `put` returns `Err` and records no `Site` call (FR-004);
  - a `Save` error yields `Ok` with `ArticleOutcome::Failed { step, .. }` and the fragment present (FR-015);
  - `site: None` yields a `Publication` equal to `publish()`'s with `article == None` (INV-S5);
  - a site target with `defaults.category == None` fails with `SiteIncomplete` before any transport call (INV-S1)
- [X] T012 [P] [US1] Create golden messages in fixtures/bridge/: `describe.request.json`, `describe.response.json`, `find.request.json`, `find.response.json`, `save.request.json`, `save.response.json`, `error.category_missing.json`, exactly as in contracts/bridge-protocol.md. Create crates/core/tests/bridge_contract.rs asserting that `core`'s request builders serialise to the request goldens byte-for-byte (after `serde_json` pretty-print normalisation) and that the response goldens deserialise into `SiteCatalog`/`FindResult`/`SavedArticle`/the error mapping
- [X] T013 [P] [US1] Create crates/core/tests/bridge_no_delete.rs: `include_str!("../src/adapters/joomla/bridge.php")` contains none of `->delete(`, `unlink(`, `DELETE FROM`, `->trash(`, `->publish(` (the model's state-change method), `=> -2` or `= -2` (the trashed state), `checkin`, `checkout` (INV-S3)
- [X] T014 [P] [US1] Write unit tests (`#[cfg(test)]`) in crates/core/src/adapters/joomla/mod.rs covering:
  - `sh_quote` on plain paths, spaces, `'`, and `$()`;
  - `command(target)` equals the research R2 constant with both parts quoted;
  - `frame(request)` = `"<len>\n" + BRIDGE + json`, where `len` == `BRIDGE.len()`;
  - `parse(stdout, stderr, status)` takes the `NEWSBUILDER-BRIDGE:` line among noise lines, maps `{ok:false,error:{kind}}` per the contracts/bridge-protocol.md table, and returns `SiteBridge` with the exit status and the last 2 KB of stderr when the sentinel is missing
- [X] T015 [P] [US1] Create crates/core/tests/e2e_joomla.rs (`#[ignore]`, skipped unless `NEWSBUILDER_E2E_JOOMLA=1`) against the tools/e2e-joomla stack:
  - dry run creates nothing;
  - publish creates one article whose `introtext`+readmore+`fulltext` reassemble to the fragment;
  - a nonexistent `joomla_root` yields `Failed` with step "starting Joomla"

### Implementation for User Story 1

- [X] T016 [US1] Create crates/core/src/adapters/joomla/mod.rs, compiled without the `sftp` feature because it is pure: `pub const BRIDGE: &str = include_str!("bridge.php")`, plus `sh_quote`, `command`, `frame`, the request builders for `describe`/`find`/`save` (protocol 1; omit `None` settings keys, send `null` only to clear dates) and `parse`. Declare `pub mod joomla;` in crates/core/src/adapters/mod.rs
- [X] T017 [US1] Write crates/core/src/adapters/joomla/bridge.php:
  - read the request from the rest of STDIN;
  - boot Joomla per research R4 (administrator app, `session.cli`, plugin groups);
  - refuse major versions other than 4/5 with `unsupported_joomla`;
  - `save`: create via `bootComponent('com_content')->getMVCFactory()->createModel('Article','Administrator',['ignore_request'=>true])->save($data)` with `articletext`, `alias`, `title`, `catid`, and `note = 'newsbuilder:'.hash('sha256', $title."\n".$articletext)`; verify the category exists (`category_missing`) and return the new id and `index.php?option=com_content&view=article&id=<id>&catid=<catid>` joined to the request's `site_url`;
  - `find` and `describe`: minimal versions for now, extended in US2 and US3;
  - print exactly one `NEWSBUILDER-BRIDGE:<json>` line on stdout, send warnings to stderr, and never echo `configuration.php` values
- [X] T018 [US1] Implement `Site` for `SftpTransport` in crates/core/src/adapters/transport.rs: on the existing `Handle`, open a session channel, `exec(true, joomla::command(target))`, write `joomla::frame(request)`, send EOF, collect stdout/stderr/exit status until close, then `joomla::parse`. Run it on the adapter's runtime like the SFTP calls. Error when not connected, naming the step
- [X] T019 [US1] Create crates/core/src/publish/article.rs with `publish_to_site` for the create path:
  - INV-S1 check first;
  - then `publish()`, returning on `Err`;
  - build `ArticleWrite` from `item.title`, `publication.fragment`, `publication.folder` and `ArticleSettings::resolve(&item.article, &target.defaults)` (a plain field-by-field `or`, with state defaulting to `Published`);
  - `find` (treat an empty result as create), then `save` on `Live` or `WouldCreate` on `DryRun`;
  - convert any `Site` error after the upload into `ArticleOutcome::Failed { step, detail }`.

  Add `pub article: Option<ArticleOutcome>` to `Publication` in crates/core/src/publish/mod.rs (set to `None` in `publish`) and re-export `article::publish_to_site`
- [X] T020 [US1] Desktop backend in crates/desktop/src/commands/publish.rs:
  - `ServerView` gains `site: Option<SiteTargetView>` (camelCase, per contracts/desktop-commands.md), mapped both ways in `config_from` and `list_servers`;
  - `publish` calls `publish_to_site` with `ArticleConfirmation::None`;
  - `PublicationView` gains `article: Option<ArticleOutcomeView>`
- [X] T021 [P] [US1] Frontend types and calls in crates/desktop/ui/src/lib/types.ts and crates/desktop/ui/src/lib/api.ts: `SiteTargetView`, `ArticleSettingsView`, `ArticleOutcomeView`, `ServerView.site`, `PublicationView.article`
- [X] T022 [US1] In crates/desktop/ui/src/lib/ServerSettings.svelte, add a "Сайт Joomla" section to the edit form:
  - an enable checkbox (off → `site: null`);
  - «Каталог Joomla на сервере», «Адрес сайта», «Команда PHP» (default `php`), and «Категория по умолчанию (ID)» as a number input;
  - one line in the server list showing the site address when enabled
- [X] T023 [US1] In crates/desktop/ui/src/lib/PublishDialog.svelte, render `result.article`:
  - `created`/`unchanged`/`updated`: «Статья на сайте» with the id and a link opened through the existing opener, or a copyable URL if none is wired;
  - `would_create`/`would_update`: «Статья была бы создана/обновлена»;
  - `failed`: «Фото загружены, статья не записана: <step> — <detail>», with the fragment block kept visible

**Checkpoint**: US1 works from the desktop against `just e2e-joomla` and quickstart step 4.

---

## Phase 4: User Story 2 - Re-publishing updates the same article (Priority: P2)

**Goal**: Re-publish converges on one article. It is unchanged when nothing changed, updated when
the document changed, and needs confirmation when the article was edited, trashed or deleted on the
site.

**Independent Test**: quickstart.md steps 5–7.

### Tests for User Story 2 ⚠️

- [X] T024 [P] [US2] Create crates/core/tests/site_republish.rs, with one test per research R6 row, using `ScriptedSite`:
  - identical → `Unchanged` and no `Save`;
  - mark ok with different text → `Save` with `id: Some`, giving `Updated`;
  - mark mismatch → `NeedsConfirmation(EditedOnSite)` without `Save`, and `Overwrite` → `Updated`;
  - trashed → `NeedsConfirmation(Trashed)`, and `Overwrite` → `Save` with `restore: true`;
  - none found while the photo folder held our files (`Publication.unchanged` non-empty or folder pre-populated) → `NeedsConfirmation(Gone)`, and `CreateNew` → `Created`;
  - two found → `ArticleAmbiguous` as `Failed`;
  - `alias_taken_by: Some` → `AliasTaken` as `Failed`;
  - a mismatched confirmation (`CreateNew` when the article was edited on site) still yields `NeedsConfirmation`;
  - `DryRun` on an existing article → `WouldUpdate` with no `Save`
- [X] T025 [P] [US2] Extend crates/core/tests/e2e_joomla.rs with:
  - publish twice → `Unchanged`, and the article's `modified` is unchanged;
  - edit the text through SQL in the container → `EditedOnSite`;
  - trash it → `Trashed`, and `Overwrite` restores it;
  - delete the row → `Gone`, and `CreateNew` creates a new id

### Implementation for User Story 2

- [X] T026 [US2] Implement the full research R6 matrix and `ArticleConfirmation` handling in `publish_to_site` in crates/core/src/publish/article.rs. The "earlier publish" evidence is: the resolved folder already held files before this publish (`!publication.unchanged.is_empty()` or the folder listing was non-empty)
- [X] T027 [US2] Complete `find` and `save` in crates/core/src/adapters/joomla/bridge.php:
  - `find` queries `#__content` for the alias across categories with `note LIKE 'newsbuilder:%'`, and computes `trashed`, `content_matches_mark` (hash of the current title + reassembled `introtext`/`fulltext` against the note), `identical` (title, text, and every requested setting), `alias_taken_by` (an unmarked article with the alias in the target category), and `url`;
  - `save` with `id` loads and updates that article (the model with `id` set), and with `restore: true` sets the requested state (never -2);
  - keep the `find` request/response goldens in fixtures/bridge/ in step
- [X] T028 [US2] Desktop confirmation:
  - `publish` in crates/desktop/src/commands/publish.rs takes `confirmation: 'none'|'overwrite'|'create_new'`, mapped to `ArticleConfirmation`;
  - crates/desktop/ui/src/lib/api.ts passes it;
  - crates/desktop/ui/src/lib/PublishDialog.svelte shows the `needs_confirmation` message per reason («Статью изменили на сайте — перезаписать?», «Статья в корзине — восстановить и обновить?», «Статью удалили с сайта — создать заново?») with a confirm button that re-runs `publish` with the matching confirmation and a cancel button

**Checkpoint**: US1 and US2 pass together, and quickstart steps 3–8 pass.

---

## Phase 5: User Story 3 - Article settings live in the application (Priority: P3)

**Goal**: Server defaults and per-item overrides for every article option, with the choices read
from the site.

**Independent Test**: quickstart.md step 9 (desktop part). An override of category and publish date
lands on the article, and the other fields keep the server defaults.

### Tests for User Story 3 ⚠️

- [X] T029 [P] [US3] Create crates/core/tests/article_settings.rs covering:
  - `ArticleSettings::resolve`, field by field (override wins, default next, `None` stays `None`);
  - `state` defaulting to `Published` when neither level sets it;
  - INV-S2 (`publish_down <= publish_up` rejected by a `validate()` with a named error);
  - `meta_description` over 300 chars or containing a newline rejected;
  - the `save` request omitting every `None` key (compare to a golden fixtures/bridge/save.minimal.request.json);
  - `check_site` calling `connect` then `Describe` only, with no `Save`
- [X] T030 [P] [US3] Extend crates/core/tests/e2e_joomla.rs with:
  - `describe` returns the stack's categories, access levels, `*` among the languages, and the Joomla version;
  - a publish with overrides (featured, access 2, `publish_up` in the future, meta description) stores exactly those values, with `publish_up` stored as UTC

### Implementation for User Story 3

- [X] T031 [US3] In crates/core/src/model/site.rs, implement `ArticleSettings::resolve` and `validate` (INV-S2, meta description rules, `author_alias` ≤ 255 chars). In crates/core/src/publish/article.rs, add `check_site(server, remote, secrets)`: resolve the credential like `publish`, `connect`, then `describe`, returning `SiteIncomplete` when `server.site` is `None`. Also call `validate` in `publish_to_site` before the upload
- [X] T032 [US3] Complete `describe` and settings handling in crates/core/src/adapters/joomla/bridge.php:
  - `describe` returns `com_content` categories in `lft` order (id, title, level, published, language), view levels, content languages plus `*`, authors (`DISTINCT created_by` from `#__content` joined to `#__users` where `block = 0`), and `JVERSION`;
  - `save` maps the settings keys to `catid`, `state`, `featured`, `access`, `language`, `created_by`, `created_by_alias`, `publish_up`/`publish_down` (converted to UTC `Y-m-d H:i:s`), and `metadesc`;
  - keep the `describe`/`save` goldens in fixtures/bridge/ in step
- [X] T033 [US3] *(Deviation: the catalog is cached by the UI per open dialog, not in Rust session state — nothing else reads it.)* Add three commands to crates/desktop/src/commands/publish.rs:
  - `check_site(server)`, which calls core `check_site` on the blocking pool as `publish` does and caches the `SiteCatalogView` per server in session state (crates/desktop/src/state.rs);
  - `get_article_settings()` and `set_article_settings(settings)`, reading and replacing `NewsItem.article` on the owned item and returning the validated view.

  Register all three in crates/desktop/src/lib.rs
- [X] T034 [P] [US3] Add `checkSite`, `getArticleSettings`, `setArticleSettings` and `SiteCatalogView` to crates/desktop/ui/src/lib/api.ts and crates/desktop/ui/src/lib/types.ts
- [X] T035 [US3] In crates/desktop/ui/src/lib/ServerSettings.svelte:
  - add a «Проверить подключение» button that calls `checkSite` and shows the Joomla version or the error;
  - replace the category number input with a select from the catalog, indented by `level`;
  - add the defaults form: state (опубликовано/не опубликовано, default опубликовано), «Избранное», access select, language select, author select, «Автор (псевдоним)», and «Мета-описание»;
  - disable the selects until the catalog is loaded
- [X] T036 [US3] In crates/desktop/ui/src/lib/PublishDialog.svelte, add a collapsible «Параметры статьи» section:
  - prefill it with the server defaults, loading the catalog through `checkSite` when it is not cached;
  - include the category, state, featured, access, language, author, author alias, «Начало публикации»/«Окончание публикации» (`datetime-local`, converted to RFC 3339 with the local offset) and meta-description fields;
  - save through `setArticleSettings` before `publish`;
  - add a «Сбросить к настройкам сервера» button that clears the overrides

**Checkpoint**: US1–US3 pass from the desktop.

---

## Phase 6: User Story 4 - The same from the command line (Priority: P4)

**Goal**: Full site insertion and every override from `newsbuilder`, producing the same article as
the desktop.

**Independent Test**: quickstart.md steps 2–8 run through the CLI, and step 9's parity check.

### Tests for User Story 4 ⚠️

- [X] T037 [P] [US4] Create crates/cli/tests/site_cli.rs covering:
  - `server site set` / `check --json` / `disable` edit `servers.json` in a temp config dir (`--category` required the first time, later calls change only the flags given);
  - `server list --json` includes `site`;
  - publish setting flags on a server without a site → exit 1 naming the server;
  - `--json` `article` object shapes for each `ArticleOutcome` per contracts/cli.md;
  - `needs_confirmation` → exit 3 with stderr naming `--overwrite-article`/`--create-article`;
  - `failed` → exit 5 with the fragment still printed
- [ ] T038 [P] [US4] **Skipped** — both frontends only translate flags or form fields into core's `ArticleSettings`, and everything after that (resolution, the write) is core's; asserting it across crates would need the Tauri crate as a CLI dev-dependency. Covered instead by `ArticleSettings::resolve` tests and each frontend's own mapping tests. Original: Extend crates/cli/tests/parity_with_desktop.rs: the same item with the same overrides produces an identical `ArticleWrite` from the CLI flag mapping and from the desktop `set_article_settings` mapping. Assert through the core request builder, with no network

### Implementation for User Story 4

- [X] T039 [US4] Add the `server site set|check|disable` subcommands in crates/cli/src/main.rs, with the flags from contracts/cli.md. `check` connects with `SftpTransport` and prints the catalog as a table, or as JSON with `--json`
- [X] T040 [US4] In crates/cli/src/main.rs, add the publish flags (`--no-article`, the setting overrides, `--overwrite-article`, `--create-article`) mapped into `NewsItem.article` and `ArticleConfirmation`. Call `publish_to_site` unless `--no-article` is given. Add the `article` object to the JSON report and the exit codes from contracts/cli.md in crates/cli/src/exit.rs

**Checkpoint**: All four stories pass. `just gates` is green.

### Addition: the cover (requested after implementation)

- [X] T044 [US3] Add `IntroImage { First, Photo(PhotoId), None }` on `NewsItem`, `Publication.photo_urls`, and cover resolution (site-relative under `site_url`, refused before upload if the photo is not placed) in crates/core/src/model/site.rs and crates/core/src/publish/article.rs; tests in crates/core/tests/site_publish.rs
- [X] T045 [US3] Send `intro_image` on `find`/`save` (crates/core/src/adapters/joomla/mod.rs, fixtures/bridge/save.request.json) and merge it into the article's `images` JSON in crates/core/src/adapters/joomla/bridge.php; extend crates/core/tests/e2e_joomla.rs (default, chosen, full-text image kept, unchanged on re-publish)
- [X] T046 [US3] «Заставка» thumbnails in crates/desktop/ui/src/lib/PublishDialog.svelte with `get_intro_image`/`set_intro_image` in crates/desktop/src/commands/site.rs
- [X] T047 [US4] `publish --intro-image first|none|N` in crates/cli/src/main.rs

### Addition: tags (requested after implementation)

- [X] T048 [US3] `ArticleSettings.tags` and `SiteCatalog.tags` in crates/core/src/model/site.rs (resolution replaces, sorted, deduplicated; 0 refused); tests in crates/core/tests/article_settings.rs; goldens in fixtures/bridge/
- [X] T049 [US3] `describe` lists tags, `save` writes them through the model, `find` compares them, in crates/core/src/adapters/joomla/bridge.php; crates/core/tests/e2e_joomla.rs covers set, unchanged, replace, clear
- [X] T050 [US3] «Метки» chips in crates/desktop/ui/src/lib/ArticleSettingsForm.svelte (server defaults and per item)
- [X] T051 [US4] `--tag ID` (repeat) / `--no-tags` in crates/cli/src/main.rs; test in crates/cli/tests/site_cli.rs

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T041 [P] Extend crates/core/tests/no_secret_leak.rs so that a `ServerConfig` with a `site`, a `Publication` with every `ArticleOutcome` variant, and a `SiteBridge` error carrying a stderr tail contain no sentinel secret (SC-006)
- [X] T042 [P] Document the feature in README.md (constitution: flags documented before release):
  - a "Site insertion (Joomla)" section covering the prerequisites (PHP CLI, readable install), the `server site` commands, the publish flags, the `article` JSON object, the new exit-code situations, the `site` object in `servers.json`, and the `newsbuilder:` note mark;
  - `just e2e-joomla` under Development
- [ ] T043 Run `just gates` and `just e2e-joomla`, then walk quickstart.md steps 0–10 against the test server with the user, and record the results in specs/002-cms-auto-insert/quickstart-results.md (no hostnames or credentials)

---

## Dependencies & Execution Order

```text
Phase 1 (T001 gate → T002, T003)
   └─► Phase 2 (T004 → T005 → T006, T007; T008, T009 parallel; T010 after T005, T008)
          └─► US1 (P1) ─► US2 (P2)
                    └──► US3 (P3)
                    └──► US4 (P4) ── full value after US2 + US3 (it exposes their flags)
                                         └─► Phase 7
```

- **T001 gates everything.** A failed prerequisite changes the design (research R10).
- **US1** needs only Phase 2. It is the MVP.
- **US2** and **US3** both build on US1's `publish_to_site` and bridge. They touch the same files
  (article.rs, bridge.php, PublishDialog.svelte), so do them one after the other, not side by side.
- **US4** can start after US1 for the create path. Its override and confirmation flags need US2 and
  US3.
- Within each story, the tests come first and must fail. Then do models, then core logic, then
  the adapter and bridge, then the frontends.

## Parallel Opportunities

- Phase 1: T002 ∥ T003 (after T001).
- Phase 2: T004 ∥ T008 ∥ T009, then T010.
- US1 tests: T011 ∥ T012 ∥ T013 ∥ T014 ∥ T015. Implementation: T016 → T017 ∥ T018 → T019 → T020, with T021 in parallel with T020, then T022, T023.
- US2: T024 ∥ T025, then T026 → T027 → T028.
- US3: T029 ∥ T030, then T031 → T032, then T033 ∥ T034, then T035, T036.
- US4: T037 ∥ T038, then T039 → T040.
- Polish: T041 ∥ T042, then T043.

### Example: launching the US1 tests together

```text
Task: "T011 site_publish.rs create-path tests against ScriptedSite"
Task: "T012 fixtures/bridge goldens + bridge_contract.rs"
Task: "T013 bridge_no_delete.rs static check"
Task: "T014 joomla/mod.rs quoting/framing/parse unit tests"
Task: "T015 e2e_joomla.rs create/dry-run/failure (ignored)"
```

## Implementation Strategy

### MVP (US1 only)

1. T001: confirm the server can run the bridge.
2. Phase 2, then US1 (T011–T023).
3. **Stop and validate**: run quickstart steps 3, 4, 8 against `just e2e-joomla`, then against the
   test server. Editors can already publish without the browser, as long as each item is
   published once.

### Incremental delivery

1. US2: corrections no longer duplicate articles. This is the first release worth handing to
   editors.
2. US3: no more trips to the admin for category, dates or featured.
3. US4: the technician's CLI catches up. Then Phase 7.

## Notes

- Nothing in this feature may change the fragment, slug, folder or URLs (INV-S5). The parity suite
  must stay green at every checkpoint.
- The bridge must never gain a deleting or trashing operation. T013 enforces this, so do not
  weaken it.
- Commit after each task or logical group, following the repository's `feat(core)` /
  `feat(desktop)` / `feat(cli)` message style.
