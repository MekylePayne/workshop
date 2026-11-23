//! HostSmith Workshop interface shell state (Svelte 5 runes).
//!
//! One module owns all shared state so panels stay small: the open
//! project, the preview URL, the current selection from the bridge, panel
//! routing, the diagnostics log and the change-history entries.

import { invoke } from "@tauri-apps/api/core";

// ------------------------------------------------------------- project

export const project = $state({
  root: "",
  scan: null,
  previewUrl: "",
});

export const selection = $state({
  page: "",
  elementIndex: null,
  tag: "",
  text: "",
  textOrdinal: null,
  media: null,
  file: "",
});

// ------------------------------------------------------------- panels

export const panels = [
  "welcome", "image-studio", "patchwork", "favicon", "seo",
  "image-workshop", "toolbox", "validator", "cloudflare", "settings",
];

export const ui = $state({
  activePanel: "welcome",
  log: [],
  journal: [],
  busy: false,
  bottomTab: "log",
});

export function openPanel(name) {
  if (!panels.includes(name)) return;
  ui.activePanel = name;
}

export function log(kind, message) {
  const stamp = new Date().toLocaleTimeString();
  ui.log.push({ kind, stamp, message });
  if (ui.log.length > 500) ui.log.splice(0, ui.log.length - 500);
}

export function clearLog() {
  ui.log.length = 0;
}

// ------------------------------------------------------------- wrappers

export async function call(command, args = {}) {
  ui.busy = true;
  try {
    const result = await invoke(command, args);
    return result;
  } catch (error) {
    log("error", typeof error === "string" ? error : JSON.stringify(error));
    throw error;
  } finally {
    ui.busy = false;
  }
}

export async function refreshProject() {
  if (!project.root) return;
  try {
    const info = await call("rescan_project");
    project.scan = info.scan;
    await refreshJournal();
  } catch {
    /* logged by call() */
  }
}

export async function refreshJournal() {
  try {
    ui.journal = await invoke("journal_entries");
  } catch {
    ui.journal = [];
  }
}

export async function openProjectFolder(path) {
  const info = await call("open_project", { path });
  project.root = info.root;
  project.scan = info.scan;
  log("info", `project opened: ${info.root} (${info.scan.totalFiles} files, ${info.scan.pages.length} pages)`);
  await refreshJournal();
  const url = await call("start_preview");
  project.previewUrl = url;
  log("info", `preview running at ${url}`);
  openPanel("image-studio");
  return info;
}

export async function closeProject() {
  await call("stop_preview");
  await call("close_project");
  project.root = "";
  project.scan = null;
  project.previewUrl = "";
  selection.elementIndex = null;
  log("info", "project closed");
}

export function pageDir(folder) {
  return folder === "." ? "" : folder;
}

export function pageHref(page) {
  const folder = page.folder === "." ? "" : `${page.folder}/`;
  return `${project.previewUrl}/${folder}`;
}

export function reloadPreview() {
  window.dispatchEvent(new CustomEvent("hsw-preview-reload"));
}

/// Mirrors hsw_core::fsutil::resolve_reference for building the old asset's
/// project-relative path from a page's src attribute.
export function resolveReference(baseDir, reference) {
  const ref = reference.trim();
  if (/^(data:|https?:\/\/|\/\/|#|mailto:|tel:)/i.test(ref)) return ref;
  const base = ref.startsWith("/") ? "" : baseDir;
  const parts = `${base}/${ref}`.split("/");
  const stack = [];
  for (const part of parts) {
    if (!part || part === ".") continue;
    if (part === "..") stack.pop();
    else stack.push(part);
  }
  return stack.join("/");
}
