<script lang="ts">
  import { selection, call, log, project, refreshProject, reloadPreview, resolveReference } from "../lib/state.svelte.js";

  let swap = $state(null);
  let optimizing = $state(false);
  let optimizedName = $state("");

  const media = $derived(selection.media);
  const hasTarget = $derived(
    selection.page && selection.elementIndex !== null && media && (media.kind === "image" || media.kind === "video" || media.kind === "source" || media.kind === "css-background")
  );

  function pageFolder() {
    // selection.page is the served path, e.g. "/about/" or "/".
    const cleaned = selection.page.replace(/^\//, "").replace(/\/$/, "");
    return cleaned === "" || cleaned.endsWith(".html") ? parentDirOf(cleaned) : cleaned;
  }

  function parentDirOf(path) {
    const index = path.lastIndexOf("/");
    return index === -1 ? "" : path.slice(0, index);
  }

  function attributeFor() {
    if (!media) return "src";
    return "src";
  }

  async function replaceWithFile() {
    const sourcePath = await call("pick_file");
    if (!sourcePath) return;
    const originalName = String(sourcePath).split(/[\\/]/).pop();
    const imported = await call("import_media", { sourcePath, originalName });
    log("ok", `imported as /image/${imported.fileName}${imported.renamedFrom ? ` (renamed from ${imported.renamedFrom})` : ""}`);
    optimizedName = imported.dest_rel;
    await buildSwapPreview(imported.dest_rel);
  }

  async function optimizeImported() {
    if (!optimizedName) return;
    optimizing = true;
    try {
      const absolute = `${project.root}/${optimizedName}`;
      const isPng = optimizedName.toLowerCase().endsWith(".png");
      const isJpg = /\.(jpe?g)$/i.test(optimizedName);
      if (!isPng && !isJpg) {
        log("error", "lossless optimization applies to PNG (oxipng) and JPEG files");
        return;
      }
      const dest = optimizedName.replace(/\.(png|jpe?g)$/i, (m) => `-opt${m.toLowerCase()}`);
      const result = await call(isPng ? "tool_png_optimize" : "tool_jpeg_optimize", {
        sourcePath: absolute,
        destPath: `${project.root}/${dest}`,
        quality: 92,
      });
      log("ok", `${optimizedName}: ${(result.inputBytes / 1024).toFixed(1)} KB -> ${(result.outputBytes / 1024).toFixed(1)} KB (lossless: ${result.lossless})`);
      optimizedName = dest;
      await buildSwapPreview(dest);
    } finally {
      optimizing = false;
    }
  }

  async function convertToWebp() {
    if (!optimizedName) return;
    optimizing = true;
    try {
      const absolute = `${project.root}/${optimizedName}`;
      const dest = optimizedName.replace(/\.(png|jpe?g|bmp|gif)$/i, ".webp");
      const result = await call("tool_image_convert", {
        sourcePath: absolute,
        destPath: `${project.root}/${dest}`,
        format: "webp-lossless",
        jpegQuality: 92,
      });
      log("ok", `WebP (lossless): ${(result.inputBytes / 1024).toFixed(1)} KB -> ${(result.outputBytes / 1024).toFixed(1)} KB`);
      optimizedName = dest;
      await buildSwapPreview(dest);
    } finally {
      optimizing = false;
    }
  }

  async function buildSwapPreview(newAssetRel) {
    if (!hasTarget) return;
    const dir = pageFolder();
    const oldRel = resolveReference(dir, media.src || "");
    swap = await call("propose_media_swap", {
      pagePath: selection.page.replace(/^\//, ""),
      pageDir: dir,
      elementIndex: selection.elementIndex,
      attribute: attributeFor(),
      newAssetRel: newAssetRel,
      oldAssetRel: oldRel,
      srcset: media.srcset || null,
    });
    if (swap.files.length === 0) {
      log("error", "no source references needed changing - is the selected media referenced from a file in this project?");
    }
  }

  async function applySwap() {
    if (!swap || swap.files.length === 0) return;
    const payload = swap.files.map((file) => ({ path: file.path, newContent: file.newContent }));
    await call("apply_edited_files", {
      files: payload,
      description: `Replace ${media?.src || "media"} with ${optimizedName}`,
    });
    log("ok", `media swap applied to ${payload.map((f) => f.path).join(", ")} - undoable via the change history`);
    swap = null;
    await refreshProject();
    reloadPreview();
  }

  function cancelSwap() {
    swap = null;
  }
</script>

<div>
  <div class="panel-title">Image Studio</div>

  {#if !hasTarget}
    <p class="muted">
      Click an <strong>image, video or element with a background image</strong> in
      the live preview. The replacement is copied into the project's
      <code>/image/</code> directory with a clean, collision-free name, and
      every reference (HTML, srcset, CSS) is updated and shown as a diff
      before anything is written.
    </p>
    {#if selection.page}
      <p class="muted">Current selection: &lt;{selection.tag}&gt; at index {selection.elementIndex} on {selection.page || "/"}</p>
    {/if}
  {:else}
    <div class="section">
      <div class="panel-title">Selection</div>
      <dl class="kv">
        <dt>Element</dt><dd>&lt;{selection.tag}&gt; #{selection.elementIndex}</dd>
        <dt>Kind</dt><dd>{media.kind}</dd>
        <dt>Source</dt><dd>{media.src || "(none)"}</dd>
        {#if media.srcset}<dt>srcset</dt><dd>{media.srcset}</dd>{/if}
      </dl>
    </div>
    <div class="section">
      <div class="panel-title">Replacement</div>
      <div class="row">
        <button class="primary" onclick={replaceWithFile}>Choose replacement file…</button>
      </div>
      {#if optimizedName}
        <p class="muted">New asset: <code>{optimizedName}</code></p>
        <div class="row">
          <button onclick={optimizeImported} disabled={optimizing}>Lossless optimize</button>
          <button onclick={convertToWebp} disabled={optimizing}>WebP (lossless)</button>
        </div>
        <p class="muted">
          Optimization never overwrites the original (MR-04); JPEG re-encoding
          is visually identical, not bit-exact, and always reports sizes.
        </p>
      {/if}
    </div>
    {#if swap}
      <div class="section">
        <div class="panel-title">Proposed changes ({swap.files.length} file{swap.files.length === 1 ? "" : "s"})</div>
        {#each swap.files as file}
          <p class="muted">{file.path} (+{file.added} / -{file.removed})</p>
          <pre class="diff">{file.diff}</pre>
        {/each}
        <div class="row">
          <button class="primary" onclick={applySwap}>Apply {swap.files.length} file change{swap.files.length === 1 ? "" : "s"}</button>
          <button onclick={cancelSwap}>Cancel</button>
        </div>
      </div>
    {/if}
  {/if}
</div>
