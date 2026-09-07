<!--
  T107, T111 — a placement, as it appears inside the text.

  This is what replaced marker typing (deviation D-9). The editor sees the photographs, the
  layout they are in, and the two controls that change either — not `[images:2,3]`.

  It is a TipTap node view, so it lives inside the document and moves with it: dragging the card
  is dragging the placement (FR-018d), and dropping a photograph onto it builds a row (FR-018c).
-->
<script lang="ts">
  import { NodeViewWrapper } from 'svelte-tiptap';
  import type { NodeViewProps } from '@tiptap/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { LAYOUTS, type Layout } from './types';
  import { photoById, ui } from './store.svelte';

  const { node, updateAttributes, deleteNode, selected }: NodeViewProps = $props();

  const photoIds = $derived((node.attrs.photoIds ?? []) as string[]);
  const layout = $derived((node.attrs.layout ?? 'full-width') as Layout);
  const photos = $derived(photoIds.map((id) => photoById(id)).filter((p) => p !== undefined));

  // A row is the only layout that admits more than one photograph (INV-4), so the control
  // offers the rest only while the card holds exactly one. The core refuses the others anyway.
  const available = $derived(photoIds.length > 1 ? LAYOUTS.filter((l) => l.value === 'row') : LAYOUTS);

  let dragOver = $state(false);

  function choose(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value as Layout;
    updateAttributes({ layout: value });
  }

  function drop(event: DragEvent) {
    dragOver = false;
    const id = event.dataTransfer?.getData('application/x-newsbuilder-photo');
    if (!id || photoIds.includes(id)) return;
    event.preventDefault();
    // Dropping a second photograph onto a card is what "put these side by side" means
    // (FR-018c). The layout follows, because a row is the only thing several photographs can be.
    updateAttributes({ photoIds: [...photoIds, id], layout: 'row' });
  }

  function dragEnter(event: DragEvent) {
    if (event.dataTransfer?.types.includes('application/x-newsbuilder-photo')) {
      event.preventDefault();
      dragOver = true;
    }
  }

  function removeOne(id: string) {
    const left = photoIds.filter((other) => other !== id);
    if (left.length === 0) {
      deleteNode();
      return;
    }
    // A row down to one photograph is no longer a row (INV-4).
    updateAttributes({
      photoIds: left,
      layout: left.length === 1 && layout === 'row' ? 'full-width' : layout,
    });
  }
</script>

<NodeViewWrapper>
  <div
    class="card"
    class:selected
    class:drag-over={dragOver}
    data-layout={layout}
    role="group"
    aria-label="Фотографии в тексте"
    ondragenter={dragEnter}
    ondragover={dragEnter}
    ondragleave={() => (dragOver = false)}
    ondrop={drop}
  >
    <div class="strip" data-drag-handle>
      {#each photos as photo (photo.id)}
        <figure>
          {#if photo.thumbUrl}
            <img src={convertFileSrc(photo.thumbUrl)} alt={photo.fileName} draggable="false" />
          {:else}
            <div class="missing" aria-hidden="true"></div>
          {/if}
          <button
            class="drop-one"
            type="button"
            title="Убрать эту фотографию отсюда"
            onclick={() => removeOne(photo.id)}>×</button
          >
          <figcaption>{photo.fileName}</figcaption>
        </figure>
      {/each}
      {#if photos.length === 0}
        <p class="empty">Фотографии не найдены</p>
      {/if}
    </div>

    <div class="controls" contenteditable="false">
      <select value={layout} onchange={choose} aria-label="Расположение" disabled={ui.busy}>
        {#each available as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
      <button type="button" onclick={() => deleteNode()} title="Убрать из текста">
        Убрать
      </button>
    </div>
  </div>
</NodeViewWrapper>

<style>
  .card {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.5rem;
    margin: 0.75rem 0;
    background: var(--surface-raised);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .card.selected {
    outline: 2px solid var(--accent);
  }
  .card.drag-over {
    border-color: var(--accent);
    background: var(--accent-wash);
  }
  .strip {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    cursor: grab;
  }
  figure {
    margin: 0;
    position: relative;
    width: 8rem;
  }
  img,
  .missing {
    width: 100%;
    height: 5.5rem;
    object-fit: cover;
    border-radius: 4px;
    display: block;
    background: var(--surface-sunken);
  }
  figcaption {
    font-size: 0.7rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .drop-one {
    position: absolute;
    top: 2px;
    right: 2px;
    border: none;
    border-radius: 50%;
    width: 1.25rem;
    height: 1.25rem;
    line-height: 1;
    background: rgb(0 0 0 / 0.6);
    color: #fff;
    cursor: pointer;
  }
  .controls {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .empty {
    color: var(--muted);
    font-size: 0.8rem;
    margin: 0;
  }
</style>
