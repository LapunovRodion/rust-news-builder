// T108 — the document and the body say the same thing, in both directions.
//
// The round trip is the property that matters: an editor's paragraph order and every card's
// photos and layout survive a trip through the editor and back. The second property is
// deviation D-9's: no marker text is ever formed, in either direction.

import { describe, expect, test } from 'vitest';
import { toBlocks, toDoc } from './body';
import type { BlockInput, BlockView } from './types';

const body: BlockView[] = [
  { kind: 'paragraph', text: 'Лид-абзац, который открывает материал.', lead: true },
  { kind: 'placement', photoIds: ['1'], layout: 'full-width' },
  { kind: 'paragraph', text: 'Второй абзац.', lead: false },
  { kind: 'placement', photoIds: ['2', '3'], layout: 'row' },
  { kind: 'paragraph', text: 'Третий абзац.', lead: false },
  { kind: 'placement', photoIds: ['4'], layout: 'float-left' },
  { kind: 'paragraph', text: '', lead: false },
  { kind: 'placement', photoIds: ['5'], layout: 'float-right' },
];

/** `BlockView[]` reduced to what a `BlockInput[]` carries, for comparison. */
function asInputs(blocks: BlockView[]): BlockInput[] {
  return blocks.map((block) =>
    block.kind === 'paragraph'
      ? { kind: 'paragraph', text: block.text }
      : { kind: 'placement', photoIds: block.photoIds, layout: block.layout },
  );
}

describe('document ↔ body', () => {
  test('a full round trip is lossless', () => {
    expect(toBlocks(toDoc(body))).toEqual(asInputs(body));
  });

  test('a document round trip is lossless too', () => {
    const once = toDoc(body);
    const twice = toDoc(
      toBlocks(once).map((block) =>
        block.kind === 'paragraph'
          ? { kind: 'paragraph', text: block.text, lead: false }
          : { kind: 'placement', photoIds: block.photoIds, layout: block.layout },
      ),
    );
    expect(twice).toEqual(once);
  });

  test('every layout survives', () => {
    const layouts = toBlocks(toDoc(body))
      .filter((block) => block.kind === 'placement')
      .map((block) => (block.kind === 'placement' ? block.layout : ''));
    expect(layouts).toEqual(['full-width', 'row', 'float-left', 'float-right']);
  });

  test('a row keeps its photos in order', () => {
    const row = toBlocks(toDoc(body)).find(
      (block) => block.kind === 'placement' && block.photoIds.length > 1,
    );
    expect(row).toEqual({ kind: 'placement', photoIds: ['2', '3'], layout: 'row' });
  });

  test('an empty body becomes one empty paragraph the caret can sit in', () => {
    const doc = toDoc([]);
    expect(doc.content).toHaveLength(1);
    expect(doc.content[0]).toEqual({ type: 'paragraph' });
    // And it comes back as one empty paragraph rather than as nothing.
    expect(toBlocks(doc)).toEqual([{ kind: 'paragraph', text: '' }]);
  });

  test('an empty paragraph carries no zero-length text node', () => {
    // ProseMirror throws on one, so this is what stops the editor failing to open a document
    // that has a blank line in it.
    const doc = toDoc([{ kind: 'paragraph', text: '', lead: false }]);
    expect(doc.content[0].content).toBeUndefined();
  });

  test('a card left with no photos is dropped rather than sent on', () => {
    // INV-1: a placement referencing nothing is not a placement.
    const doc = toDoc([{ kind: 'placement', photoIds: [], layout: 'row' }]);
    expect(toBlocks(doc)).toEqual([]);
  });

  test('an unknown layout falls back rather than reaching the core', () => {
    const doc = {
      type: 'doc' as const,
      content: [{ type: 'placement', attrs: { photoIds: ['1'], layout: 'diagonal' } }],
    };
    expect(toBlocks(doc)).toEqual([{ kind: 'placement', photoIds: ['1'], layout: 'full-width' }]);
  });

  test('photo ids survive arriving as a JSON string', () => {
    const doc = {
      type: 'doc' as const,
      content: [{ type: 'placement', attrs: { photoIds: '["7","8"]', layout: 'row' } }],
    };
    expect(toBlocks(doc)).toEqual([{ kind: 'placement', photoIds: ['7', '8'], layout: 'row' }]);
  });

  test('no marker text is formed anywhere, in either direction', () => {
    // Deviation D-9. The editor works on structure; `[image:1]` is a thing only the importer
    // has ever seen.
    const serialised = JSON.stringify(toDoc(body)) + JSON.stringify(toBlocks(toDoc(body)));
    expect(serialised).not.toMatch(/\[image/);
    expect(serialised).not.toMatch(/\[images/);
    expect(serialised).not.toMatch(/\[image-left/);
    expect(serialised).not.toMatch(/\[image-right/);
  });

  test('paragraph text is never turned into a marker even when it looks like one', () => {
    // A document whose prose happens to contain marker-shaped text keeps it as prose: the
    // editor does not parse markers at all, only the importer does.
    const blocks = toBlocks(toDoc([{ kind: 'paragraph', text: 'Пример: [image:1]', lead: false }]));
    expect(blocks).toEqual([{ kind: 'paragraph', text: 'Пример: [image:1]' }]);
  });

  test('a node the schema does not admit is ignored rather than thrown on', () => {
    const doc = {
      type: 'doc' as const,
      content: [
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Nope' }] },
        { type: 'paragraph', content: [{ type: 'text', text: 'Kept.' }] },
      ],
    };
    expect(toBlocks(doc)).toEqual([{ kind: 'paragraph', text: 'Kept.' }]);
  });
});
