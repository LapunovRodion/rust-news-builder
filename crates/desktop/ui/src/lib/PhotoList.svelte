<!--
  T078, T087, T088 — the photographs the item holds.

  Order here is the item's order, which is what decides published names, so reordering is a real
  edit and not a display preference. Every photograph says whether it is in the text (FR-010),
  because a photograph that is in the item but nowhere on the page is the one mistake this list
  exists to make visible.
-->
<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { readImage } from '@tauri-apps/plugin-clipboard-manager';
  import { open } from '@tauri-apps/plugin-dialog';
  import * as api from './api';
  import { addWarnings, applyItem, run, ui } from './store.svelte';
  import type { PhotoView } from './types';

  const { oninsert, oncrop }: { oninsert: (id: string) => void; oncrop: (photo: PhotoView) => void } =
    $props();

  let renaming = $state<string | null>(null);
  let draft = $state('');

  async function pick() {
    const chosen = await open({
      multiple: true,
      filters: [
        { name: 'Изображения', extensions: ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'tif', 'tiff'] },
      ],
    });
    if (!chosen) return;
    const paths = Array.isArray(chosen) ? chosen : [chosen];
    const result = await run(() => api.addPhotosFromPaths(paths));
    if (result) {
      applyItem(result.item);
      addWarnings(result.warnings);
    }
  }

  /** Clipboard paste (FR-007). The permission lives in the webview, so the read happens here. */
  async function paste() {
    const result = await run(async () => {
      const image = await readImage();
      const bytes = await image.rgba();
      // `rgba()` gives raw pixels, which the core cannot sniff a format from. PNG is what a
      // paste is, so encode it here — the one image operation the frontend performs, and it
      // touches no rule the core owns.
      return api.addPhotoFromClipboard(Array.from(await toPng(bytes, image)));
    });
    if (result) {
      applyItem(result.item);
      addWarnings(result.warnings);
    }
  }

  async function toPng(
    rgba: Uint8Array,
    image: { size: () => Promise<{ width: number; height: number }> },
  ): Promise<Uint8Array> {
    const { width, height } = await image.size();
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('изображение из буфера обмена не удалось прочитать');
    context.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
    const blob = await canvas.convertToBlob({ type: 'image/png' });
    return new Uint8Array(await blob.arrayBuffer());
  }

  async function remove(id: string) {
    const result = await run(() => api.removePhoto(id));
    if (result) {
      applyItem(result.item);
      addWarnings(result.warnings);
    }
  }

  async function moveBy(index: number, delta: number) {
    const order = ui.item.photos.map((photo) => photo.id);
    const target = index + delta;
    if (target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    const item = await run(() => api.reorderPhotos(order));
    if (item) applyItem(item);
  }

  async function commitRename(id: string) {
    const name = draft.trim();
    renaming = null;
    if (!name) return;
    const item = await run(() => api.renamePhoto(id, name));
    if (item) applyItem(item);
  }

  async function turn(id: string, quarters: number) {
    const item = await run(() => api.rotatePhoto(id, quarters));
    if (item) applyItem(item);
  }

  async function revert(id: string) {
    const item = await run(() => api.setCrop(id, null));
    if (item) applyItem(item);
  }

  function startDrag(event: DragEvent, id: string) {
    event.dataTransfer?.setData('application/x-newsbuilder-photo', id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
  }
</script>

<section class="photos">
  <header>
    <h2>Фотографии</h2>
    <div class="tools">
      <button type="button" onclick={pick} disabled={ui.busy}>Добавить…</button>
      <button type="button" onclick={paste} disabled={ui.busy}>Вставить</button>
    </div>
  </header>

  {#if ui.item.photos.length === 0}
    <p class="empty">
      Перетащите фотографии в окно, вставьте из буфера обмена или нажмите «Добавить».
    </p>
  {/if}

  <ol>
    {#each ui.item.photos as photo, index (photo.id)}
      <li class:unused={!photo.used}>
        <div
          class="thumb"
          draggable="true"
          ondragstart={(event) => startDrag(event, photo.id)}
          role="button"
          tabindex="0"
          title="Перетащите в текст"
        >
          {#if photo.thumbUrl}
            <img src={convertFileSrc(photo.thumbUrl)} alt={photo.fileName} />
          {:else}
            <div class="missing"></div>
          {/if}
        </div>

        <div class="body">
          {#if renaming === photo.id}
            <input
              bind:value={draft}
              onblur={() => commitRename(photo.id)}
              onkeydown={(e) => {
                if (e.key === 'Enter') commitRename(photo.id);
                if (e.key === 'Escape') renaming = null;
              }}
            />
          {:else}
            <button
              class="name"
              type="button"
              onclick={() => {
                renaming = photo.id;
                draft = photo.fileName;
              }}>{photo.fileName}</button
            >
          {/if}

          <p class="meta">
            {photo.width}×{photo.height}
            <span class="badge" class:in-text={photo.used}>
              {photo.used ? 'в тексте' : 'не размещена'}
            </span>
          </p>

          <div class="row">
            <button type="button" onclick={() => oninsert(photo.id)} disabled={ui.busy}>
              В текст
            </button>
            <button type="button" onclick={() => turn(photo.id, -1)} title="Повернуть влево">
              ↺
            </button>
            <button type="button" onclick={() => turn(photo.id, 1)} title="Повернуть вправо">
              ↻
            </button>
            <button type="button" onclick={() => oncrop(photo)}>Кадр…</button>
            <button
              type="button"
              onclick={() => revert(photo.id)}
              disabled={!photo.crop}
              title="Вернуть полный кадр">⤺</button
            >
            <button type="button" class="danger" onclick={() => remove(photo.id)}>Удалить</button>
          </div>

          <div class="row order">
            <button type="button" onclick={() => moveBy(index, -1)} disabled={index === 0}>↑</button>
            <button
              type="button"
              onclick={() => moveBy(index, 1)}
              disabled={index === ui.item.photos.length - 1}>↓</button
            >
          </div>
        </div>
      </li>
    {/each}
  </ol>
</section>

<style>
  .photos {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    border-right: 1px solid var(--line);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
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
  .tools {
    display: flex;
    gap: 0.35rem;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0.5rem;
    overflow: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  li {
    display: flex;
    gap: 0.6rem;
    padding: 0.5rem;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-raised);
  }
  li.unused {
    border-style: dashed;
  }
  .thumb {
    width: 5.5rem;
    flex: none;
    cursor: grab;
  }
  .thumb img,
  .missing {
    width: 100%;
    height: 4rem;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    background: var(--surface-sunken);
  }
  .body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .name {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: text;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    margin: 0;
    font-size: 0.75rem;
    color: var(--muted);
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .badge {
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    background: var(--warn-wash);
    color: var(--warn);
  }
  .badge.in-text {
    background: var(--accent-wash);
    color: var(--accent);
  }
  .row {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
  }
  .row.order {
    margin-top: auto;
  }
  .empty {
    padding: 1rem 0.75rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .danger {
    color: var(--danger);
  }
</style>
