# Implementation Plan: Insert the News Item into the Site Automatically

**Branch**: `002-cms-auto-insert` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-cms-auto-insert/spec.md`

## Summary

Publish goes one step further: after the photos upload, the application creates or updates the
Joomla article itself, so the editor never pastes the fragment into the CMS by hand.

The whole design comes from one choice (research R1). On the SSH session that already carries the
photos, the application runs the server's `php` with a small **bridge** program sent on stdin.
The bridge boots Joomla and saves the article through Joomla's own article model. Joomla therefore
does all its usual bookkeeping (assets, workflow, readmore split, plugins, defaults) and reads its
own database settings. The application gets no new secret and no new network-path crate.

Everything that decides something stays in `core`: settings resolution, which article is the
item's, create vs update vs ask. It works against a new `Site` port, faked in tests. The CLI and
desktop gain settings screens and flags, and nothing else.

## Technical Context

**Language/Version**: Rust (stable, edition 2024) as in 001; TypeScript + Svelte 5 frontend. The
bridge is PHP targeting the server's PHP (≥ 7.2.5 for Joomla 4, ≥ 8.1 for Joomla 5).

**Primary Dependencies**: No new Rust crates. `russh` exec channels on the existing session;
`serde_json` for the bridge protocol; `time` for publication dates. Dev shell gains `php` for
`php -l`. The opt-in e2e uses container images `joomla:5` and `mariadb`.

**Storage**: `servers.json` gains an optional `site` object per server (backward compatible via
`serde(default)`). Per-item overrides live on the in-memory `NewsItem`. No local article
bookkeeping: articles are found on the site by alias and ownership mark (research R5).

**Testing**: `cargo test` with an in-memory `Site` fake (decision matrix, resolution, invariants);
golden bridge messages in `fixtures/bridge/`; static no-delete test on the bridge source; `php -l`
in `just gates`; opt-in `just e2e-joomla`.

**Target Platform**: Desktop as in 001 (Windows 10+, Linux). The server side is a Linux host with
Joomla 4.x/5.x and PHP CLI reachable by the SSH account.

**Project Type**: Existing three-crate workspace (core library, CLI, Tauri desktop).

**Performance Goals**: The article step adds under 3 s to a publish on a typical link (one PHP boot
per operation, two operations per publish: `find` then `save`). SC-001: Word file to live article
under 3 minutes.

**Constraints**: No deletion of articles (FR-008, INV-S3). No secret added to any file or output
(FR-014). With no site target, behaviour byte-identical to 001 (FR-013, INV-S5). Bridge runs
from stdin and leaves no file on the server.

**Scale/Scope**: One article per item per server; category lists of hundreds; authors limited to
people who have written articles.

**Open verification**: The server facts in research R10 (PHP CLI present, Joomla major version,
install readable). They could not be checked while planning, so the first implementation task checks them. A failed
check reopens R1 or R3 before other work.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle / constraint | Status | How |
|------------------------|--------|-----|
| I. Port fidelity | PASS | Feature is new (spec: "Reference behaviour: None"). Fragment, slug, folder and URLs unchanged; insertion off unless a site target exists (INV-S5). Config files from 001 load unchanged. No deviations to record |
| II. Core library, thin frontends | PASS | `Site` port, `publish_to_site`, `check_site`, `ArticleSettings::resolve` and the R6 matrix live in `core`; the bridge source is a `core` asset. CLI and desktop only map flags/forms to core types |
| III. Test-first against fixtures | PASS (noted) | Unit tests against the `Site` fake precede code; bridge messages are goldens in `fixtures/bridge/`. There is no Python reference for this feature, so goldens are captured from the e2e Joomla, not from the reference |
| IV. Determinism, isolated side effects | PASS | Site access only through the `Site` port; dry run supported end to end (FR-009); re-publish converges — unchanged article gets no write (FR-006); fragment still reads no clock |
| V. Explicit errors, no panics, no secrets | PASS | New typed errors name the step, category or article id; article failures after upload are reported, not aborting (`ArticleOutcome::Failed`); no new secret exists; bridge `detail` excludes configuration values |
| Tech: new crates on network path justified | PASS | None added |
| Tech: native SFTP, fallback transports documented | JUSTIFIED | An SSH **exec channel** is a new remote capability. See Complexity Tracking |
| Workflow: README documents flags/config | Planned | `server site`, new `publish` flags, `article` JSON, `site` config — task in `/speckit-tasks` |
| Repo content: no real hosts/credentials | PASS | Artifacts use placeholder hosts; `server.md` is ignored by `.gitignore` (uncommitted change in the working tree) |

**Post-design re-check (after Phase 1)**: unchanged. All PASS, with the one justified item below.

## Project Structure

### Documentation (this feature)

```text
specs/002-cms-auto-insert/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── bridge-protocol.md
│   ├── core-api.md
│   ├── cli.md
│   └── desktop-commands.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/core/src/
├── model/
│   ├── server.rs            # ServerConfig.site: Option<SiteTarget>
│   ├── site.rs              # NEW: SiteTarget, ArticleSettings, ArticleState, SiteCatalog, ArticleOutcome
│   └── item.rs              # NewsItem.article: ArticleSettings (overrides)
├── ports.rs                 # NEW trait Site
├── publish/
│   ├── mod.rs               # unchanged `publish`
│   └── article.rs           # NEW: publish_to_site, check_site, the R6 matrix
├── adapters/
│   ├── transport.rs         # SftpTransport implements Site via exec channel
│   └── joomla/
│       ├── mod.rs           # NEW: command quoting, stdin framing, response parsing
│       └── bridge.php       # NEW: include_str! asset
└── error.rs                 # new variants (contracts/core-api.md)

crates/core/tests/
├── site_publish.rs          # NEW: matrix + resolution against the Site fake
├── bridge_contract.rs       # NEW: goldens in fixtures/bridge/
└── bridge_no_delete.rs      # NEW: static check on bridge.php

crates/cli/src/main.rs        # `server site set|check|disable`, publish flags, JSON `article`
crates/desktop/src/commands/publish.rs   # check_site, article settings, confirmation
crates/desktop/ui/src/lib/
├── ServerSettings.svelte    # "Сайт Joomla" section
├── PublishDialog.svelte     # "Параметры статьи", link, confirmation
├── api.ts, types.ts         # new commands and views

fixtures/bridge/              # NEW: golden request/response JSON
tools/e2e-joomla/             # NEW: compose file for the opt-in suite
justfile, flake.nix, README.md
```

**Structure Decision**: No new crate or project. The feature is a new port plus its adapter in
`core`, following the `Transport` pattern from 001. The PHP bridge is a data asset of the
adapter, not a separate deployable.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| SSH exec channel beside SFTP. 001 kept the remote surface to list/mkdir/put, so that "no deletion" is guaranteed by what the transport cannot do | Saving a Joomla article correctly needs Joomla's own code running on the server (research R1) | Direct SQL needs a DB credential, a new crate, and a reimplementation of Joomla's side tables. The Web Services API needs an HTTP token, not SSH. **Containment**: the command is a compile-time constant with two quoted parameters; the only program run is the embedded bridge; the `Site` port has no delete operation; a static test rejects deleting calls in `bridge.php` |
| PHP source in a Rust repository | The bridge has to run inside Joomla | There is no way to use Joomla's model from outside PHP. The file is one self-contained script, linted in the gates and exercised by the opt-in e2e |
