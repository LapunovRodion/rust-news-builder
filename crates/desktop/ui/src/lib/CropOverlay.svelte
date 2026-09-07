<!--
  T077 — the crop frame (FR-012 – FR-014).

  A movable, resizable rectangle over the photograph, with the result visible as it moves. It
  works in *source pixels*: what the frame says is exactly what `core::photo::set_crop` records,
  so there is no rounding step between what the editor saw and what is published.

  Nothing here writes to the source file. A crop is an adjustment on the item; the photograph on
  disk is never opened again after it was read in (FR-015).
-->
<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import type { CropRect, PhotoView } from './types';

  const {
    photo,
    suggest,
    onconfirm,
    onrevert,
    oncancel,
  }: {
    photo: PhotoView;
    suggest: (width: number, height: number) => Promise<CropRect | null>;
    onconfirm: (crop: CropRect) => void;
    onrevert: () => void;
    oncancel: () => void;
  } = $props();

  /** The photograph's own pixel space, before any crop — what a `CropRect` is measured in. */
  const source = $derived(sourceSize(photo));

  // Seeded empty and filled by the effect below, so the frame follows the photograph it is
  // shown for rather than capturing whichever one it opened with.
  let frame = $state<CropRect>({ x: 0, y: 0, width: 1, height: 1 });
  let box = $state<HTMLDivElement | null>(null);
  let drag: { mode: 'move' | 'resize'; x: number; y: number; start: CropRect } | null = null;

  $effect(() => {
    frame = photo.crop ?? { x: 0, y: 0, width: source.width, height: source.height };
  });

  function sourceSize(view: PhotoView): { width: number; height: number } {
    // `PhotoView` reports the *effective* size, which a quarter-turn has already transposed.
    // The crop is stored in the untransposed frame, so turn it back.
    const swapped = view.rotation === 1 || view.rotation === 3;
    return swapped
      ? { width: view.height, height: view.width }
      : { width: view.width, height: view.height };
  }

  /** Source pixels per displayed pixel. */
  function scale(): number {
    if (!box) return 1;
    return source.width / box.clientWidth;
  }

  function begin(mode: 'move' | 'resize', event: PointerEvent) {
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { mode, x: event.clientX, y: event.clientY, start: { ...frame } };
  }

  function move(event: PointerEvent) {
    if (!drag) return;
    const k = scale();
    const dx = Math.round((event.clientX - drag.x) * k);
    const dy = Math.round((event.clientY - drag.y) * k);

    if (drag.mode === 'move') {
      frame = {
        ...frame,
        x: clamp(drag.start.x + dx, 0, source.width - frame.width),
        y: clamp(drag.start.y + dy, 0, source.height - frame.height),
      };
      return;
    }
    // Resizing keeps the top-left corner and never lets the frame leave the image (INV-6).
    frame = {
      ...frame,
      width: clamp(drag.start.width + dx, 16, source.width - frame.x),
      height: clamp(drag.start.height + dy, 16, source.height - frame.y),
    };
  }

  function end(event: PointerEvent) {
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    drag = null;
  }

  function clamp(value: number, low: number, high: number): number {
    return Math.max(low, Math.min(high, value));
  }

  async function useSuggestion(width: number, height: number) {
    const suggested = await suggest(width, height);
    // `null` means the photograph already has that shape and needs no crop at all.
    frame = suggested ?? { x: 0, y: 0, width: source.width, height: source.height };
  }

  const percent = $derived({
    left: `${(frame.x / source.width) * 100}%`,
    top: `${(frame.y / source.height) * 100}%`,
    width: `${(frame.width / source.width) * 100}%`,
    height: `${(frame.height / source.height) * 100}%`,
  });
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Кадрирование">
  <div class="panel">
    <h2>{photo.fileName}</h2>

    <div class="stage" bind:this={box}>
      {#if photo.thumbUrl}
        <img src={convertFileSrc(photo.thumbUrl)} alt="" draggable="false" />
      {/if}
      <div
        class="frame"
        style:left={percent.left}
        style:top={percent.top}
        style:width={percent.width}
        style:height={percent.height}
        onpointerdown={(e) => begin('move', e)}
        onpointermove={move}
        onpointerup={end}
        role="application"
        aria-label="Область кадра"
        tabindex="-1"
      >
        <button
          class="handle"
          type="button"
          aria-label="Изменить размер"
          onpointerdown={(e) => begin('resize', e)}
          onpointermove={move}
          onpointerup={end}
        ></button>
      </div>
    </div>

    <p class="readout">
      {frame.width} × {frame.height} пикселей, от ({frame.x}, {frame.y})
    </p>

    <div class="shapes">
      <span>Подсказать кадр:</span>
      <button type="button" onclick={() => useSuggestion(3, 2)}>3:2</button>
      <button type="button" onclick={() => useSuggestion(16, 9)}>16:9</button>
      <button type="button" onclick={() => useSuggestion(1, 1)}>1:1</button>
    </div>

    <div class="actions">
      <button type="button" onclick={oncancel}>Отмена</button>
      <button type="button" onclick={onrevert}>Вернуть полный кадр</button>
      <button type="button" class="primary" onclick={() => onconfirm(frame)}>Применить</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.55);
    display: grid;
    place-items: center;
    z-index: 20;
  }
  .panel {
    background: var(--surface);
    border-radius: 10px;
    padding: 1rem;
    width: min(46rem, 92vw);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  h2 {
    margin: 0;
    font-size: 1rem;
  }
  .stage {
    position: relative;
    background: var(--surface-sunken);
    line-height: 0;
  }
  .stage img {
    width: 100%;
    height: auto;
    display: block;
    user-select: none;
  }
  .frame {
    position: absolute;
    border: 2px solid var(--accent);
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.45);
    cursor: move;
    touch-action: none;
  }
  .handle {
    position: absolute;
    right: -8px;
    bottom: -8px;
    width: 16px;
    height: 16px;
    border: 2px solid var(--accent);
    background: var(--surface);
    border-radius: 3px;
    cursor: nwse-resize;
    padding: 0;
    touch-action: none;
  }
  .readout {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .shapes,
  .actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .shapes span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .actions {
    justify-content: flex-end;
  }
</style>
