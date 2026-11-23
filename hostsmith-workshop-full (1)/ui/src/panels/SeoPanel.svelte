<script lang="ts">
  import { project, selection, call, log, refreshProject, reloadPreview, refreshJournal } from "../lib/state.svelte.js";

  let analysis = $state(null);
  let running = $state(false);
  let guidedValue = $state("");
  let preview = $state(null);

  const counts = $derived.by(() => {
    const result = { error: 0, warning: 0, passed: 0, info: 0 };
    if (analysis) for (const finding of analysis.findings) result[finding.severity] += 1;
    return result;
  });

  async function runAnalysis() {
    running = true;
    try {
      analysis = await call("analyze_seo");
      log("info", `SEO analysis finished: ${analysis.pagesAnalyzed} page(s), ${counts.error} error(s), ${counts.warning} warning(s)`);
    } finally {
      running = false;
    }
  }

  function guidedKindFor(finding) {
    if (finding.id === "SEO-01" || finding.id === "SEO-02") return "title";
    if (finding.id === "SEO-03" || finding.id === "SEO-04") return "description";
    if (finding.id === "SEO-08") return "canonical";
    return "viewport";
  }

  async function runFix(finding, value) {
    const rel = finding.page === "site"
      ? selection.page.replace(/^\//, "") || "index.html"
      : `${finding.page === "." ? "" : `${finding.page}/`}index.html`;
    preview = await call("plan_seo_fix", {
      pagePath: rel,
      kind: finding.autoFix ? (finding.autoFix === "lang" ? "lang" : "viewport") : guidedKindFor(finding),
      value: value ?? null,
      lang: "en",
    });
  }

  async function applyFix() {
    await call("apply_edited_files", {
      files: [{ path: preview.path, newContent: preview.newContent }],
      description: `SEO fix on ${preview.path}`,
    });
    log("ok", `SEO fix applied to ${preview.path} (+${preview.added}/-${preview.removed})`);
    preview = null;
    await refreshProject();
    reloadPreview();
    await refreshJournal();
    await runAnalysis();
  }

  const autoFixable = $derived(analysis ? analysis.findings.filter((f) => f.autoFix && f.severity !== "passed") : []);
  const needsInput = $derived(analysis ? analysis.findings.filter((f) => f.needsInput && f.severity !== "passed") : []);
  const rest = $derived(
    analysis
      ? analysis.findings.filter((f) => f.severity !== "passed" && !f.autoFix && !f.needsInput)
      : []
  );
</script>

<div>
  <div class="panel-title">SEO Inspector - local technical checks</div>
  <p class="muted">
    On-page technical checks only. No ranking predictions, no Core Web
    Vitals, no off-page signals (TL-08) - this inspector tells you what a
    crawler can read locally, not where Google will rank you.
  </p>
  <div class="row">
    <button class="primary" onclick={runAnalysis} disabled={running || !project.root}>Run analysis</button>
    {#if analysis}
      <span class="muted">{analysis.pagesAnalyzed} page(s): {counts.error} errors, {counts.warning} warnings, {counts.passed} passed, {counts.info} info</span>
    {/if}
  </div>

  {#if autoFixable.length}
    <div class="section">
      <div class="panel-title">Automatic fixes (one click, diff before apply)</div>
      {#each autoFixable as finding}
        <div class="finding {finding.severity}">
          <div class="finding-head"><span class="finding-id">{finding.id}</span><span class="finding-msg">{finding.message}</span></div>
          <p class="finding-note">{finding.recommendation}</p>
          <button onclick={() => runFix(finding)}>Preview fix on {finding.page}</button>
        </div>
      {/each}
    </div>
  {/if}

  {#if needsInput.length}
    <div class="section">
      <div class="panel-title">Guided fixes (need your wording)</div>
      <div class="field">
        <label for="guided-value">Value for the guided fix (title, description or canonical URL)</label>
        <input id="guided-value" bind:value={guidedValue} placeholder="e.g. Handcrafted blades - HostSmith Workshop" />
      </div>
      {#each needsInput as finding}
        <div class="finding {finding.severity}">
          <div class="finding-head"><span class="finding-id">{finding.id}</span><span class="finding-msg">{finding.message}</span> <span class="badge">{finding.page}</span></div>
          <p class="finding-note">{finding.recommendation}</p>
          <button disabled={!guidedValue.trim()} onclick={() => runFix(finding, guidedValue.trim())}>Preview fix</button>
        </div>
      {/each}
    </div>
  {/if}

  {#if rest.length}
    <div class="section">
      <div class="panel-title">Other findings</div>
      {#each rest as finding}
        <div class="finding {finding.severity}">
          <div class="finding-head"><span class="finding-id">{finding.id}</span><span class="finding-msg">{finding.message}</span> <span class="badge">{finding.page}</span></div>
          <p class="finding-note">{finding.recommendation}</p>
        </div>
      {/each}
    </div>
  {/if}

  {#if preview}
    <div class="section">
      <div class="panel-title">Proposed fix ({preview.path})</div>
      <pre class="diff">{preview.diff}</pre>
      <div class="row">
        <button class="primary" onclick={applyFix}>Apply fix</button>
        <button onclick={() => (preview = null)}>Cancel</button>
      </div>
    </div>
  {/if}
</div>
