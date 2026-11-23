// Selection bridge + application event wiring.
//
// The preview server injects a script into served HTML that posts
// `hsw-select` messages to the parent window (clicks with the annotated
// element index, the selected text node's ordinal and media info). This
// module forwards them into the shared state and listens to the Tauri
// events emitted by the Rust shell.

import { listen } from "@tauri-apps/api/event";
import { selection, refreshProject, log } from "./state.svelte.js";

let installed = false;

export async function setupBridge() {
  if (installed) return;
  installed = true;

  window.addEventListener("message", (event) => {
    const data = event.data || {};
    if (data.type === "hsw-select") {
      selection.page = data.page || "";
      selection.elementIndex = typeof data.elementIndex === "number" ? data.elementIndex : null;
      selection.tag = data.tag || "";
      selection.text = data.text || "";
      selection.textOrdinal = typeof data.textOrdinal === "number" ? data.textOrdinal : null;
      selection.media = data.media || null;
      if (data.media) {
        log("info", `selected ${data.tag} -> ${data.media.kind}: ${data.media.src || "(no src)"}`);
      } else if (data.text) {
        log("info", `selected text in <${data.tag}>: "${data.text.slice(0, 60)}"`);
      }
    }
  });

  try {
    await listen("fs-changed", (event) => {
      const paths = event.payload || [];
      log("info", `project files changed outside the application: ${paths.join(", ")} - journal entries for them were invalidated`);
      refreshProject();
    });
    await listen("deploy-progress", (event) => {
      const payload = event.payload || {};
      log("info", `deployment ${payload.step}: ${payload.done}/${payload.total}`);
    });
  } catch (error) {
    // Running outside Tauri (plain browser dev): the preview and panels
    // still work, Tauri-only events simply do not fire.
    log("info", "running outside the Tauri shell - desktop commands unavailable");
  }
}
