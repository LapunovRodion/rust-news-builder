// Between the TipTap document and `BlockInput[]` (T108).
//
// The editor's schema has exactly two node types — paragraph and placement (research R11) —
// which is what makes this translation total rather than lossy. There is no bold, no list, no
// heading: the news page has a title and paragraphs, and anything else would render as
// something `core::render` has no style for.
//
// **Marker text is never formed here.** A placement is a node holding photo ids, and it crosses
// the boundary as `{ kind: 'placement', photoIds, layout }`. The string `[image:1]` appears
// nowhere in the editor, which is deviation D-9 made structural.

import type { BlockInput, BlockView, Layout } from './types';

/** The node names in the editor schema. Two, and only two. */
export const PARAGRAPH_NODE = 'paragraph';
export const PLACEMENT_NODE = 'placement';

/** A ProseMirror-shaped document, typed only as far as this module needs. */
export type DocNode = {
  type: string;
  attrs?: Record<string, unknown>;
  content?: DocNode[];
  text?: string;
};

export type Doc = { type: 'doc'; content: DocNode[] };

/** The document the editor should show for a body. */
export function toDoc(blocks: BlockView[]): Doc {
  const content = blocks.map((block) =>
    block.kind === 'paragraph' ? paragraphNode(block.text) : placementNode(block.photoIds, block.layout),
  );
  // ProseMirror will not hold an empty document; an empty body is one empty paragraph, which
  // is also where the caret has to go for the editor to be usable at all.
  return { type: 'doc', content: content.length > 0 ? content : [paragraphNode('')] };
}

/** The body the core should hold for a document. */
export function toBlocks(doc: Doc): BlockInput[] {
  const out: BlockInput[] = [];
  for (const node of doc.content ?? []) {
    if (node.type === PLACEMENT_NODE) {
      const photoIds = readIds(node.attrs?.photoIds);
      // A card with no photos left is dropped rather than sent on: INV-1 forbids an empty
      // placement, and the core would refuse it anyway.
      if (photoIds.length === 0) continue;
      out.push({ kind: 'placement', photoIds, layout: readLayout(node.attrs?.layout) });
      continue;
    }
    if (node.type === PARAGRAPH_NODE) {
      out.push({ kind: 'paragraph', text: textOf(node) });
    }
    // Anything else cannot occur: the schema admits no other node. Ignoring it rather than
    // throwing keeps a paste that somehow smuggled one in from destroying the document.
  }
  return out;
}

function paragraphNode(text: string): DocNode {
  // An empty paragraph has no text node at all — a zero-length text node is invalid in
  // ProseMirror and throws when the document is created.
  return text.length > 0
    ? { type: PARAGRAPH_NODE, content: [{ type: 'text', text }] }
    : { type: PARAGRAPH_NODE };
}

function placementNode(photoIds: string[], layout: Layout): DocNode {
  return { type: PLACEMENT_NODE, attrs: { photoIds: [...photoIds], layout } };
}

/** The plain text of a paragraph, with no marks, because the schema carries none. */
function textOf(node: DocNode): string {
  if (!node.content) return '';
  return node.content.map((child) => child.text ?? '').join('');
}

function readIds(value: unknown): string[] {
  if (Array.isArray(value)) return value.map(String).filter((id) => id.length > 0);
  // The attribute survives a serialisation round trip as a JSON string in some ProseMirror
  // paths; accepting both is cheaper than debugging why a card lost its photos.
  if (typeof value === 'string' && value.length > 0) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed)) return parsed.map(String);
    } catch {
      return value.split(',').map((part) => part.trim()).filter(Boolean);
    }
  }
  return [];
}

const VALID_LAYOUTS: Layout[] = ['full-width', 'row', 'float-left', 'float-right'];

function readLayout(value: unknown): Layout {
  return VALID_LAYOUTS.includes(value as Layout) ? (value as Layout) : 'full-width';
}

/** `BlockView[]` as `BlockInput[]`, dropping the fields the core derives itself. */
export function viewsToInputs(blocks: BlockView[]): BlockInput[] {
  return blocks.map((block) =>
    block.kind === 'paragraph'
      ? { kind: 'paragraph', text: block.text }
      : { kind: 'placement', photoIds: block.photoIds, layout: block.layout },
  );
}
