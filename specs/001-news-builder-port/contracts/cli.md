# Contract: command-line interface

Binary: `newsbuilder`. Deliberately small (FR-036 – FR-038): it builds and publishes a news item
that already needs no human decision. Arrangement, cropping, and editing belong to the desktop
application.

Its user is a technician automating repeat publishing, not the editor persona of the user
stories.

---

## `newsbuilder build`

Renders the fragment locally. No network, ever.

| Flag | Required | Meaning |
|------|----------|---------|
| `--input <PATH>` | yes | `.docx`, `.txt`, or `.md` source |
| `--images-dir <PATH>` | no | Photos for marker documents; unnecessary for `.docx` with embedded images |
| `--output <PATH>` | yes | Where the HTML fragment is written |
| `--title <TEXT>` | no | Overrides detected title |
| `--news-slug <TEXT>` | no | Overrides derived slug |
| `--public-base-url <URL>` | no | Without it, image URLs are local placeholders |
| `--appearance <PATH>` | no | JSON appearance override; see [appearance-config.md](./appearance-config.md) |
| `--json` | no | Machine-readable result on stdout |

## `newsbuilder publish`

Builds, then uploads. Takes every `build` flag plus:

| Flag | Required | Meaning |
|------|----------|---------|
| `--server <NAME>` | yes | A saved server configuration |
| `--dry-run` | no | Full build and plan, zero remote mutation (FR-031) |

`--server` names a configuration created in the desktop application or by `server add`. The
credential is fetched from the OS secret store; it is never a flag, so it cannot land in shell
history or a process listing (FR-040).

## `newsbuilder server`

`server list` / `server add` / `server remove` / `server set-credential <NAME>`.

`set-credential` reads the secret from a terminal prompt with echo disabled, or from stdin when
not a TTY. It is never accepted as an argument.

---

## Behaviour

**Refusals** — these exit non-zero *before* any photo is processed:

- The item still needs a human arrangement decision: photos present with no placement, and no
  markers to derive one from. The message points at the desktop application (edge case).
- No credential resolves for the named server (FR-029).
- The remote base path is missing or not writable (edge case).

**Warnings** go to stderr and do not stop the build (FR-034). A run that produced warnings still
exits 0 unless `--strict` is given.

**Parity**: `publish` on a prepared item produces the same fragment and the same remote result
as publishing that item from the desktop application. Both call `core::build` and
`core::publish` with the same arguments — there is no second code path (FR-039, SC-011).

**Text I/O**: results to stdout, diagnostics to stderr. `--json` switches stdout to a single
JSON object; stderr stays human-readable.

---

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success (warnings permitted unless `--strict`) |
| 1 | Usage error — bad flags, missing required argument |
| 2 | Input error — unreadable document, unsupported format, missing photo |
| 3 | Refusal — needs a human decision, or no credential |
| 4 | Processing error — a photo could not be brought within the size budget |
| 5 | Transport error — connection, authentication, or upload failed |

---

## `--json` output

```json
{
  "ok": true,
  "fragment_path": "/path/to/news.html",
  "slug": "den-konstitutsii",
  "dry_run": false,
  "photos": [
    { "name": "img1.jpg", "bytes": 412883, "quality": 78,
      "url": "https://example.org/images/news/w/den-konstitutsii/img1.jpg" }
  ],
  "warnings": [
    { "kind": "unused_photo", "detail": "img7.jpg is not referenced by any placement" }
  ]
}
```

On failure: `{"ok": false, "error": {"kind": "no_credential", "detail": "...", "exit": 3}}`.

No field of this document ever contains a password or key material.
