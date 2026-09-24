# Contract: core API additions

Additions to `newsbuilder-core`. Types are specified in [data-model.md](../data-model.md); this
file fixes the functions and ports. Nothing in `specs/001-news-builder-port/contracts/core-api.md`
changes signature.

---

## Port

```rust
/// The route to the site's articles. Like `Transport`, it has no way to delete (FR-008).
pub trait Site {
    fn describe(&mut self, target: &SiteTarget) -> Result<SiteCatalog>;
    fn find(&mut self, target: &SiteTarget, probe: &ArticleProbe) -> Result<FindResult>;
    fn save(&mut self, target: &SiteTarget, write: &ArticleWrite) -> Result<SavedArticle>;
}
```

`Site` is used only on a connection already opened by `Transport::connect`. The SFTP adapter
(`adapters::transport::SftpTransport`) implements both traits on one SSH session; the test fake
records calls in memory.

## Functions

```rust
/// Unchanged: photos only.
pub fn publish(item, server, transport, secrets, mode, sources) -> Result<Publication>;

/// Photos, then the article when `server.site` is set. Photo failures are `Err` and no article is
/// touched (FR-004). Article failures are `Ok` with `article: Some(ArticleOutcome::Failed { .. })`,
/// so the fragment is never lost (FR-015).
pub fn publish_to_site<R: Transport + Site>(
    item: &NewsItem,
    server: &ServerConfig,
    remote: &mut R,
    secrets: &dyn SecretStore,
    mode: PublishMode,
    confirmation: ArticleConfirmation,
    sources: &dyn PhotoBytesSource,
) -> Result<Publication>;

/// Connects and describes the site without writing (FR-011, FR-017).
pub fn check_site<R: Transport + Site>(
    server: &ServerConfig,
    remote: &mut R,
    secrets: &dyn SecretStore,
) -> Result<SiteCatalog>;

/// Field-by-field merge, override first (research R7).
impl ArticleSettings {
    pub fn resolve(overrides: &Self, defaults: &Self) -> Self;
}
```

`publish_to_site` with `server.site == None` returns exactly what `publish` returns (INV-S5).

## Order inside `publish_to_site`

1. INV-S1 check (default category present) — before any photo is processed.
2. `publish(...)` as today. `Err` returns immediately.
3. `find`, then the research R6 matrix, honouring `confirmation`.
4. `DryRun`: `WouldCreate` / `WouldUpdate` / `Unchanged` / `NeedsConfirmation`; no `save`.
5. `Live`: `save` when the matrix says create/update; outcome recorded on `Publication.article`.

## New errors

| Variant | Exit (CLI) | Message names |
|---------|-----------|---------------|
| `SiteIncomplete { server, field }` | 3 | The server and the missing field |
| `SiteUnsupported { found }` | 5 | The Joomla version found |
| `CategoryMissing { id }` | 3 | The category id |
| `AliasTaken { alias, article }` | 3 | The alias and the article id holding it |
| `ArticleAmbiguous { alias, ids }` | 3 | Every article id found |
| `SiteBridge { step, detail }` | 5 | The step: "starting Joomla", "reading articles", "saving the article" |

Errors raised after photos uploaded are carried as `ArticleOutcome::Failed` rather than returned.
