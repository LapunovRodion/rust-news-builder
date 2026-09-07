<!--
  T043, T096, T109, T110 — the editing surface.

  Two node types and nothing else: paragraph and placement (research R11). No headings, no
  lists, no bold — the news page has a title and paragraphs, and anything else would render as
  something `core::render` has no style for.

  The editor never shows marker text and never lets one be typed (FR-018b, deviation D-9).
  Photographs enter the text as cards: at the caret, or where they are dropped.
-->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Editor } from '@tiptap/core';
  import Document from '@tiptap/extension-document';
  import Paragraph from '@tiptap/extension-paragraph';
  import Text from '@tiptap/extension-text';
  import History from '@tiptap/extension-history';
  import Gapcursor from '@tiptap/extension-gapcursor';
  import Dropcursor from '@tiptap/extension-dropcursor';
  import { PlacementNode } from './placement-node';
  import { toBlocks, toDoc, type Doc } from './body';
  import { ui } from './store.svelte';
  import type { BlockInput, Layout } from './types';

  const {
    onBodyChange,
    onArrange,
  }: {
    onBodyChange: (blocks: BlockInput[]) => void;
    onArrange: () => void;
  } = $props();

  let host: HTMLDivElement;
  let editor = $state<Editor | null>(null);
  let revision = -1;

  /** Where the caret is, as an index among the document's top-level blocks. */
  export function caretBlockIndex(): number {
    if (!editor) return ui.item.blocks.length;
    // `$from` is read as a property rather than destructured: Svelte reserves the `$`
    // prefix for its own stores, and a local of that name will not compile.
    const at = editor.state.selection.$from;
    return at.depth === 0 ? at.index(0) : at.index(0) + 1;
  }

  /** Puts a placement at the caret (FR-018a). */
  export function insertAtCaret(photoIds: string[], layout: Layout) {
    editor
      ?.chain()
      .focus()
      .insertPlacement(photoIds, photoIds.length > 1 ? 'row' : layout)
      .run();
  }

  onMount(() => {
    editor = new Editor({
      element: host,
      extensions: [
        Document,
        Paragraph,
        Text,
        PlacementNode,
        Gapcursor,
        Dropcursor,
        History,
      ],
      content: toDoc(ui.item.blocks) as unknown as Record<string, unknown>,
      editorProps: {
        attributes: { class: 'prose', spellcheck: 'true' },
        // A photograph dragged from the list drops where the cursor shows it will (FR-018a).
        // Dropping onto a card is the card's business, so this ignores those.
        handleDrop: (view, event) => {
          const id = event.dataTransfer?.getData('application/x-newsbuilder-photo');
          if (!id) return false;
          if ((event.target as HTMLElement | null)?.closest('[data-placement]')) return false;

          const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
          if (!at) return false;
          event.preventDefault();
          editor
            ?.chain()
            .focus()
            .setTextSelection(at.pos)
            .insertPlacement([id], 'full-width')
            .run();
          return true;
        },
      },
      onUpdate: ({ editor: instance }) => {
        const doc = instance.getJSON() as unknown as Doc;
        onBodyChange(toBlocks(doc));
      },
    });
    revision = ui.documentRevision;
  });

  // A structural change made on the Rust side — an arrangement, a removed photo, an inserted
  // card — comes back as a fresh body, and the document is rebuilt from it. Typing does not
  // bump the revision, so the caret is never moved out from under the editor.
  $effect(() => {
    const wanted = ui.documentRevision;
    if (!editor || wanted === revision) return;
    revision = wanted;
    editor.commands.setContent(toDoc(ui.item.blocks) as unknown as Record<string, unknown>, false);
  });

  onDestroy(() => editor?.destroy());
</script>

<section class="editor">
  <header>
    <h2>Текст</h2>
    <button type="button" onclick={onArrange} disabled={ui.busy || ui.item.photos.length === 0}>
      Разместить автоматически
    </button>
  </header>
  <div class="surface" bind:this={host}></div>
</section>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
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
  .surface {
    flex: 1;
    overflow: auto;
    padding: 1rem 1.25rem;
  }
  .surface :global(.prose) {
    outline: none;
    max-width: 40rem;
    line-height: 1.6;
  }
  .surface :global(.prose p) {
    margin: 0 0 0.9rem;
  }
  .surface :global(.ProseMirror-gapcursor:after) {
    border-top-color: var(--accent);
  }
</style>
