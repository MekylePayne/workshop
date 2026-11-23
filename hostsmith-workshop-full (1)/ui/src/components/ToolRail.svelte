<script lang="ts">
  import { ui, openPanel } from "../lib/state.svelte.js";

  const tools = [
    { id: "image-studio", label: "Image Studio", hint: "Select and replace images and video in the preview" },
    { id: "patchwork", label: "Patchwork", hint: "Edit text directly on the page" },
    { id: "favicon", label: "Favicons", hint: "Favicon detection, generation and previews" },
    { id: "seo", label: "SEO", hint: "Local technical SEO analysis and fixes" },
    { id: "image-workshop", label: "Image Worksh.", hint: "Resize, crop, rotate, convert, optimize" },
    { id: "toolbox", label: "Toolbox", hint: "JSON, minifiers, diff, QR, colours, counters" },
    { id: "validator", label: "Validator", hint: "HostSmith structure validation" },
    { id: "source", label: "Source", hint: "Inspect and edit raw source files" },
    { id: "cloudflare", label: "Upload", hint: "Cloudflare Pages and Workers deployment" },
    { id: "settings", label: "Settings", hint: "Badge signature, defaults, backups" },
  ];
</script>

<nav class="toolrail">
  {#each tools as tool}
    <button
      class="tool"
      class:active={ui.activePanel === tool.id}
      title={tool.hint}
      onclick={() => openPanel(tool.id)}
    >
      {tool.label}
    </button>
  {/each}
</nav>

<style>
  .toolrail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 118px;
    flex-shrink: 0;
    padding: 6px;
    background: var(--panel);
    border-right: 1px solid var(--edge);
  }

  .tool {
    background: none;
    border: 1px solid transparent;
    text-align: left;
    padding: 6px 10px;
    color: var(--fg);
  }

  .tool:hover {
    background: #35393c;
  }

  .tool.active {
    background: #3a4147;
    border-color: #4c565e;
    color: #ffffff;
  }
</style>
