<script lang="ts">
  import { selection, project, call, log, refreshProject, reloadPreview, refreshJournal } from "../lib/state.svelte.js";

  let path = $state("");
  let content = $state("");
  let original = $state("");
  let binary = $state(false);

  async function load(rel) {
    const target = rel || selection.file;
    if (!target) return;
    path = target;
    try {
      content = await call("read_text_file", { rel: target });
      original = content;
      binary = false;
    } catch (error) {
      binary = true;
      content = "";
      original = "";
      log("error", String(error));
    }
  }

  async function save() {
    if (content === original) {
      log("info", "file unchanged - nothing to save");
      return;
    }
    await call("apply_edited_files", {
      files: [{ path, newContent: content }],
      description: `Edit source of ${path}`,
    });
    log("ok", `saved ${path} - backup written and change journalled`);
    original = content;
    await refreshProject();
    reloadPreview();
    await refreshJournal();
  }

  $effect(() => {
    if (selection.file && selection.file !== path) {
      if (/\.(html?|css|js|mjs|json|xml|txt|md|webmanifest)$/i.test(selection.file)) {
        load(selection.file);
      }
    }
  });
</script>

<div>
  <div class="panel-title">Source editor</div>
  <div class="row">
    <input class="grow" placeholder="project-relative path, e.g. about/index.html" bind:value={path} />
    <button onclick={() => load(path)}>Load</button>
  </div>
  {#if binary}
    <p class="error-text">This file is binary (an image, video or archive) - edit it with the Image Workshop or Image Studio instead.</p>
  {:else if content || path}
    <div class="field">
      <textarea rows="16" bind:value={content} aria-label="Source file content"></textarea>
    </div>
    <div class="row">
      <button class="primary" onclick={save} disabled={content === original}>Save (backed up, undoable)</button>
      <button onclick={() => load(path)} disabled={content === original}>Revert editor</button>
    </div>
    <p class="muted">
      Direct source editing keeps the site untouched until you save; the save
      writes a backup first and the change history can revert it.
    </p>
  {:else}
    <p class="muted">Load a file from the project browser (double-click) or type a path above.</p>
  {/if}
</div>
