#!/usr/bin/env python3
"""Run the Python reference over ``fixtures/inputs/`` and write the goldens.

Constitution III makes these files the parity gate: the Rust tests compare against what the
reference actually produced, not against anyone's reading of it. Nothing here re-implements
reference behaviour -- every value written out comes from calling into ``news_builder``.

The upload step is the one thing stubbed. ``build_with_content(upload=False)`` is the
reference's own seam for that, so the captured path is otherwise the production one.

Invoked by ``tools/capture-reference/src/main.rs``; not meant to be run by hand.
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import traceback
from argparse import Namespace
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
INPUTS = REPO_ROOT / "fixtures" / "inputs"
REFERENCE = REPO_ROOT / "fixtures" / "reference"

# Fixed connection values, so goldens never depend on anyone's environment.
REMOTE_PATH = "/var/www/html/news"
PUBLIC_BASE_URL = "https://example.org/news/2026/03/"

# Base-path and base-URL pairs with and without trailing slashes, which is what
# tests/parity_paths.rs asserts against (T048).
PATH_VARIANTS = [
    ("/var/www/html/news", "https://example.org/news/2026/03/"),
    ("/var/www/html/news/", "https://example.org/news/2026/03"),
    ("/var/www/html/news//", "https://example.org/news/2026/03//"),
    ("news", "https://example.org/"),
    ("", "https://example.org/news"),
]

DOCUMENT_NAMES = ["input.txt", "input.md", "input.docx", "news.docx", "news.txt", "news.md"]


def find_document(case: Path) -> Path | None:
    for name in DOCUMENT_NAMES:
        candidate = case / name
        if candidate.is_file():
            return candidate
    return None


class SaveSpy:
    """Records the quality each ``Image.save`` was called with.

    The reference steps quality down until the encoded file fits ``max_bytes`` and does not
    report where it stopped. The quality reached is exactly what tests/parity_encode.rs needs,
    so it is observed here rather than inferred later.
    """

    def __init__(self) -> None:
        self.calls: list[dict] = []
        self._original = None

    def __enter__(self) -> "SaveSpy":
        from PIL import Image

        self._original = Image.Image.save
        spy = self

        def save(image, fp, format=None, **params):  # noqa: A002 - matching Pillow's signature
            result = spy._original(image, fp, format=format, **params)
            try:
                size = Path(str(fp)).stat().st_size
            except OSError:
                size = None
            spy.calls.append(
                {
                    "file": Path(str(fp)).name,
                    "format": params.get("format", format),
                    "quality": params.get("quality"),
                    "size": size,
                    "width": image.width,
                    "height": image.height,
                }
            )
            return result

        Image.Image.save = save
        return self

    def __exit__(self, *exc) -> None:
        from PIL import Image

        if self._original is not None:
            Image.Image.save = self._original

    def final_per_file(self) -> list[dict]:
        """The last save for each output file: the encode the reference actually kept."""
        last: dict[str, dict] = {}
        attempts: dict[str, int] = {}
        for call in self.calls:
            last[call["file"]] = call
            attempts[call["file"]] = attempts.get(call["file"], 0) + 1
        out = []
        for name in sorted(last):
            record = dict(last[name])
            record["attempts"] = attempts[name]
            out.append(record)
        return out


def capture_case(news_builder, case: Path, out: Path) -> dict:
    """Captures every observable the reference produces for one input case."""
    out.mkdir(parents=True, exist_ok=True)
    summary: dict = {"case": case.name}
    log_lines: list[str] = []

    def logger(message: str) -> None:
        log_lines.append(message)

    document = find_document(case)
    if document is None:
        summary["skipped"] = "no input document"
        (out / "SKIPPED").write_text(summary["skipped"] + "\n", encoding="utf-8")
        return summary

    summary["document"] = document.name

    # --- Reading: title detection and body normalisation -------------------------------
    try:
        detected_title, body = news_builder.read_input_document(document)
    except Exception as exc:
        (out / "read_error.txt").write_text(f"{type(exc).__name__}: {exc}\n", encoding="utf-8")
        summary["read_error"] = str(exc)
        return summary

    (out / "title.txt").write_text((detected_title or "") + "\n", encoding="utf-8")
    (out / "title_detected.txt").write_text(
        "none" if detected_title is None else "some", encoding="utf-8"
    )
    (out / "body.txt").write_text(body + "\n", encoding="utf-8")

    # --- Marker parsing ----------------------------------------------------------------
    try:
        blocks = news_builder.parse_blocks(body)
        (out / "blocks.json").write_text(
            json.dumps(
                [
                    {"kind": "paragraph", "text": block.text}
                    if type(block).__name__ == "ParagraphBlock"
                    else {"kind": "placement", "layout": block.layout, "indices": list(block.indices)}
                    for block in blocks
                ],
                indent=2,
                ensure_ascii=False,
            )
            + "\n",
            encoding="utf-8",
        )
    except Exception as exc:
        (out / "blocks_error.txt").write_text(f"{type(exc).__name__}: {exc}\n", encoding="utf-8")
        summary["blocks_error"] = str(exc)

    title = news_builder.normalize_title(detected_title or "")

    # --- Slug and path construction ----------------------------------------------------
    if title:
        slug = news_builder.slugify(title)
        (out / "slug.txt").write_text(slug + "\n", encoding="utf-8")
        folder = news_builder.build_news_folder_name(title, None)
        # A title that transliterates to nothing falls back to `news-<timestamp>`, which would
        # put a fresh diff in the goldens on every capture. Mask the clock: the *rule* is the
        # thing worth pinning, and Clock is a port precisely so the port can reproduce it.
        timestamped = slug == "news" and folder != "news"
        if timestamped:
            folder = "news-YYYYmmdd-HHMMSS"
        variants = [
            {
                "remote_base_path": base_path,
                "public_base_url": base_url,
                "remote_path": news_builder.build_news_remote_path(base_path, folder),
                "public_url_base": news_builder.build_news_public_base_url(base_url, folder),
            }
            for base_path, base_url in PATH_VARIANTS
        ]
        (out / "paths.json").write_text(
            json.dumps(
                {
                    "title": title,
                    "slug": slug,
                    "news_folder": folder,
                    "news_folder_is_timestamped": timestamped,
                    "variants": variants,
                },
                indent=2,
                ensure_ascii=False,
            )
            + "\n",
            encoding="utf-8",
        )

    # --- Rendering and image processing ------------------------------------------------
    images_dir = case / "images"
    if not images_dir.is_dir():
        (out / "NO_IMAGES").write_text(
            "this case carries no images/ directory, so the reference cannot render it;\n"
            "its text goldens above are still authoritative\n",
            encoding="utf-8",
        )
        summary["rendered"] = False
        _write_log(out, log_lines)
        return summary

    args = Namespace(
        input=str(document),
        images_dir=str(images_dir),
        output=str(out / "fragment.html"),
        full_output=None,
        title=None,
        remote_host="example.invalid",
        remote_user="editor",
        remote_path=REMOTE_PATH,
        remote_port=22,
        ssh_key=None,
        ssh_password=None,
        public_base_url=PUBLIC_BASE_URL,
        news_slug=None,
        style_config=None,
        keep_temp=True,
    )

    with SaveSpy() as spy:
        try:
            result = news_builder.build_with_content(
                args=args,
                title=title,
                body=body,
                images_dir=images_dir,
                output_path=Path(args.output),
                logger=logger,
                upload=False,
            )
            summary["rendered"] = True
            # Same clock problem as above: a title with no letters produces a timestamped
            # folder, which reaches the fragment through every image URL. Mask it so the
            # golden pins structure rather than the second it was captured in.
            mask = _timestamp_mask(news_builder, title, result.news_folder)
            (out / "build.json").write_text(
                json.dumps(
                    {
                        "news_folder": mask(result.news_folder),
                        "remote_path": mask(result.remote_path),
                        "public_base_url": mask(result.public_base_url),
                    },
                    indent=2,
                    ensure_ascii=False,
                )
                + "\n",
                encoding="utf-8",
            )
            fragment_path = out / "fragment.html"
            fragment_path.write_text(
                mask(fragment_path.read_text(encoding="utf-8")), encoding="utf-8"
            )
        except Exception as exc:
            summary["rendered"] = False
            summary["build_error"] = str(exc)
            # A refusal is itself the reference behaviour worth pinning: the port turns
            # several of these into warnings (FR-034), and the golden records what changed.
            (out / "build_error.txt").write_text(
                f"{type(exc).__name__}: {exc}\n", encoding="utf-8"
            )
            (out / "build_traceback.txt").write_text(traceback.format_exc(), encoding="utf-8")

    (out / "encode.json").write_text(
        json.dumps(spy.final_per_file(), indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    # keep_temp copies the processed images next to the fragment. Their sizes are already in
    # encode.json, and committing megabytes of re-encoded photos would bloat the repository.
    processed_dir = out / "fragment_processed_images"
    if processed_dir.is_dir():
        shutil.rmtree(processed_dir)

    _write_log(out, log_lines)
    return summary


def _timestamp_mask(news_builder, title: str, news_folder: str):
    """Returns a function replacing a timestamped folder name with a stable placeholder."""
    if news_builder.slugify(title) != "news" or news_folder == "news":
        return lambda value: value
    return lambda value: value.replace(news_folder, "news-YYYYmmdd-HHMMSS")


def _write_log(out: Path, log_lines: list[str]) -> None:
    """Writes the reference's own log, which is where its warning text lives."""
    import re as _re

    # The timestamped-folder fallback reaches the log as well; mask it for the same reason.
    log_lines = [_re.sub(r"news-\d{8}-\d{6}", "news-YYYYmmdd-HHMMSS", line) for line in log_lines]
    (out / "log.txt").write_text("\n".join(log_lines) + ("\n" if log_lines else ""), encoding="utf-8")
    warnings = [line for line in log_lines if line.startswith("Warning:")]
    (out / "warnings.txt").write_text(
        "\n".join(warnings) + ("\n" if warnings else ""), encoding="utf-8"
    )


