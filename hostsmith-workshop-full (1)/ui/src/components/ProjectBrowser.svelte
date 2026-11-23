<script lang="ts">
  import { project, selection, call, openPanel } from "../lib/state.svelte.js";

  let entries = $state([]);
  let current = $state("");
  let loading = $state(false);

  async function load(rel) {
    current = rel;
    loading = true;
    try {
      entries = await call("list_project_dir", { rel });
    } catch {
      entries = [];
    }
    loading = false;
  }

  function crumbs(rel) {
    const parts = rel ? rel.split("/") : [];
    const out = [{ name: "root", rel: "" }];
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      out.push({ name: part, rel: acc });
    }
    return out;
  }

  function selectFile(entry) {
    selection.file = entry.rel;
  }

  function openFile(entry) {
    selectFile(entry);
    if (entry.is_dir) {
      load(entry.rel);
    } else if (/\.(html?|css|js|mjs|json|xml|txt|md|webmanifest)$/i.test(entry.name)) {
      openPanel("source");
    }
  }

  const isImage = (name) => /\.(png|jpe?g|gif|webp|svg|avif|ico|bmp)$/i.test(name);
  const isVideo = (name) => /\.(mp4|webm|mov|mkv)$/i.test(name);
  const isCode = (name) => /\.(html?|css|js|mjs|json|xml|txt|md|webmanifest)$/i.test(name);

  $effect(() => {
    if (project.root) load("");
  });
</script>

<div class="browser">
  <div class="panel-title">Project files</div>
  <div class="crumbs">
    {#each crumbs(current) as crumb, index}
      {#if index > 0}<span class="sep">/</span>{/if}
      <button class="crumb" onclick={() => load(crumb.rel)}>{crumb.name}</button>
    {/each}
  </div>
  <div class="listing">
    {#if loading}
      <p class="muted">loading…</p>
    {:else if entries.length === 0}
      <p class="muted">empty folder</p>
    {/if}
    {#each entries as entry (entry.rel)}
      <button
        class="entry"
        class:selected={selection.file === entry.rel}
        onclick={() => selectFile(entry)}
        ondblclick={() => openFile(entry)}
        title={entry.is_dir ? "double-click to open folder" : "double-click to open in the source editor"}
      >
        <span class="glyph">{entry.is_dir ? "▤" : isImage(entry.name) ? "▣" : isVideo(entry.name) ? "▶" : isCode(entry.name) ? "≡" : "·"}</span>
        <span class="name">{entry.name}</span>
        {#if !entry.is_dir}
          <span class="size">{entry.size > 1024 ? `${(entry.size / 1024).toFixed(1)} KB` : `${entry.size} B`}</span>
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .browser {
    width: 230px;
    flex-shrink: 0;
    background: var(--panel);
    border-right: 1px solid var(--edge);
    padding: 8px 10px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .crumbs {
    margin-bottom: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    align-items: center;
  }

  .crumb {
    background: none;
    border: none;
    color: var(--accent);
    padding: 1px 2px;
    font-size: 12px;
  }

  .sep {
    color: var(--fg-dim);
  }

  .listing {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .entry {
    display: flex;
    gap: 6px;
    align-items: center;
    background: none;
    border: 1px solid transparent;
    text-align: left;
    padding: 3px 6px;
    font-size: 12px;
  }

  .entry:hover {
    background: #35393c;
  }

  .entry.selected {
    background: #3a4147;
    border-color: #4c565e;
  }

  .glyph {
    width: 14px;
    color: var(--fg-dim);
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .size {
    color: var(--fg-dim);
    font-size: 11px;
  }
</style>
