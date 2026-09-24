// The shapes that cross the IPC boundary (contracts/desktop-commands.md).
//
// These mirror the `serde` types in `crates/desktop/src/state.rs`. They carry *structure* —
// never HTML, and never marker text: the editor works on placement cards and the reader never
// sees a marker (deviation D-9).

export type Layout = 'full-width' | 'row' | 'float-left' | 'float-right';

export const LAYOUTS: { value: Layout; label: string }[] = [
  { value: 'full-width', label: 'Во всю ширину' },
  { value: 'row', label: 'В ряд' },
  { value: 'float-left', label: 'Слева, текст обтекает' },
  { value: 'float-right', label: 'Справа, текст обтекает' },
];

export type CropRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type BlockView =
  | { kind: 'paragraph'; text: string; lead: boolean }
  | { kind: 'placement'; photoIds: string[]; layout: Layout };

/** A body block on the way back to Rust. */
export type BlockInput =
  | { kind: 'paragraph'; text: string }
  | { kind: 'placement'; photoIds: string[]; layout: Layout };

export type PhotoView = {
  id: string;
  fileName: string;
  width: number;
  height: number;
  /** Whether any placement references it (FR-010). */
  used: boolean;
  /** A path the asset protocol serves; empty when no thumbnail could be rendered. */
  thumbUrl: string;
  crop: CropRect | null;
  /** Quarter-turns clockwise, 0–3. */
  rotation: number;
};

export type ItemView = {
  title: string;
  blocks: BlockView[];
  photos: PhotoView[];
  slug: string;
  dirty: boolean;
};

export type Warning = { kind: string; detail: string };

export type ImportResult = {
  item: ItemView;
  /** False when the document carried no headline. The interface asks; it never invents one. */
  titleDetected: boolean;
  warnings: Warning[];
};

export type AddResult = { item: ItemView; warnings: Warning[] };

export type PreviewResult = { fragment: string; warnings: Warning[] };

export type ArrangeResult = {
  item: ItemView;
  placed: number;
  replacedManual: number;
  /** What a second call with `replaceManual: true` would discard (FR-020). */
  preservedManual: number;
};

export type ServerView = {
  name: string;
  host: string;
  user: string;
  port: number;
  remoteBasePath: string;
  publicBaseUrl: string;
  auth: 'password' | 'key';
  keyPath: string | null;
  hasCredential: boolean;
  /** The Joomla site articles go into; `null` publishes photos only (002 FR-013). */
  site: SiteTargetView | null;
};

/** Article settings. `null` means "not set here": the server default, then Joomla's, applies. */
export type ArticleSettingsView = {
  category: number | null;
  state: 'published' | 'unpublished' | null;
  featured: boolean | null;
  access: number | null;
  language: string | null;
  author: number | null;
  authorAlias: string | null;
  /** RFC 3339. */
  publishUp: string | null;
  publishDown: string | null;
  metaDescription: string | null;
  /** `null`: not set here. `[]`: no tags. */
  tags: number[] | null;
};

export type SiteTargetView = {
  joomlaRoot: string;
  siteUrl: string;
  php: string;
  defaults: ArticleSettingsView;
};

export type SiteCatalogView = {
  joomlaVersion: string;
  categories: { id: number; title: string; level: number; published: boolean }[];
  accessLevels: { id: number; title: string }[];
  languages: { code: string; title: string }[];
  authors: { id: number; title: string }[];
  tags: { id: number; title: string; level: number }[];
};

export type ConfirmationReason = 'gone' | 'trashed' | 'edited_on_site';
export type ArticleConfirmation = 'none' | 'overwrite' | 'create_new';

export type ArticleOutcomeView =
  | { outcome: 'created' | 'updated' | 'unchanged'; id: number; url: string }
  | { outcome: 'would_create' | 'would_update'; id: number | null; settings: ArticleSettingsView }
  | { outcome: 'needs_confirmation'; reason: ConfirmationReason; id: number | null }
  | { outcome: 'failed'; step: string; detail: string };

export function emptyArticleSettings(): ArticleSettingsView {
  return {
    category: null,
    state: null,
    featured: null,
    access: null,
    language: null,
    author: null,
    authorAlias: null,
    publishUp: null,
    publishDown: null,
    metaDescription: null,
    tags: null,
  };
}

export type PublishedFile = { name: string; url: string };

export type PublicationView = {
  fragment: string;
  remoteFolder: string;
  slug: string;
  uploaded: PublishedFile[];
  unchanged: string[];
  dryRun: boolean;
  warnings: Warning[];
  article: ArticleOutcomeView | null;
};

export type PublishProgress = { file: string; index: number; total: number };

/** A failed command. `detail` already names the offending file, marker or photo (SC-007). */
export type CommandError = { kind: string; detail: string };

/** Whether an unknown value is one of ours, so the interface can render `detail` as it came. */
export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as CommandError).kind === 'string' &&
    typeof (value as CommandError).detail === 'string'
  );
}

/** The text to show for any thrown value. */
export function messageOf(error: unknown): string {
  if (isCommandError(error)) return error.detail;
  if (error instanceof Error) return error.message;
  return String(error);
}