def capture_styles(news_builder, out: Path) -> None:
    """Pins the reference's built-in appearance and every shipped preset.

    ``crates/core`` hard-codes ``DEFAULT_STYLES`` as its one built-in appearance (FR-025), and
    the appearance loader must keep reading the preset files unmodified. Both claims are only
    checkable against the values the reference actually holds.
    """
    out.mkdir(parents=True, exist_ok=True)
    (out / "default_config.json").write_text(
        json.dumps(news_builder.DEFAULT_CONFIG, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    container_cases = {
        "built_in": news_builder.DEFAULT_STYLES["container"],
        "omit": "__omit__",
        "empty": "",
        "no_trailing_semicolon": "color: red",
    }
    (out / "container_style.json").write_text(
        json.dumps(
            {name: news_builder.build_container_style(value) for name, value in container_cases.items()},
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    (out / "transliteration.json").write_text(
        json.dumps(news_builder.CYRILLIC_TRANSLIT, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    presets_src = Path(news_builder.__file__).parent / "style-presets"
    if presets_src.is_dir():
        presets_dst = out / "style-presets"
        if presets_dst.exists():
            shutil.rmtree(presets_dst)
        shutil.copytree(presets_src, presets_dst)


def capture_natural_sort(news_builder, out: Path) -> None:
    """Pins the ordering rule on names chosen to break a bytewise sort."""
    names = [
        "photo1.jpg", "photo2.jpg", "photo10.jpg", "photo12.jpg", "photo02.jpg",
        "Photo3.JPG", "img_20260101_001.jpg", "img_20260101_010.jpg",
        "a.jpg", "a1.jpg", "a10.jpg", "a2.jpg", "1.jpg", "10.jpg", "2.jpg",
        "фото1.jpg", "фото10.jpg", "фото2.jpg",
    ]
    ordered = sorted((Path(name) for name in names), key=news_builder.natural_sort_key)
    out.mkdir(parents=True, exist_ok=True)
    (out / "natural_sort.json").write_text(
        json.dumps(
            {"input": names, "ordered": [path.name for path in ordered]},
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )


def capture_slugs(news_builder, out: Path) -> None:
    """Pins the transliteration on inputs a table-driven port would get subtly wrong."""
    samples = [
        "День Конституции", "Здравствуй, мир!", "Ёлки-палки", "Съезд подъездов",
        "Обычный текст", "і ї є ў", "Іван Їжак Європа Ўзвышша",
        "Hello & Goodbye", "Hello  --  World", "!!!", "", "   ",
        "Ъ Ь Ы Э Ю Я", "щука", "ЩУКА", "Цыплёнок", "2026 год", "Café Naïve",
        "МКА-2026: итоги", "a/b\\c", "tabs\tand\nnewlines",
    ]
    out.mkdir(parents=True, exist_ok=True)
    (out / "slugs.json").write_text(
        json.dumps(
            [{"input": value, "slug": news_builder.slugify(value)} for value in samples],
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )


def capture_normalisation(news_builder, out: Path) -> None:
    """Pins the text-normalisation rules that decide paragraph boundaries."""
    samples = [
        "Title  \n\n\nBody​   text",
        "  leading and trailing  ",
        "a\r\nb\r\nc",
        "one\n\n\n\n\ntwo",
        "tabs\t\tcollapse",
        "﻿bom at the start",
        "line one\nline two\n\npara two",
    ]
    split_samples = [
        "First.\n\n[image:1]\n\nSecond.",
        "a\nb\n\nc\nd",
        "\n\n\n",
        "single",
    ]
    out.mkdir(parents=True, exist_ok=True)
    (out / "normalisation.json").write_text(
        json.dumps(
            {
                "normalize_text_content": [
                    {"input": value, "output": news_builder.normalize_text_content(value)}
                    for value in samples
                ],
                "split_paragraphs": [
                    {"input": value, "output": news_builder.split_paragraphs(value)}
                    for value in split_samples
                ],
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, help="Checkout of the Python reference.")
    parser.add_argument("--commit", required=True, help="The pinned commit being captured.")
    parser.add_argument("--case", action="append", help="Capture only these cases.")
    args = parser.parse_args()

    sys.path.insert(0, args.reference)
    import news_builder  # noqa: E402 - the path must be set first

    REFERENCE.mkdir(parents=True, exist_ok=True)

    cases = sorted(path for path in INPUTS.iterdir() if path.is_dir())
    if args.case:
        wanted = set(args.case)
        cases = [case for case in cases if case.name in wanted]

    summaries = []
    for case in cases:
        summary = capture_case(news_builder, case, REFERENCE / case.name)
        summaries.append(summary)
        note = summary.get("skipped") or summary.get("build_error") or "ok"
        print(f"  {case.name}: {note}")

    tables = REFERENCE / "_tables"
    capture_styles(news_builder, tables)
    capture_natural_sort(news_builder, tables)
    capture_slugs(news_builder, tables)
    capture_normalisation(news_builder, tables)
    print("  _tables: styles, natural sort, slugs, normalisation")

    (REFERENCE / "PINNED_COMMIT").write_text(args.commit + "\n", encoding="utf-8")
    (REFERENCE / "MANIFEST.json").write_text(
        json.dumps({"commit": args.commit, "cases": summaries}, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
