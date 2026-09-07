// The placement node: the second and last node type in the editor's schema (research R11).
//
// It is an atom — a single indivisible thing the caret moves past rather than into. That is
// what makes a photograph behave like an object in the text instead of like a character
// sequence, and it is why no marker text has to exist for the editor to work (deviation D-9).

import { Node, mergeAttributes } from '@tiptap/core';
import { SvelteNodeViewRenderer } from 'svelte-tiptap';
import PlacementCard from './PlacementCard.svelte';
import type { Layout } from './types';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    placement: {
      /** Puts a placement where the caret is. */
      insertPlacement: (photoIds: string[], layout: Layout) => ReturnType;
    };
  }
}

export const PlacementNode = Node.create({
  name: 'placement',
  group: 'block',
  atom: true,
  draggable: true,
  selectable: true,

  addAttributes() {
    return {
      photoIds: {
        default: [] as string[],
        // Stored on the DOM node as JSON so a drag inside ProseMirror, which serialises
        // through HTML, does not flatten the array into "1,2".
        parseHTML: (element) => {
          const raw = element.getAttribute('data-photo-ids');
          if (!raw) return [];
          try {
            const parsed: unknown = JSON.parse(raw);
            return Array.isArray(parsed) ? parsed.map(String) : [];
          } catch {
            return [];
          }
        },
        renderHTML: (attributes) => ({
          'data-photo-ids': JSON.stringify(attributes.photoIds ?? []),
        }),
      },
      layout: {
        default: 'full-width' as Layout,
        parseHTML: (element) => element.getAttribute('data-layout') ?? 'full-width',
        renderHTML: (attributes) => ({ 'data-layout': attributes.layout }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-placement]' }];
  },

  renderHTML({ HTMLAttributes }) {
    // This is the *editor's* DOM, not the exported fragment. The fragment is rendered by
    // `core::render` and shown in the preview iframe; nothing here contributes to it.
    return ['div', mergeAttributes(HTMLAttributes, { 'data-placement': '' })];
  },

  addNodeView() {
    return SvelteNodeViewRenderer(PlacementCard);
  },

  addCommands() {
    return {
      insertPlacement:
        (photoIds: string[], layout: Layout) =>
        ({ commands }) =>
          commands.insertContent({
            type: this.name,
            attrs: { photoIds, layout },
          }),
    };
  },
});
