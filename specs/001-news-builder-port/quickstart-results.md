# Quickstart results — T106

**Feature**: [spec.md](./spec.md) | **Scenarios**: [quickstart.md](./quickstart.md)
**Run**: 2026-09-03, Linux, 16 cores, `nix develop` toolchain (rustc 1.98.0)

Every scenario in [quickstart.md](./quickstart.md) was run, with one exception that cannot be
run by a machine at all. What follows records what each one actually did, not what it was
expected to do.

---

## Summary

| Scenario | Covers | Result |
|----------|--------|--------|
| Setup — the four merge gates | — | **PASS** |
| 1 — Word file in, laid-out page out | SC-001, SC-002 | **PASS** (CLI half); desktop half **not run** — needs a display |
| 2 — Publish and hand a fragment to the CMS | SC-004, SC-009, SC-010 | **PASS**, via `just e2e` against a real `sshd` |
| 3 — Portrait photos keep their heads | SC-003, SC-006 | **PASS** (benchmark + source safety); interactive crop **not run** — needs a display |
| 4 — Build from loose photos, no folder | SC-002 | **Not run** — entirely a desktop interaction. Covered by tests, see below |
| 5 — Automatic arrangement | SC-008 | **PASS** (determinism + arrangement tests); interactive half **not run** |
| 6 — Command line matches the application | SC-011 | **PASS** |
| 7 — Degraded and failing paths | SC-007 | **PASS**, with one correction to the scenario's own table |
| SC-005 — a new editor publishing unaided | SC-005 | **OUTSTANDING — requires a person.** See below |

Two defects were found by running these, both now fixed. Two errors in `quickstart.md` itself
were found and corrected.

---

## Setup — the merge gates

```
cargo fmt --all -- --check      clean
cargo clippy --all-targets --all-features -- -D warnings    clean
cargo test --workspace          all suites pass
just parity                     the captured goldens match
```

---

## Scenario 1 — Word file in, laid-out page out

```bash
cargo run -p newsbuilder-cli -- build \
  --input fixtures/inputs/word-with-photos/input.docx \
  --output /tmp/news.html
```

Exit 0. `<h1>Word With Photos</h1>` from the document's own first paragraph; **3 `<img>`
elements for the 3 images embedded in the `.docx`** (`unzip -l` confirms `word/media/` holds
three), each at the position it occupied. No `--images-dir` was passed and none was needed —
which is FR-001 through FR-003 and the whole of SC-002 for the Word path.

The fragment carries no `<style>`, no `<link>` and no `class` attribute.

> **Correction to quickstart.md.** The scenario named
> `fixtures/inputs/word-with-photos/news.docx`; the fixture is `input.docx`. Corrected in both
> places it appeared.

**Not run**: the `cargo tauri dev` half — opening the same file in the application and watching
the preview follow a keystroke. It needs a display. The rebuild path it exercises is measured
directly instead, in `crates/core/tests/preview_latency.rs` (see scenario 5).

---

## Scenario 2 — Publish and hand a fragment to the CMS

Run as `just e2e`, which stands up a throwaway `sshd` on loopback in a temporary directory and
publishes the `markers` fixture through the real SFTP adapter. The scenario's five claims, in
its own order:

| Claim | Result |
|-------|--------|
| Dry run touches nothing (FR-031) | **PASS** — the folder does not exist afterwards |
| The photos land where the fragment says (FR-026) | **PASS** — every reported URL ends in a file the server holds, and every URL appears in the fragment |
| Inline styles only | **PASS** — `grep -E '<style|<link|class='` finds nothing |
| Re-publishing converges (FR-030, SC-009) | **PASS** — zero uploads, and the directory is byte-for-byte what it was |
| No secret in what is produced (FR-032, SC-010) | **PASS** — asserted in `parity_with_desktop.rs` and in `no_secret_leak.rs` |

The host-key check (deviation D-15) and a rejected credential were run as part of the same
sequence: an unknown host key is refused with a message naming `known_hosts`, and a wrong
passphrase fails as a transport error that names the key without echoing the secret.

> **Defect found and fixed.** The first live publish failed: every upload returned *"No such
> file"*. `russh_sftp`'s `SftpSession::write` opens with `OpenFlags::WRITE` alone — no `CREATE`
> — so it cannot write a file that is not already there, which is every first publish. It would
> also have written over the front of a shorter replacement and left the old tail behind.
> `SftpTransport::put` now uses `create` (`CREATE | TRUNCATE | WRITE`) and closes the handle.
> No fake could have caught this; it is exactly the thin layer T099 exists for.

---

## Scenario 3 — Portrait photos keep their heads

The SC-003 benchmark is `crates/core/tests/portrait_benchmark.rs`, against the twenty annotated
portraits in `fixtures/inputs/portraits/`. It passes: the default framing keeps the whole
annotated head in all twenty photographs across all three offered shapes, and a photo added to
an item carries no crop at all, so nothing is cropped without the editor asking. The same file
records that a naive centred crop loses heads on this set, so the rule is doing work.

**Source safety (FR-015)** was run exactly as the scenario writes it:

```bash
find fixtures/inputs/portraits -type f -exec sha256sum {} + | sort > before.txt
# ... a full build over those photos ...
find fixtures/inputs/portraits -type f -exec sha256sum {} + | sort | diff before.txt -
```

No difference. `crates/core/tests/source_untouched.rs` makes the same claim through a full
crop–rotate–build cycle.

**Not run**: dragging the crop frame and watching the result update live. Needs a display.

---

## Scenario 4 — Build from loose photos, no folder

Entirely a desktop interaction — dropping files, pasting from the clipboard, reordering — so
none of it was run here. The rules underneath it are covered by tests that were run:

