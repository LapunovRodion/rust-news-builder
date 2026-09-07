<!--
  T044 — the preview.

  **The one frontend rule worth failing a review over** (contracts/desktop-commands.md): the
  string `core::build` returned goes into the iframe unchanged. Not templated, not
  post-processed, not re-styled, and no part of the news page composed here. That is the whole
  reason the preview matches the export (FR-022) — it is the *same artefact*, not a second
  rendering of the same data.

  The iframe is sandboxed with no script execution. Fragments carry no scripts by contract; a
  sandbox means a malformed one cannot run anything either.
-->
<script lang="ts">
  const { fragment }: { fragment: string } = $props();

  // `srcdoc` needs a document, and a fragment is not one. This wrapper is the *frame* the CMS
  // will put the fragment into — it contributes nothing to what is exported, and nothing here
  // may style the fragment itself: every style in the output is inline, by contract (FR-023).
  const page = $derived(
    `<!doctype html><html><head><meta charset="utf-8">` +
      `<style>html{background:#fff;color:#111;font:16px/1.6 system-ui,sans-serif;}` +
      `body{margin:0;padding:24px;}</style></head><body>${fragment}</body></html>`,
  );
</script>

<section class="preview">
  <header><h2>Предпросмотр</h2></header>
  {#if fragment}
    <iframe title="Предпросмотр материала" sandbox="" srcdoc={page}></iframe>
  {:else}
    <p class="empty">Откройте документ или добавьте фотографии, чтобы увидеть страницу.</p>
  {/if}
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border-left: 1px solid var(--line);
  }
  header {
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin: 0;
  }
  iframe {
    flex: 1;
    border: none;
    width: 100%;
    background: #fff;
  }
  .empty {
    padding: 2rem 1.25rem;
    color: var(--muted);
  }
</style>
