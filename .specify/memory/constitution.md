<!--
Sync Impact Report
- Version change: none (unfilled template) → 1.0.0
- Bump rationale: initial ratification; the previous file was an unpopulated scaffold with
  no governed content, so this is the first binding version.
- Modified principles:
  - [PRINCIPLE_1_NAME] → I. Port Fidelity First (NON-NEGOTIABLE)
  - [PRINCIPLE_2_NAME] → II. Core Library, Thin Frontends
  - [PRINCIPLE_3_NAME] → III. Test-First Against Reference Fixtures (NON-NEGOTIABLE)
  - [PRINCIPLE_4_NAME] → IV. Deterministic Output, Isolated Side Effects
  - [PRINCIPLE_5_NAME] → V. Explicit Errors, No Panics, No Leaked Secrets
- Added sections:
  - [SECTION_2_NAME] → Technology and Output Constraints
  - [SECTION_3_NAME] → Development Workflow and Quality Gates
- Removed sections: none
- Deferred TODOs: none
-->

# Rust News Builder Constitution

## Core Principles

### I. Port Fidelity First (NON-NEGOTIABLE)

This project is a Rust port of the Python tool at
`https://github.com/LapunovRodion/news-builder`. Until an amendment says otherwise, that
implementation is the normative reference for observable behavior.

- Every ported capability MUST reproduce the reference's observable output for identical
  input: HTML fragment bytes, generated slug, image ordering, remote directory layout, and
  public URLs.
- Divergence from the reference MUST be deliberate: recorded in the feature spec under a
  "Deviations from reference" heading, with rationale. Undocumented divergence is a defect,
  not a design choice.
- Features that do not exist in the reference MAY be added, but they MUST NOT alter the
  behavior of already-ported capabilities.

Rationale: the value of this project is a like-for-like replacement. Silent behavioral drift
would break CMS output that downstream editors already depend on.

### II. Core Library, Thin Frontends

- All domain logic — document reading, marker parsing, transliteration and slug building,
  image processing, HTML rendering, upload orchestration — MUST live in a library crate that
  is usable with no CLI and no GUI present.
- The CLI binary and the GUI MUST be thin adapters: argument and event handling, wiring, and
  presentation only. No domain rule may exist solely inside a frontend.
- Any behavior reachable from the GUI MUST be reachable through the library API, and any
  behavior exposed by the CLI MUST be covered by CLI-level tests.

Rationale: in the reference, the GUI is roughly twice the size of the CLI script, which makes
logic duplication between the two the most likely source of divergent behavior. A single core
removes that class of bug by construction.

### III. Test-First Against Reference Fixtures (NON-NEGOTIABLE)

- TDD is mandatory: a failing test is written first, then the implementation, then refactor.
- Every ported unit MUST be covered by golden fixtures captured from the Python reference and
  committed to the repository; parity is asserted against those fixtures, not against
  hand-written expectations.
- Golden coverage is required for at minimum: HTML rendering of each marker form, slug
  transliteration, marker parsing, natural-sort image ordering, and remote path / public URL
  construction.
- Every fixed bug MUST first be reproduced by a failing regression test.

Rationale: parity is only real if it is executable. Fixtures turn Principle I from an
intention into a gate.

### IV. Deterministic Output, Isolated Side Effects

- Identical inputs and configuration MUST produce byte-identical HTML. Output MUST NOT depend
  on hash iteration order, locale, wall-clock time, or raw filesystem enumeration order;
  images are ordered by the natural-sort rule.
- Filesystem, network/SSH, and clock access MUST sit behind traits so the core is fully
  testable without a remote server.
- Every build MUST support a dry-run mode that performs no remote mutation and still produces
  the full local output.
- Uploads MUST be idempotent: re-running a build for the same news item converges on the same
  remote state rather than duplicating or accumulating files.

Rationale: the tool writes to a live web server. Non-determinism here is not a test nuisance,
it is production damage.

### V. Explicit Errors, No Panics, No Leaked Secrets

- Library code MUST NOT use `unwrap`, `expect`, `panic!`, or `process::exit` on reachable
  paths. Errors are typed and returned via `Result` with actionable context: which file, which
  marker, which image.
- Partial failure MUST be reported precisely rather than aborting the run: markers with no
  matching image, images never referenced, and per-image processing failures are surfaced as
  warnings, matching the reference's warning behavior.
- SSH passwords and private key material MUST NOT appear in logs, error messages, rendered
  output, or panic payloads. Persisting credentials to disk MUST be an explicit user opt-in,
  written with owner-only permissions (0600).

Rationale: a crash mid-upload leaves a half-published news item, and this tool handles
production deployment credentials by design.

## Technology and Output Constraints

- Language: Rust on a stable toolchain pinned via `rust-toolchain.toml`. Workspace layout is a
  core library crate plus separate CLI and GUI crates.
- Supported inputs: `.docx`, `.txt`, `.md`.
- The marker language is frozen: `[image:N]`, `[images:N,M,...]`, `[image-left:N]`,
  `[image-right:N]`, where `N` is a 1-based index into the natural-sorted image list.
- Generated HTML MUST use inline styles only — no `<style>` blocks, no external stylesheets,
  no classes used for styling. This is a hard CMS constraint, not a preference.
- Style and image-processing settings come from JSON configuration matching the reference
  schema and defaults (`max_width` 1600, `max_bytes` 512000, JPEG/WebP quality 85 with a floor
  of 50). The existing `style-presets/*.json` files MUST load unmodified.
- The image pipeline MUST apply EXIF orientation correction and MUST re-encode with decreasing
  quality to meet the byte budget before reporting failure.
- Remote layout is `<remote-path>/<slug>/<file>` with public URLs at
  `<public-base-url>/<slug>/<file>`. The slug is derived by transliterating the title unless
  explicitly overridden.
- Authentication accepts an SSH key or a password; at least one MUST be provided. Uploads use
  a native SFTP client, with any fallback transport documented in the plan that introduces it.
- Each new third-party crate on the network, image, or archive path MUST be justified in the
  implementation plan for the feature that introduces it.

## Development Workflow and Quality Gates

- Work flows through Spec Kit: `/speckit-specify` → `/speckit-plan` → `/speckit-tasks` →
  `/speckit-implement`. Every spec MUST state which reference behavior it ports, or state
  explicitly that it is new.
- No change merges unless all of the following pass: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` across the workspace, and the
  reference parity fixtures.
- Each change MUST assert constitution compliance. A violation is permitted only when
  justified in the plan's Complexity Tracking section.
- Fixtures and repository content MUST NOT contain real credentials, private key material, or
  production hostnames.
- User-facing flags, markers, and configuration keys MUST be documented in the README before a
  release that ships them.

## Governance

This constitution supersedes other project practices; where guidance conflicts, the
constitution wins.

- Amendment procedure: amendments are proposed as a change to this file, stating the rationale,
  the version bump, and a migration note for any spec, plan, or task list the change
  invalidates. An amendment takes effect when merged.
- Versioning policy: MAJOR for removing or redefining a principle in a backward-incompatible
  way, MINOR for a new principle or section or materially expanded guidance, PATCH for
  clarifications and wording fixes that do not change meaning.
- Compliance review: compliance is checked at planning time and again before merge. Accumulated
  deviations from the Python reference are reviewed at every release.
- Runtime development guidance for AI agents lives in `CLAUDE.md` at the repository root; it
  elaborates on this constitution and MUST NOT contradict it.

**Version**: 1.0.0 | **Ratified**: 2026-09-01 | **Last Amended**: 2026-09-01
