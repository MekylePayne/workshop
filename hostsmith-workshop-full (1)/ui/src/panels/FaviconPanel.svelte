<script lang="ts">
  import { project, call, log, refreshProject, reloadPreview, refreshJournal } from "../lib/state.svelte.js";

  let detection = $state(null);
  let generated = $state(null);

  async function runDetection() {
    detection = await call("favicon_detect", { pagePath: "index.html", pageDir: "" });
  }

  async function generate() {
    const sourcePath = await call("pick_file");
    if (!sourcePath) return;
    generated = await call("favicon_generate", { sourcePath });
    log("ok", `favicon artifacts written: ${generated.files.join(", ")}`);
    await refreshProject();
  }

  async function applyReferences() {
    if (!generated) return;
    const info = await call("read_text_file", { rel: "index.html" });
    await call("apply_edited_files", {
      files: [{ path: "index.html", newContent: info.replace("</head>", `${generated.htmlSnippet}</head>`) }],
      description: "Add favicon references to index.html",
    });
    log("ok", "favicon references added to index.html");
    await refreshProject();
    reloadPreview();
    await refreshJournal();
  }
</script>

<div>
  <div class="panel-title">Favicon Studio</div>

  <div class="section">
    <div class="row">
      <button onclick={runDetection} disabled={!project.root}>Detect current favicons</button>
      <button class="primary" onclick={generate} disabled={!project.root}>Generate from an image…</button>
    </div>
    <p class="muted">Generation writes favicon.ico (16/32/48), per-size PNGs, an Apple touch icon and a manifest - never overwriting an existing manifest silently.</p>
  </div>

  {#if detection}
    <div class="section">
      <div class="panel-title">Detection</div>
      <dl class="kv">
        <dt>favicon.ico</dt><dd>{detection.faviconIcoFile ? "present" : "missing"}</dd>
        <dt>Icon links</dt><dd>{detection.icons.length}</dd>
        <dt>Touch icons</dt><dd>{detection.appleTouchIcons.length}</dd>
        <dt>Manifest</dt><dd>{detection.manifest || "none"}</dd>
        <dt>Missing files</dt><dd>{detection.missingFiles.join(", ") || "none"}</dd>
      </dl>
      {#each detection.issues as issue}
        <div class="finding warning"><p class="finding-note">{issue}</p></div>
      {/each}
    </div>
  {/if}

  {#if generated}
    <div class="section">
      <div class="panel-title">Generated</div>
      <p class="muted">{generated.files.join(", ")}</p>
      <p class="muted">Manifest {generated.manifestWritten ? "written" : "kept (already existed)"}.</p>
      <div class="field">
        <label for="favicon-snippet">Head references to insert</label>
        <pre id="favicon-snippet" class="tool-output">{generated.htmlSnippet}</pre>
      </div>
      <button class="primary" onclick={applyReferences}>Add references to index.html</button>
    </div>
  {/if}

  <div class="section">
    <div class="panel-title">Simulated previews</div>
    <p class="muted">
      The tab and Google-result drawings in the interface are generated
      locally from your icon files - they are simulations, not data from
      Google. Real search appearance depends on indexing and is out of any
      local tool's reach.
    </p>
  </div>
</div>
