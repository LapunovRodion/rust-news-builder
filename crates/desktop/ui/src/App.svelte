<!--
  T045, T086, T096 — the window.

  Three panes: the photographs, the text, and the page as it will be published. The third is not
  a rendering of the first two — it is the exact string `core::build` produced, which is what
  makes "what you see is what the CMS gets" true by construction rather than by discipline
  (FR-022).

  Photographs arrive through Tauri's own drag-and-drop event, not HTML5's: the webview cannot
  read a dropped file's path, and the core needs one (research R8).
-->
<script lang="ts">
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open, save, confirm, message } from '@tauri-apps/plugin-dialog';
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { onDestroy } from 'svelte';
  import * as api from './lib/api';
  import CropOverlay from './lib/CropOverlay.svelte';
  import Editor from './lib/Editor.svelte';
  import PhotoList from './lib/PhotoList.svelte';
  import Preview from './lib/Preview.svelte';
  import PublishDialog from './lib/PublishDialog.svelte';
  import ServerSettings from './lib/ServerSettings.svelte';
  import { addWarnings, applyItem, clearError, run, setWarnings, ui } from './lib/store.svelte';
  import type { BlockInput, CropRect, PhotoView } from './lib/types';

  let editor = $state<ReturnType<typeof Editor> | null>(null);
  let showServers = $state(false);
  let showPublish = $state(false);
  let cropping = $state<PhotoView | null>(null);
  let titleDraft = $state('');
  let slugDraft = $state('');
  let rebuildTimer: ReturnType<typeof setTimeout> | null = null;
  const unlisteners: Array<() => void> = [];

  $effect(() => {
    titleDraft = ui.item.title;
    slugDraft = ui.item.slug;
  });

  // --- The live preview (T045, SC-008) --------------------------------------------------
  //
  // Debounced rather than immediate: a keystroke should not queue a build behind the one still
  // running. 120 ms leaves room inside the 150 ms the requirement allows.
  const REBUILD_DELAY_MS = 120;

  function scheduleRebuild() {
    if (rebuildTimer) clearTimeout(rebuildTimer);
    rebuildTimer = setTimeout(rebuild, REBUILD_DELAY_MS);
  }

  async function rebuild() {
    const preview = await run(() => api.buildPreview());
    if (preview) {
      ui.fragment = preview.fragment;
      setWarnings(preview.warnings);
    }
  }

  // Any change to the item is a reason to rebuild. Reading `documentRevision` and the blocks
  // makes this run for structural changes; the body push below covers typing.
  $effect(() => {
    void ui.item;
    scheduleRebuild();
  });

  // --- Editing ---------------------------------------------------------------------------

  async function pushBody(blocks: BlockInput[]) {
    const item = await run(() => api.setBody(blocks));
    // `false`: the editor is the one that just changed, so rebuilding its document from the
    // answer would move the caret the editor is still typing at.
    if (item) applyItem(item, false);
  }

  async function commitTitle() {
    if (titleDraft === ui.item.title) return;
    const item = await run(() => api.setTitle(titleDraft));
    if (item) applyItem(item, false);
  }

  async function commitSlug() {
    if (slugDraft === ui.item.slug) return;
    const item = await run(() => api.setSlug(slugDraft));
    if (item) applyItem(item, false);
    else slugDraft = ui.item.slug;
  }

  // --- Documents -------------------------------------------------------------------------

  async function openDocument() {
    const chosen = await open({
      multiple: false,
      filters: [{ name: 'Документы', extensions: ['docx', 'txt', 'md'] }],
    });
    if (!chosen || Array.isArray(chosen)) return;
    const result = await run(() => api.openDocument(chosen));
    if (!result) return;
    applyItem(result.item);
    setWarnings(result.warnings);
    if (!result.titleDetected) {
      // FR-004: no title was found, and none is invented. The field is where it gets one.
      await message('Заголовок в документе не найден — впишите его в поле «Заголовок».', {
        title: 'Заголовок',
        kind: 'info',
      });
    }
  }

  async function startNew() {
    if (ui.item.dirty && !(await confirm('Несохранённые изменения будут потеряны. Продолжить?'))) {
      return;
    }
    const item = await run(() => api.newItem());
    if (item) {
      applyItem(item);
      setWarnings([]);
    }
  }

  async function exportFragment() {
    const path = await save({
      defaultPath: `${ui.item.slug}.html`,
      filters: [{ name: 'HTML', extensions: ['html'] }],
    });
    if (!path) return;
    await run(() => api.exportFragment(path));
  }

  async function copyFragment() {
    const fragment = await run(() => api.copyFragment());
    if (fragment) await writeText(fragment);
  }

  // --- Arrangement (T096, FR-020) ----------------------------------------------------------

  async function arrange() {
    const first = await run(() => api.arrangeAuto(false));
    if (!first) return;
    applyItem(first.item);

    // The first call changed nothing the editor had done, and says how much is in the way.
    // Only now is there a question worth asking.
    if (first.preservedManual > 0) {
      const replace = await confirm(
        `${first.preservedManual} размещений сделаны вручную. Заменить их автоматическими?`,
        { title: 'Разместить заново', kind: 'warning' },
      );
      if (replace) {
        const second = await run(() => api.arrangeAuto(true));
        if (second) applyItem(second.item);
      }
    }
  }

  // --- Photos ------------------------------------------------------------------------------

  function insertPhoto(id: string) {
    // At the caret, which is what FR-018a means by "where the editor is working".
    editor?.insertAtCaret([id], 'full-width');
  }

  async function applyCrop(crop: CropRect) {
    const photo = cropping;
    cropping = null;
    if (!photo) return;
    const item = await run(() => api.setCrop(photo.id, crop));
    if (item) applyItem(item);
  }

  async function revertCrop() {
    const photo = cropping;
    cropping = null;
    if (!photo) return;
    const item = await run(() => api.setCrop(photo.id, null));
    if (item) applyItem(item);
  }

  // --- Tauri's drag-and-drop (T086, research R8) --------------------------------------------

  $effect(() => {
    void (async () => {
      const stop = await getCurrentWebview().onDragDropEvent(async (event) => {
        if (event.payload.type !== 'drop') return;
        const paths = event.payload.paths;
        if (paths.length === 0) return;
        const result = await run(() => api.addPhotosFromPaths(paths));
        if (result) {
          applyItem(result.item);
          addWarnings(result.warnings);
        }
      });
      unlisteners.push(stop);
    })();
  });

  // --- Unsaved changes on close (T103) ------------------------------------------------------

  $effect(() => {
    void (async () => {
      const stop = await getCurrentWindow().onCloseRequested(async (event) => {
        if (!ui.item.dirty) return;
        const leave = await confirm(
          'Материал не выгружен и не опубликован. Закрыть и потерять изменения?',
          { title: 'Несохранённые изменения', kind: 'warning' },
        );
        if (!leave) event.preventDefault();
      });
      unlisteners.push(stop);
    })();
  });

  onDestroy(() => {
    if (rebuildTimer) clearTimeout(rebuildTimer);
    unlisteners.forEach((stop) => stop());
  });
