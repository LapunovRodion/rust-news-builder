# Quickstart: validating site insertion

**Feature**: [spec.md](./spec.md) | Contracts: [cli](./contracts/cli.md),
[bridge](./contracts/bridge-protocol.md), [desktop](./contracts/desktop-commands.md)

Run against a **test** Joomla site only. `newsroom` below is a saved server (001 quickstart) whose
SSH account can read the Joomla install. Host names are placeholders.

---

## 0. Prerequisites (research R10)

```bash
ssh editor@test-host 'php -v | head -1; ls /var/www/html/configuration.php'
```

Expected: PHP ≥ 8.1 (Joomla 5) or ≥ 7.2.5 (Joomla 4), and the file listed. Either failing stops
here — see research R10.

## 1. Automated gates

```bash
just gates          # includes the Site fake tests, the bridge goldens, the no-delete static test, php -l
just e2e-joomla     # opt-in: Joomla 5 + MariaDB + sshd in containers
```

Expected: all pass. `e2e-joomla` covers scenarios 3–7 below against a throwaway site.

## 2. Configure and check (FR-011, FR-017)

```bash
newsbuilder server site set newsroom --joomla-root /var/www/html \
  --site-url https://test-host/ --category <ID>
newsbuilder server site check newsroom
```

Expected: the Joomla version and lists of categories, access levels, languages and authors with
ids. Nothing on the site changes (compare the admin article list before and after).

## 3. Dry run writes nothing (FR-009, SC-004)

```bash
newsbuilder publish --input fixtures/inputs/word-with-photos/news.docx --server newsroom --dry-run --json
```

Expected: `article.outcome` = `would_create` with the resolved settings; article count on the site
unchanged.

## 4. Create (US1, FR-001, FR-003)

Same command without `--dry-run`.

Expected: exit 0, `article.outcome` = `created`, `article.url` opens the article, published
(Q3), in the chosen category. The front page shows the announcement cut at the readmore marker;
the article page shows the full text and every photo. Opening it in the Joomla editor shows the
same content a hand-pasted fragment would.

## 5. Unchanged and update (US2, SC-003)

Run step 4 again → `unchanged`, the article's "Modified" date does not move. Edit one word in
the document and run again → `updated`, same `id`. After five runs the site holds one article with
this alias.

## 6. Edited on site (FR-007)

Change the article's text in the Joomla admin, then publish again.

Expected: exit 3, reason `edited_on_site`, the article untouched. With `--overwrite-article`:
`updated`.

## 7. Gone and trashed (US2 sc3)

Trash the article in Joomla, publish → exit 3, `trashed`; with `--overwrite-article` it is restored
and updated. Delete it from the trash, publish → exit 3, `gone`; with `--create-article` a new
article is created.

## 8. Failure keeps the work (US1 sc5, SC-005)

Set `--joomla-root /nonexistent`, publish.

Expected: exit 5, photos reported as uploaded, `article.outcome` = `failed` with step
"starting Joomla", and the fragment printed.

## 9. Desktop parity (US3, US4, FR-016)

In the application: Серверы → the server → Сайт Joomla → Проверить подключение; choose defaults;
open the same document; Публикация → override category and publish date → Опубликовать.

Expected: the link is shown; the article carries the overrides and the server defaults otherwise.
Publishing the same item from the CLI with the same overrides reports `unchanged`.

## 10. No secret leaks (SC-006)

```bash
grep -rI "password\|dbpass\|\$password" ~/.config/newsbuilder/ ; newsbuilder publish … --json | grep -i pass
```

Expected: no output.
