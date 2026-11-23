<script lang="ts">
  import { selection, call, log, refreshProject, reloadPreview, refreshJournal } from "../lib/state.svelte.js";

  let edited = $state("");
  let preview = $state(null);

  const canEdit = $derived(
    selection.page && selection.textOrdinal !== null && selection.elementIndex !== null
  );

  function loadSelection() {
    edited = selection.text || "";
    preview = null;
  }

  async function propose() {
    preview = await call("propose_text_edit", {
      path: selection.page.replace(/^\//, ""),
      elementIndex: selection.elementIndex,
      ordinal: selection.textOrdinal,
      replacement: edited,
    });
  }

  async function apply() {
    await call("apply_edited_files", {
      files: [{ path: preview.path, newContent: preview.newContent }],
      description: `Edit text in <${selection.tag}> on ${selection.page}`,
    });
    log("ok", `text change applied to ${preview.path} (+${preview.added}/-${preview.removed} lines)`);
    preview = null;
    await refreshProject();
    reloadPreview();
    await refreshJournal();
  }

  $effect(() => {
    if (selection.text) loadSelection();
  });
</script>

<div>
  <div class="panel-title">Patchwork - edit text in place</div>

  {#if !canEdit}
    <p class="muted">
      Click directly on a piece of text in the live preview. Patchwork
      replaces only that one text node in the source - nested tags, links,
      attributes and everything around it keep their exact original bytes
      (review 6.3). The diff is shown before anything is written, and the
      change is undoable.
    </p>
    {#if selection.page}
      <p class="muted">
        Current selection: &lt;{selection.tag}&gt;#{selection.elementIndex}
        {#if selection.text === null}
          - click directly on the text itself (not just the element) to edit it.
        {/if}
      </p>
    {/if}
  {:else}
    <div class="section">
      <div class="panel-title">Selected text</div>
      <p class="muted">&lt;{selection.tag}&gt; · text node {selection.textOrdinal} · {selection.page}</p>
      <div class="field">
        <label for="patchwork-text">Replacement text (entities are escaped automatically)</label>
        <textarea id="patchwork-text" rows="4" bind:value={edited}></textarea>
      </div>
      <div class="row">
        <button class="primary" onclick={propose} disabled={!edited.length && !selection.text.length}>Preview change</button>
      </div>
    </div>
    {#if preview}
      <div class="section">
        <div class="panel-title">Proposed source change ({preview.path})</div>
        <pre class="diff">{preview.diff}</pre>
        <div class="row">
          <button class="primary" onclick={apply}>Apply (backed up, undoable)</button>
          <button onclick={() => (preview = null)}>Cancel</button>
        </div>
      </div>
    {/if}
  {/if}
</div>
