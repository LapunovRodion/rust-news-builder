// The one place the interface keeps what the Rust side told it.
//
// This is a *view*, never a second copy that drifts: every command returns a fresh `ItemView`
// and the whole of it lands here (contracts/desktop-commands.md). The one thing the editor is
// briefly ahead on is the text being typed, which is why `applyItem` can be told not to
// re-render the document — replacing it under a moving caret is how an editor loses a word.

import type { ItemView, PhotoView, Warning } from './types';
import { messageOf } from './types';

/** An empty item, so the interface has something to show before anything is opened. */
const EMPTY: ItemView = { title: '', blocks: [], photos: [], slug: 'news', dirty: false };

export const ui = $state({
  item: EMPTY as ItemView,
  /** The rendered fragment, exactly as `core::build` produced it (FR-022). */
  fragment: '',
  warnings: [] as Warning[],
  /** The last failure, for the status bar. */
  error: '' as string,
  busy: false,
  /** Bumped whenever the editor document has to be rebuilt from `item.blocks`. */
  documentRevision: 0,
});

/** Takes a fresh view from a command. */
export function applyItem(item: ItemView, rerenderEditor = true) {
  ui.item = item;
  if (rerenderEditor) ui.documentRevision += 1;
}

export function setWarnings(warnings: Warning[]) {
  ui.warnings = warnings;
}

export function addWarnings(warnings: Warning[]) {
  if (warnings.length > 0) ui.warnings = [...ui.warnings, ...warnings];
}

export function clearError() {
  ui.error = '';
}

/**
 * Runs a command, showing its failure rather than swallowing it.
 *
 * `detail` is rendered as it arrived: the core has already made it name the offending file,
 * marker or photo, and rewording it here would throw that away (SC-007).
 */
export async function run<T>(work: () => Promise<T>): Promise<T | undefined> {
  ui.busy = true;
  ui.error = '';
  try {
    return await work();
  } catch (error) {
    ui.error = messageOf(error);
    return undefined;
  } finally {
    ui.busy = false;
  }
}

export function photoById(id: string): PhotoView | undefined {
  return ui.item.photos.find((photo) => photo.id === id);
}
