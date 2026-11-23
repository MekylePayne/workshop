<script lang="ts">
  import { project, selection, call, log } from "../lib/state.svelte.js";

  let sourceRel = $state("");
  let info = $state(null);
  let result = $state(null);
  let working = $state(false);
  let maxWidth = $state(800);
  let maxHeight = $state(800);
  let cropX = $state(0);
  let cropY = $state(0);
  let cropW = $state(200);
  let cropH = $state(200);
  let quality = $state(92);

  function abs(rel) {
    return `${project.root}/${rel}`;
  }

  function destFor(suffix, ext) {
    const base = sourceRel.replace(/\.[^.]+$/, "");
    return `image/${base.split("/").pop()}-${suffix}.${ext}`;
  }

  async function loadInfo() {
    info = await call("tool_image_info", { sourcePath: abs(sourceRel) });
    result = null;
  }

  async function run(command, args, label) {
    working = true;
    try {
      result = await call(command, args);
      log("ok", `${label}: ${result.width}x${result.height}, ${(result.inputBytes / 1024).toFixed(1)} KB -> ${(result.outputBytes / 1024).toFixed(1)} KB (${result.lossless ? "lossless" : "not bit-exact"})`);
    } finally {
      working = false;
    }
  }

  function chooseFromSelection() {
    if (selection.file && /\.(png|jpe?g|gif|webp|avif|bmp|ico)$/i.test(selection.file)) {
      sourceRel = selection.file;
      loadInfo();
    } else {
      log("info", "select an image file in the project browser first");
    }
  }
</script>

<div>
  <div class="panel-title">Image Workshop</div>
  <p class="muted">
    Batch-ready operations that always write NEW files - originals are never
    touched (MR-04). WebP output is lossless by design (TL-07); JPEG output
    re-encodes pixels and is labelled honestly.
  </p>

  <div class="section">
    <div class="row">
      <input class="grow" placeholder="project-relative image path" bind:value={sourceRel} />
      <button onclick={chooseFromSelection}>Use browser selection</button>
      <button onclick={loadInfo}>Info</button>
    </div>
    {#if info}
      <dl class="kv">
        <dt>Dimensions</dt><dd>{info.width} x {info.height}</dd>
        <dt>Format</dt><dd>{info.format}</dd>
        <dt>Size</dt><dd>{(info.bytes / 1024).toFixed(1)} KB</dd>
      </dl>
    {/if}
  </div>

  {#if sourceRel}
    <div class="section">
      <div class="panel-title">Resize (fit within, no upscale)</div>
      <div class="row">
        <input type="number" bind:value={maxWidth} min="1" style="width:80px" /> x
        <input type="number" bind:value={maxHeight} min="1" style="width:80px" />
        <button disabled={working} onclick={() => run("tool_image_resize", { sourcePath: abs(sourceRel), destPath: abs(destFor("resized", "png")), maxWidth, maxHeight, format: "png" }, "resize")}>Resize to PNG</button>
      </div>
    </div>
    <div class="section">
      <div class="panel-title">Crop</div>
      <div class="row">
        x <input type="number" bind:value={cropX} style="width:70px" />
        y <input type="number" bind:value={cropY} style="width:70px" />
        w <input type="number" bind:value={cropW} style="width:70px" />
        h <input type="number" bind:value={cropH} style="width:70px" />
        <button disabled={working} onclick={() => run("tool_image_crop", { sourcePath: abs(sourceRel), destPath: abs(destFor("crop", "png")), x: cropX, y: cropY, width: cropW, height: cropH, format: "png" }, "crop")}>Crop</button>
      </div>
    </div>
    <div class="section">
      <div class="panel-title">Rotate and convert</div>
      <div class="row">
        <button disabled={working} onclick={() => run("tool_image_rotate", { sourcePath: abs(sourceRel), destPath: abs(destFor("rot90", "png")), quarterTurns: 1, format: "png" }, "rotate 90")}>Rotate 90</button>
        <button disabled={working} onclick={() => run("tool_image_rotate", { sourcePath: abs(sourceRel), destPath: abs(destFor("rot180", "png")), quarterTurns: 2, format: "png" }, "rotate 180")}>180</button>
        <button disabled={working} onclick={() => run("tool_image_rotate", { sourcePath: abs(sourceRel), destPath: abs(destFor("rot270", "png")), quarterTurns: 3, format: "png" }, "rotate 270")}>270</button>
      </div>
      <div class="row">
        <button disabled={working} onclick={() => run("tool_image_convert", { sourcePath: abs(sourceRel), destPath: abs(destFor("webp", "webp")), format: "webp-lossless", jpegQuality: quality }, "WebP lossless")}>WebP (lossless)</button>
        <button disabled={working} onclick={() => run("tool_image_convert", { sourcePath: abs(sourceRel), destPath: abs(destFor("jpg", "jpg")), format: "jpeg", jpegQuality: quality }, `JPEG q${quality}`)}>JPEG</button>
        <label class="inline-label">JPEG quality <input type="number" min="50" max="100" bind:value={quality} style="width:60px" /></label>
      </div>
      <div class="row">
        <button disabled={working} onclick={() => run("tool_png_optimize", { sourcePath: abs(sourceRel), destPath: abs(destFor("opt", "png")) }, "PNG optimize (oxipng)")}>Optimize PNG</button>
        <button disabled={working} onclick={() => run("tool_jpeg_optimize", { sourcePath: abs(sourceRel), destPath: abs(destFor("opt", "jpg")), quality }, "JPEG optimize")}>Optimize JPEG</button>
      </div>
    </div>
  {/if}

  {#if result}
    <div class="section">
      <div class="panel-title">Last result</div>
      <dl class="kv">
        <dt>Output</dt><dd>{result.outputPath.replace(project.root + "/", "")}</dd>
        <dt>Dimensions</dt><dd>{result.width} x {result.height}</dd>
        <dt>Sizes</dt><dd>{(result.inputBytes / 1024).toFixed(1)} KB -> {(result.outputBytes / 1024).toFixed(1)} KB</dd>
        <dt>Pixels</dt><dd>{result.lossless ? "bit-identical to input" : "re-encoded (visually identical, not bit-exact)"}</dd>
      </dl>
    </div>
  {/if}
</div>