</script>

<main>
  <nav>
    <button type="button" onclick={openDocument}>Открыть…</button>
    <button type="button" onclick={startNew}>Новый</button>
    <span class="gap"></span>
    <label class="field">
      Заголовок
      <input bind:value={titleDraft} onblur={commitTitle} placeholder="Заголовок материала" />
    </label>
    <label class="field slug">
      Адрес
      <input bind:value={slugDraft} onblur={commitSlug} />
    </label>
    <span class="gap"></span>
    <button type="button" onclick={exportFragment}>Выгрузить…</button>
    <button type="button" onclick={copyFragment}>Скопировать</button>
    <button type="button" onclick={() => (showServers = true)}>Серверы…</button>
    <button type="button" class="primary" onclick={() => (showPublish = true)}>Опубликовать…</button>
  </nav>

  <div class="panes">
    <PhotoList oninsert={insertPhoto} oncrop={(photo) => (cropping = photo)} />
    <Editor bind:this={editor} onBodyChange={pushBody} onArrange={arrange} />
    <Preview fragment={ui.fragment} />
  </div>

  <footer>
    {#if ui.error}
      <p class="error">
        {ui.error}
        <button type="button" onclick={clearError}>×</button>
      </p>
    {:else if ui.warnings.length > 0}
      <details>
        <summary>{ui.warnings.length} предупреждений</summary>
        <ul>
          {#each ui.warnings as warning, index (index)}
            <li>{warning.detail}</li>
          {/each}
        </ul>
      </details>
    {:else}
      <p class="quiet">{ui.item.photos.length} фотографий · {ui.item.slug}</p>
    {/if}
  </footer>
</main>

{#if showServers}
  <ServerSettings onclose={() => (showServers = false)} />
{/if}

{#if showPublish}
  <PublishDialog onclose={() => (showPublish = false)} />
{/if}

{#if cropping}
  <CropOverlay
    photo={cropping}
    suggest={(w, h) => api.suggestCrop(cropping!.id, w, h)}
    onconfirm={applyCrop}
    onrevert={revertCrop}
    oncancel={() => (cropping = null)}
  />
{/if}

<style>
  main {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100vh;
  }
  nav {
    display: flex;
    align-items: flex-end;
    gap: 0.4rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--line);
    background: var(--surface-raised);
  }
  .gap {
    flex: 1;
  }
  .field {
    display: flex;
    flex-direction: column;
    font-size: 0.7rem;
    color: var(--muted);
    gap: 0.1rem;
  }
  .field input {
    min-width: 16rem;
  }
  .field.slug input {
    min-width: 9rem;
    font-family: ui-monospace, monospace;
  }
  .panes {
    display: grid;
    grid-template-columns: 20rem minmax(24rem, 1fr) minmax(22rem, 1fr);
    min-height: 0;
  }
  footer {
    border-top: 1px solid var(--line);
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
    background: var(--surface-raised);
  }
  .error {
    color: var(--danger);
    margin: 0;
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .quiet {
    color: var(--muted);
    margin: 0;
  }
  details ul {
    margin: 0.3rem 0 0;
    padding-left: 1.1rem;
    color: var(--warn);
  }
  summary {
    cursor: pointer;
    color: var(--warn);
  }
</style>