| Claim | Test |
|-------|------|
| Dropped photos append in drop order; reordering and removal carry placements | `crates/core/tests/photo_management.rs` |
| A `.pdf` is refused by name and adds nothing | `crates/core/tests/unsupported_input.rs` |
| Two photos named `IMG_0001.jpg` both publish, under distinct names | `crates/core/tests/duplicate_names.rs` |
| Clipboard and dropped photos alike | `crates/core/tests/photo_sources.rs` |

---

## Scenario 5 — Automatic arrangement

**Determinism (FR-024)**, run as the scenario writes it:

```bash
cargo run -p newsbuilder-cli -- build --input fixtures/inputs/mixed-orientation/input.txt \
  --images-dir fixtures/inputs/mixed-orientation/images --output /tmp/build-a.html
# … the same again into build-b.html …
cmp /tmp/build-a.html /tmp/build-b.html      # byte-identical
```

Byte-identical. The arrangement rules themselves — every photo placed, layout varying with
shape, surplus photos grouped into rows rather than dropped, one manual override moving nothing
else — are asserted in `arrange.rs`, `arrange_determinism.rs` and `arrange_manual_guard.rs`,
all passing.

**SC-008**, which the acceptance table hangs on this scenario, was measured directly rather than
by eye. `just bench-preview`, optimised, thirty photos of 2400×1600 and 1600×2400:

| Measurement | Budget | Result |
|-------------|--------|--------|
| Rebuild after a text edit (worst of ten) | 150 ms | **393 µs** |
| Cold thirty-photo rebuild | 1 s | **513 ms** |
| Rebuild while dragging a crop (worst of five) | 150 ms | **33 ms** |

> **Defect found and fixed.** Both numbers were originally far outside the budget: a text edit
> cost **3.9 s** and a cold rebuild **3.88 s**, because every rebuild re-encoded every placed
> photo — a decode, a resize and a quality-ladder search each, thirty of them, on every
> keystroke. Two changes:
>
> 1. `core::build_with_cache` keeps encoded photos between rebuilds, keyed by everything that
>    decides their bytes and deliberately *not* by the published name, so retitling is cheap
>    too. A text edit now re-encodes nothing at all. 3.9 s → 694 µs.
> 2. The photos that *do* need encoding are encoded across threads. A cold rebuild is the one
>    case where the cache cannot help, and thirty independent jobs were being run one at a
>    time. 3.88 s → 513 ms.
>
> `preview_latency.rs` asserts that a cached rebuild is byte-for-byte an uncached one, so the
> optimisation cannot quietly change output and slip past the parity suite.

---

## Scenario 6 — Command line matches the application

`crates/cli/tests/parity_with_desktop.rs`, six tests, all passing. One prepared item is
published to two different folders — one through the real `newsbuilder` binary as a subprocess,
one through `core::publish` driven the way the Tauri command drives it — and the two fragments
are compared with the folder normalised away. They are identical, and the raw fragments are
*not*, so the normalisation is not doing the work.

The photo names, the byte counts and the dry-run plan match as well, and neither side's output
carries the credential.

---

## Scenario 7 — Degraded and failing paths

Every row was run against the real binary.

| Situation | Expected | Observed |
|-----------|----------|----------|
| Marker referencing a missing photo | Warning naming the marker, rest builds, exit 0 | `warning: marker [image:7] has no matching photo`, exit **0** ✓ |
| The same, under `--strict` | Non-zero | exit **2** ✓ |
| CLI run on an item needing an arrangement decision | Refused, points at the application, exit 3 | Refusal naming the photo count and pointing at the desktop application, exit **3** ✓ |
| Unsupported input format | Named, exit 2 | ``unsupported format for `README.md.pdf` ``, exit **2** ✓ |
| No such server / no credential | Refusal, exit 3 | ``there is no saved server called `no-such-server` `` plus what to do about it, exit **3** ✓ |
| Bad flags | Usage error, exit 1 | exit **1** ✓ |
| Photo over budget at the quality floor | *table said exit 4* | Warning naming the photo and the size — `photo 'oversized-01.jpg' is still 720673 bytes at the minimum quality of 50; it was published anyway` — other photos still publish, exit **0** |
| Server unreachable / connection dropped mid-upload | Names the failed step | Covered by `publish_refusals.rs` and by the wrong-passphrase phase of `just e2e`; both name the step |
| Secret store unavailable | Explained plainly, asked per session, nothing written to disk | `publish_refusals.rs` asserts the refusal; the per-session fallback is `desktop::commands::publish::ResolvedSecrets` |

> **Correction to quickstart.md.** The over-budget row claimed exit 4. FR-034 makes this a
> warning the build survives, so exit 0 is the correct behaviour and exit 4 is the fatal form
> the build path does not take. The row has been corrected rather than the code.

**SC-007 itself** — that *every* message names its offending file, marker or photo — is now
audited mechanically rather than by reading. `crates/core/src/error.rs` carries an exhaustive
audit that will not compile when a variant is added without saying what that variant's offending
item is. One real gap was closed in the process: `Error::DocumentUnreadable` never named the
file, because import is handed bytes and does not know it. `Error::in_file` now attaches it at
the frontends, which do.

---

## SC-005 — outstanding

> *"An editor who has never seen the application publishes their first news item without written
> instructions."*

**This has not been done, and cannot be done by running anything.** It needs a person who has
not seen the application, sat in front of it with a Word file and a server configured, watched
and not helped. quickstart.md says so itself.

Everything else in this document can be re-run on demand; this one needs scheduling. It is the
only acceptance criterion in the feature that is still open.

What to record when it is run: whether they published unaided, where they hesitated, and what
they tried that the interface did not offer. A failure here is a design finding, not a bug
report.
