// Every call into the Rust core, in one place.
//
// Nothing in this file decides anything. It names the commands and types their arguments, so
// that a component asking for a behaviour that does not exist as a command fails to compile —
// which is the practical form of constitution principle II.

import { invoke } from '@tauri-apps/api/core';
import type {
  AddResult,
  ArrangeResult,
  BlockInput,
  CropRect,
  ImportResult,
  ItemView,
  Layout,
  PreviewResult,
  PublicationView,
  ServerView,
} from './types';

// --- Document and item -----------------------------------------------------------------

export const openDocument = (path: string) => invoke<ImportResult>('open_document', { path });
export const newItem = () => invoke<ItemView>('new_item');
export const setTitle = (title: string) => invoke<ItemView>('set_title', { title });
export const setBody = (blocks: BlockInput[]) => invoke<ItemView>('set_body', { blocks });
export const setSlug = (slug: string) => invoke<ItemView>('set_slug', { slug });

// --- Preview and export ----------------------------------------------------------------

export const buildPreview = () => invoke<PreviewResult>('build_preview');
export const exportFragment = (path: string) => invoke<void>('export_fragment', { path });
export const copyFragment = () => invoke<string>('copy_fragment');
export const isDirty = () => invoke<boolean>('is_dirty');

// --- Photos ----------------------------------------------------------------------------

export const addPhotosFromPaths = (paths: string[]) =>
  invoke<AddResult>('add_photos_from_paths', { paths });
export const addPhotoFromClipboard = (bytes: number[]) =>
  invoke<AddResult>('add_photo_from_clipboard', { bytes });
export const removePhoto = (id: string) => invoke<AddResult>('remove_photo', { id });
export const reorderPhotos = (order: string[]) => invoke<ItemView>('reorder_photos', { order });
export const renamePhoto = (id: string, name: string) =>
  invoke<ItemView>('rename_photo', { id, name });
export const setCrop = (id: string, crop: CropRect | null) =>
  invoke<ItemView>('set_crop', { id, crop });
export const rotatePhoto = (id: string, quartersTurned: number) =>
  invoke<ItemView>('rotate_photo', { id, quartersTurned });
export const suggestCrop = (id: string, aspectWidth: number, aspectHeight: number) =>
  invoke<CropRect | null>('suggest_crop_for', { id, aspectWidth, aspectHeight });

// --- Placements ------------------------------------------------------------------------

export const insertPlacement = (at: number, photoIds: string[], layout: Layout) =>
  invoke<ItemView>('insert_placement', { at, photoIds, layout });
export const movePlacement = (from: number, to: number) =>
  invoke<ItemView>('move_placement', { from, to });
export const removePlacement = (at: number) => invoke<ItemView>('remove_placement', { at });
export const addToPlacement = (at: number, photoId: string) =>
  invoke<ItemView>('add_to_placement', { at, photoId });
export const setLayout = (at: number, layout: Layout) =>
  invoke<ItemView>('set_layout', { at, layout });

// --- Arrangement -----------------------------------------------------------------------

export const arrangeAuto = (replaceManual: boolean) =>
  invoke<ArrangeResult>('arrange_auto', { replaceManual });
export const setOnePlacement = (at: number, photoIds: string[], layout: Layout) =>
  invoke<ItemView>('set_one_placement', { at, placement: { photoIds, layout } });

// --- Servers and publishing --------------------------------------------------------------

export const listServers = () => invoke<ServerView[]>('list_servers');
export const saveServer = (config: ServerView) => invoke<void>('save_server', { config });
export const deleteServer = (name: string) => invoke<void>('delete_server', { name });
export const setCredential = (name: string, secret: string) =>
  invoke<void>('set_credential', { name, secret });
export const deleteCredential = (name: string) => invoke<void>('delete_credential', { name });
export const secretStoreAvailable = () => invoke<boolean>('secret_store_available');
export const setSessionCredential = (name: string, secret: string) =>
  invoke<void>('set_session_credential', { name, secret });
export const publish = (server: string, dryRun: boolean) =>
  invoke<PublicationView>('publish', { server, dryRun });
