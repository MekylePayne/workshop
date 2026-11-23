<script lang="ts">
  import { project, reloadPreview } from "../lib/state.svelte.js";
  import { onMount } from "svelte";

  const PRESETS = [
    { id: "desktop", label: "Desktop 1440", width: 1440, height: 900, dpr: 1, note: "" },
    { id: "laptop", label: "Laptop 1280", width: 1280, height: 800, dpr: 1, note: "" },
    { id: "tablet", label: "Tablet 768", width: 768, height: 1024, dpr: 2, note: "" },
    { id: "mobile", label: "Mobile 360", width: 360, height: 800, dpr: 3, note: "" },
    { id: "a13", label: "Galaxy A13 (est.)", width: 360, height: 800, dpr: 3, note: "Estimated CSS viewport (PR-05) - physical-device verification outstanding" },
  ];

  let preset = $state(PRESETS[0]);
  let customWidth = $state(800);
  let zoomPercent = $state(100);
  let rotated = $state(false);
  let frame = $state(null);

  const zoom = $derived(zoomPercent / 100);
  const viewportWidth = $derived(preset.id === "custom" ? Math.max(200, Math.min(2400, customWidth)) : preset.width);

  let currentHref = $state("");

  function navigate(href) {
    currentHref = href;
    if (frame) frame.src = href;
  }

  $effect(() => {
    if (project.previewUrl && !currentHref) {
      navigate(`${project.previewUrl}/`);
    }
  });

  function jumpToPage(event) {
    const folder = event.target.value;
    if (!folder) return;
    navigate(`${project.previewUrl}/${folder === "." ? "" : `${folder}/`}`);
  }

  onMount(() => {
    const handler = () => {
      if (frame && currentHref) {
        frame.src = currentHref;
      }
    };
    window.addEventListener("hsw-preview-reload", handler);
    return () => window.removeEventListener("hsw-preview-reload", handler);
  });
</script>

<div class="preview-pane">
  <div class="toolbar">
    {#each PRESETS as p}
      <button class="preset" class:active={preset.id === p.id} title={p.note || p.label} onclick={() => (preset = p)}>
        {p.label}
      </button>
    {/each}
    <span class="sep"></span>
    <label class="inline-label">custom
      <input type="number" min="200" max="2400" step="10" bind:value={customWidth} onchange={() => (preset = { id: "custom", width: customWidth, height: 900 })} />
    </label>
    <span class="sep"></span>
    <label class="inline-label">zoom
      <input type="range" min="30" max="150" bind:value={zoomPercent} />
      <span class="zoom-value">{zoomPercent}%</span>
    </label>
    <button onclick={() => (rotated = !rotated)} title="Rotate the viewport">{rotated ? "⟳ landscape" : "⟳ portrait"}</button>
    <span class="spacer"></span>
    {#if project.scan && project.scan.pages.length}
      <select onchange={jumpToPage} aria-label="Page navigation">
        <option value="" disabled selected>go to page…</option>
        {#each project.scan.pages as page}
          <option value={page.folder}>{page.folder === "." ? "home" : page.folder}</option>
        {/each}
      </select>
    {/if}
    <button class="primary" onclick={reloadPreview}>Refresh</button>
  </div>

  <div class="stage">
    {#if !project.previewUrl}
      <div class="empty-state">
        <p><strong>No project open.</strong></p>
        <p>Use File → Open Project Folder (or import a ZIP).<br />
        The preview server binds 127.0.0.1 only and resolves every path
        strictly inside the project root.</p>
      </div>
    {:else}
      <div
        class="viewport"
        style="width: {(rotated ? 900 : viewportWidth) * zoom}px; height: {(rotated ? viewportWidth : 900) * zoom}px;"
      >
        <iframe
          bind:this={frame}
          title="Website preview"
          style="width: {rotated ? 900 : viewportWidth}px; height: {rotated ? viewportWidth : 900}px; transform: scale({zoom}) {rotated ? 'rotate(90deg) translate(0px, -100%)' : ''}; transform-origin: top left;"
        ></iframe>
      </div>
      <div class="stage-note">
        viewport {rotated ? 900 : viewportWidth} × {rotated ? viewportWidth : 900} CSS px
        {#if preset.note}
          · <span class="estimate" title={preset.note}>estimated device values (not yet verified on hardware)</span>
        {/if}
        · click an element in the preview to select it
      </div>
    {/if}
  </div>
</div>

<style>
  .preview-pane {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--panel-dark);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--edge);
    flex-wrap: wrap;
  }

  .preset.active {
    border-color: var(--accent);
    color: #fff;
    background: var(--accent-press);
  }

  .sep {
    width: 1px;
    height: 18px;
    background: var(--edge-light);
  }

  .inline-label {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--fg-dim);
    font-size: 12px;
  }

  .inline-label input[type="number"] {
    width: 70px;
  }

  .zoom-value {
    min-width: 36px;
    color: var(--fg);
  }

  .spacer {
    flex: 1;
  }

  .stage {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 16px;
  }

  .viewport {
    background: #fff;
    box-shadow: 0 0 0 1px var(--edge-light);
    overflow: hidden;
  }

  iframe {
    border: none;
    background: #fff;
  }

  .stage-note {
    margin-top: 8px;
    color: var(--fg-dim);
    font-size: 11.5px;
  }

  .estimate {
    color: var(--warn);
    cursor: help;
  }
</style>
