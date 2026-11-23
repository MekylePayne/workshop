<script lang="ts">
  import { project, ui } from "../lib/state.svelte.js";
</script>

<footer class="statusbar">
  <span class="status-item">{project.root ? project.root : "no project open"}</span>
  {#if project.scan}
    <span class="status-item">{project.scan.pages.length} pages · {project.scan.totalFiles} files · {(project.scan.totalBytes / 1024).toFixed(0)} KB</span>
    <span class="status-item">{project.previewUrl}</span>
  {/if}
  {#if project.scan && !project.scan.imageDirPresent}
    <span class="status-item warn">/image/ missing</span>
  {/if}
  <span class="spacer"></span>
  {#if ui.busy}
    <span class="status-item">working…</span>
  {:else}
    <span class="status-item ok">ready</span>
  {/if}
</footer>

<style>
  .statusbar {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 3px 10px;
    background: #26282b;
    border-top: 1px solid var(--edge);
    color: var(--fg-dim);
    font-size: 11.5px;
    user-select: none;
  }

  .status-item {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-item.warn {
    color: var(--warn);
  }

  .status-item.ok {
    color: var(--ok);
  }

  .spacer {
    flex: 1;
  }
</style>
