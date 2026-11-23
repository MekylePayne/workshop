<script lang="ts">
  import { ui, call, log, clearLog, refreshProject, refreshJournal, reloadPreview } from "../lib/state.svelte.js";

  let backups = $state([]);
  let confirmingRestore = $state("");

  async function loadBackups() {
    try {
      backups = await call("list_backups");
    } catch {
      backups = [];
    }
  }

  async function doUndo(redo) {
    try {
      const outcome = await call("undo_step", { redo });
      if (!outcome) {
        log("info", redo ? "nothing to redo" : "nothing to undo");
        return;
      }
      log("ok", `${redo ? "redo" : "undo"}: ${outcome.description} (${outcome.files.join(", ") || "no files"})`);
      if (outcome.skipped?.length) {
        log("error", `skipped: ${outcome.skipped.join("; ")}`);
      }
      await refreshProject();
      reloadPreview();
      await refreshJournal();
    } catch {
      /* logged by call() */
    }
  }

  function fmtTime(unix) {
    return unix ? new Date(unix * 1000).toLocaleString() : "";
  }

  async function restore(backup) {
    if (confirmingRestore !== backup.path) {
      confirmingRestore = backup.path;
      return;
    }
    confirmingRestore = "";
    await call("restore_backup", { backupPath: backup.path, only: null });
    log("ok", `restored ${backup.files.length} file(s) from ${backup.path.split(/[\\/]/).pop()}`);
    await refreshProject();
    reloadPreview();
    await loadBackups();
  }

  $effect(() => {
    if (ui.bottomTab === "backups") loadBackups();
  });

  window.addEventListener("hsw-undo", () => doUndo(false));
  window.addEventListener("hsw-redo", () => doUndo(true));
</script>

<div class="bottom">
  <div class="tabbar">
    <button class="tab" class:active={ui.bottomTab === "log"} onclick={() => (ui.bottomTab = "log")}>Diagnostics ({ui.log.length})</button>
    <button class="tab" class:active={ui.bottomTab === "changes"} onclick={() => { ui.bottomTab = "changes"; refreshJournal(); }}>Change history ({ui.journal.length})</button>
    <button class="tab" class:active={ui.bottomTab === "backups"} onclick={() => (ui.bottomTab = "backups")}>Backups</button>
    <span class="spacer"></span>
    <button onclick={() => doUndo(false)} title="Undo survives an application restart (MR-03)">Undo</button>
    <button onclick={() => doUndo(true)}>Redo</button>
    <button onclick={() => clearLog()}>Clear log</button>
  </div>

  {#if ui.bottomTab === "log"}
    <div class="log">
      {#each ui.log as line}
        <div class="log-line {line.kind}"><span class="kind">{line.kind}</span><span class="stamp">{line.stamp}</span>{line.message}</div>
      {/each}
      {#if ui.log.length === 0}
        <p class="muted">No diagnostics yet. Selections, edits, deployments and errors are all logged here.</p>
      {/if}
    </div>
  {:else if ui.bottomTab === "changes"}
    <div class="scrollarea">
      <table class="data">
        <thead><tr><th>When</th><th>Operation</th><th>Files</th><th>State</th></tr></thead>
        <tbody>
          {#each ui.journal as entry}
            <tr>
              <td>{fmtTime(entry.time_unix)}</td>
              <td>{entry.description}</td>
              <td>{entry.files.map((f) => f.path).join(", ")}</td>
              <td>
                {#if entry.invalidated}<span class="badge">invalidated (external edit)</span>
                {:else if entry.undone}<span class="badge">undone (redoable)</span>
                {:else}<span class="badge">applied</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if ui.journal.length === 0}
        <p class="muted">No changes recorded yet. Every confirmed edit lands here with its backup, and undo/redo survive an application restart (MR-03).</p>
      {/if}
    </div>
  {:else}
    <div class="scrollarea">
      <table class="data">
        <thead><tr><th>Backup</th><th>Created</th><th>Files</th><th></th></tr></thead>
        <tbody>
          {#each backups as backup}
            <tr>
              <td>{backup.path.split(/[\\/]/).pop()}</td>
              <td>{fmtTime(backup.created_unix)}</td>
              <td>{backup.files.join(", ")}</td>
              <td>
                <button class:danger={confirmingRestore === backup.path} onclick={() => restore(backup)}>
                  {confirmingRestore === backup.path ? "Click again to confirm restore" : "Restore"}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if backups.length === 0}
        <p class="muted">No backups yet. Every edit that replaces content first writes a timestamped ZIP outside the website folder (MR-02), so restores never end up in deployments.</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .bottom {
    height: 190px;
    flex-shrink: 0;
    background: var(--panel);
    border-top: 1px solid var(--edge);
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
  }

  .spacer {
    flex: 1;
  }

  .log,
  .scrollarea {
    flex: 1;
    overflow: auto;
  }

  .tabbar button {
    font-size: 12px;
  }
</style>
