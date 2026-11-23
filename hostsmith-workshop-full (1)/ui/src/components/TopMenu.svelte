<script lang="ts">
  import { openPanel, openProjectFolder, closeProject, call, log, reloadPreview } from "../lib/state.svelte.js";

  let { children } = $props();

  let open = $state("");

  async function openFolder() {
    open = "";
    const path = await call("pick_folder");
    if (!path) return;
    await openProjectFolder(path);
  }

  async function importZip() {
    open = "";
    const zipPath = await call("pick_file");
    if (!zipPath) return;
    const base = String(zipPath).split(/[\\/]/).pop() || "imported-site";
    const name = base.replace(/\.zip$/i, "");
    log("info", `importing ${base} to the Desktop as "${name}"...`);
    await call("import_zip", { zipPath, folderName: name });
    // import_zip opens the extracted folder as the project
    openPanel("image-studio");
  }

  const menus = $state({
    File: [
      { label: "Open Project Folder…", action: openFolder },
      { label: "Import ZIP to Desktop…", action: importZip },
      { label: "Close Project", action: () => { open = ""; closeProject(); } },
    ],
    Edit: [
      { label: "Undo last change", action: () => { open = ""; window.dispatchEvent(new CustomEvent("hsw-undo")); } },
      { label: "Redo", action: () => { open = ""; window.dispatchEvent(new CustomEvent("hsw-redo")); } },
    ],
    View: [
      { label: "Reload preview", action: () => { open = ""; reloadPreview(); } },
    ],
    Tools: [
      { label: "Image Studio", action: () => { open = ""; openPanel("image-studio"); } },
      { label: "Patchwork (text editing)", action: () => { open = ""; openPanel("patchwork"); } },
      { label: "Favicon Studio", action: () => { open = ""; openPanel("favicon"); } },
      { label: "SEO Inspector", action: () => { open = ""; openPanel("seo"); } },
      { label: "Image Workshop", action: () => { open = ""; openPanel("image-workshop"); } },
      { label: "Developer Toolbox", action: () => { open = ""; openPanel("toolbox"); } },
      { label: "Structure Validator", action: () => { open = ""; openPanel("validator"); } },
      { label: "Source Editor", action: () => { open = ""; openPanel("source"); } },
      { label: "Settings", action: () => { open = ""; openPanel("settings"); } },
    ],
    Upload: [
      { label: "Cloudflare Pages and Workers…", action: () => { open = ""; openPanel("cloudflare"); } },
    ],
    Help: [
      { label: "About HostSmith Workshop", action: () => { open = ""; log("info", "HostSmith Workshop - local-first static website workshop. Preview: 127.0.0.1 only. No telemetry. Cloudflare is contacted only when you deploy."); } },
    ],
  });

  function toggle(name) {
    open = open === name ? "" : name;
  }

  function run(item) {
    item.action();
  }
</script>

<header class="menubar">
  <span class="brand">HostSmith Workshop</span>
  {#each Object.keys(menus) as name}
    <div class="menu">
      <button class="menu-item" class:active={open === name} onclick={() => toggle(name)}>{name}</button>
      {#if open === name}
        <div class="dropdown">
          {#each menus[name] as item}
            <button class="dropdown-item" onclick={() => run(item)}>{item.label}</button>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
  <span class="spacer"></span>
  {@render children?.()}
</header>

<style>
  .menubar {
    display: flex;
    align-items: center;
    gap: 2px;
    background: #2b2e31;
    border-bottom: 1px solid var(--edge);
    padding: 4px 8px;
    user-select: none;
  }

  .brand {
    font-weight: 600;
    margin-right: 12px;
    color: var(--fg-strong);
  }

  .menu {
    position: relative;
  }

  .menu-item {
    background: none;
    border: 1px solid transparent;
    color: var(--fg);
    padding: 4px 10px;
  }

  .menu-item:hover,
  .menu-item.active {
    background: #35393c;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 50;
    min-width: 240px;
    background: #2e3134;
    border: 1px solid var(--edge-light);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
  }

  .dropdown-item {
    background: none;
    border: none;
    text-align: left;
    padding: 6px 12px;
  }

  .dropdown-item:hover {
    background: #3a4147;
  }

  .spacer {
    flex: 1;
  }
</style>
