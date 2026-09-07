# Quickstart: validating the News Builder port

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-09-01

How to build the project and prove each user story actually works. Written to be run in order:
scenarios build on the setup above them.

---

## Prerequisites

| Need | Why |
|------|-----|
| Rust stable, as pinned in `rust-toolchain.toml` | Everything |
| Node 20+ and pnpm | The desktop frontend only; `core` and `cli` build without it |
| Tauri system dependencies (`webkit2gtk` on Linux, WebView2 on Windows) | The desktop application |
| Python 3.11+ | Only to regenerate parity fixtures |
| A reachable SSH account, or a local `sshd` | Only for publish scenarios |

Linux desktops additionally need a Secret Service daemon (`gnome-keyring-daemon` or KWallet) for
stored credentials. Without one, the application prompts per session — that is scenario 7.

---

## Setup

```bash
cargo build --workspace
cargo test --workspace                 # unit, golden, and integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

These four are the merge gate from the constitution's Development Workflow. They must be green
before any scenario below is considered meaningful.

Regenerating parity goldens (only when the pinned reference commit moves):

```bash
cargo run -p capture-reference -- --out fixtures/reference
git diff --stat fixtures/reference    # a non-empty diff is a behaviour change to review
```

---

## Scenario 1 — Word file in, laid-out page out (US1, P1)

Proves FR-001 – FR-005: no photo is exported by hand and no folder is chosen.

```bash
cargo run -p newsbuilder-cli -- build \
  --input fixtures/inputs/word-with-photos/input.docx \
  --output /tmp/news.html
```

**Expect**: exit 0. `/tmp/news.html` contains the document's title as an `<h1>`, its paragraphs
in order, and one `<img>` for every photo embedded in the `.docx`, each at the position it
occupied in the document. No `--images-dir` was passed and none was needed.

**Then, in the application**:

```bash
cargo tauri dev --config crates/desktop/tauri.conf.json
```

Open the same file. The photo list fills from the embedded images in document order. Edit a
paragraph — the preview updates without a rebuild button (FR-022). Nothing in the flow asked for
a folder.

---

## Scenario 2 — Publish and hand a fragment to the CMS (US2, P2)

Proves FR-026 – FR-034. Start with the dry run, which touches nothing:

```bash
cargo run -p newsbuilder-cli -- publish \
  --input fixtures/inputs/word-with-photos/input.docx \
  --output /tmp/news.html \
  --server test-server \
  --dry-run --json
```

**Expect**: `"dry_run": true`, a `photos` array with the URL each photo *would* receive, and no
change on the server — verify with `ssh test-server ls <remote-base-path>`.

Then publish for real by dropping `--dry-run`.

**Expect**: a folder named from the transliterated slug appears under the base path, holding
every used photo. Every `src` in the fragment is `{public_base_url}/{slug}/{file}`. The fragment
contains no `<style>`, no `<link>`, and no `class` attribute:

```bash
grep -E '<style|<link|class=' /tmp/news.html && echo "CONTRACT VIOLATION" || echo "inline styles only"
```

**Convergence** (FR-030, SC-009): run the same publish a second time.

**Expect**: it reports zero uploads and zero removals. Remote file listings before and after are
identical, timestamps included.

**Secrets** (FR-032, SC-010): run with `RUST_LOG=debug` and search the output, then search every
file the application writes.

```bash
grep -ri "$TEST_PASSWORD" ~/.config/newsbuilder /tmp/news.html 2>/dev/null && echo "LEAK" || echo "clean"
```

---

## Scenario 3 — Portrait photos keep their heads (US3, P3)

Proves FR-012 – FR-017 and measures SC-003.

In the application, add `fixtures/inputs/portraits/` — twenty portrait photographs of people.
Place each in a layout that forces a crop.

**Expect**: in all twenty, the subject's head survives the default framing. That is the SC-003
benchmark; record failures with the photo name, since each is a case for `frame.rs`.

Then open the crop control on one photo, drag the frame, and confirm the result updates live.
Publish, and check the uploaded file reflects the crop. Revert the crop and confirm the photo
returns to full frame.

**Source safety** (FR-015): checksum the inputs before and after the whole scenario.

```bash
find fixtures/inputs/portraits -type f -exec sha256sum {} + > /tmp/before.txt
# ... run the scenario ...
find fixtures/inputs/portraits -type f -exec sha256sum {} + | diff /tmp/before.txt -
```

**Expect**: no difference. The application never writes to a source photo.

---

## Scenario 4 — Build from loose photos, no folder (US4, P4)

Proves FR-006 – FR-011.

In the application: start a new item, drag a handful of photos onto the window, paste one more
from the clipboard, type the text, arrange, publish.

**Expect**: dropped photos append in drop order; the pasted image joins the list; reordering and
removal carry existing placements with them, never re-pointing at a different photo. Dropping a
`.pdf` is refused by name and adds nothing. Two photos named `IMG_0001.jpg` from different
folders both publish, under distinct names.

At no point does the flow require selecting a directory.

---

## Scenario 5 — Automatic arrangement (US5, P5)

Proves FR-018 – FR-021.

Load `fixtures/inputs/mixed-orientation/` — landscape and portrait photos and several
paragraphs of text — then trigger automatic arrangement.

**Expect**: every photo is placed; layouts vary with photo shape rather than being uniform;
surplus photos group into rows rather than disappearing. Change one placement by hand and
confirm no other placement moves. Arrange again and confirm the warning appears before your
manual placement is replaced.

**Determinism** (FR-024): build twice without editing between, and compare.

```bash
cmp /tmp/build-a.html /tmp/build-b.html && echo "byte-identical"
```

---

## Scenario 6 — Command line matches the application (FR-039, SC-011)

Publish one prepared item both ways, to different slugs on the same server, and compare the
fragments with the slug normalised away.

**Expect**: identical output. A difference means domain logic has leaked into a frontend, which
is a constitution principle II violation, not a cosmetic bug.

---

## Scenario 7 — Degraded and failing paths

| Situation | Expected behaviour |
|-----------|--------------------|
| Secret store unavailable (`systemctl --user stop gnome-keyring-daemon`) | Explained plainly, credential asked per session, nothing written to disk (FR-041) |
| Credential deleted from the store between publishes | Asked again, not an opaque auth failure |
| Server unreachable (wrong port) | Names the failed step; local work intact; exit 5 |
| Marker referencing a missing photo | Warning naming the marker; the rest still builds; exit 0 |
| Photo over budget at the quality floor | Warning naming that photo and the size reached; other photos still publish; exit 0, or 2 under `--strict`. Exit 4 is the fatal form, which the build path does not take (FR-034) |
| CLI run on an item needing an arrangement decision | Refused, pointing at the application; exit 3 |
| Connection dropped mid-upload | Names the file in flight; re-publishing completes the item |

Every message must name the specific offending item — that is what SC-007 measures, and a
message that says only "upload failed" is a defect.

---

## Acceptance summary

| Story | Scenario | Success criteria covered |
|-------|----------|--------------------------|
| US1 Word import | 1 | SC-001, SC-002 |
| US2 Publish | 2 | SC-004, SC-009, SC-010 |
| US3 Framing | 3 | SC-003, SC-006 |
| US4 Loose photos | 4 | SC-002 |
| US5 Arrangement | 5 | SC-008 |
| Interface parity | 6 | SC-011 |
| Failure behaviour | 7 | SC-007 |

SC-005 — a new editor publishing unaided, without written instructions — is not a command you
can run. Test it with a person who has not seen the application, and watch without helping.
