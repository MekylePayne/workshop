<script lang="ts">
  import { project, call, log } from "../lib/state.svelte.js";

  let profile = $state("hostsmith");
  let requiredClasses = $state([]);
  let results = $state([]);
  let ran = $state(false);

  async function loadSettings() {
    try {
      const config = await call("get_settings");
      requiredClasses = config.badgeRequiredClasses || [];
    } catch {
      requiredClasses = [];
    }
  }

  async function run() {
    results = await call("validate_structure", { profile, requiredClasses });
    ran = true;
    const errors = results.filter((r) => r.severity === "error").length;
    const warnings = results.filter((r) => r.severity === "warning").length;
    log("info", `structure validation: ${errors} error(s), ${warnings} warning(s)`);
  }

  loadSettings();
</script>

<div>
  <div class="panel-title">Project Structure Validator</div>
  <p class="muted">
    Reports only - the validator never restructures a client's website
    (spec Section 13). The HostSmith profile enforces the conventions; the
    generic profile downgrades everything to advisory.
  </p>
  <div class="section">
    <div class="row">
      <label class="inline-label"><input type="radio" bind:group={profile} value="hostsmith" /> HostSmith (enforcing)</label>
      <label class="inline-label"><input type="radio" bind:group={profile} value="generic" /> Generic static site (advisory)</label>
    </div>
    <div class="field">
      <label>Badge signature classes (PR-08 - leave empty for pending mode: any hs- class counts)</label>
      <input class="grow" placeholder="hs-footer, hs-footer-link" oninput={(e) => (requiredClasses = e.target.value.split(",").map((s) => s.trim()).filter(Boolean))} value={requiredClasses.join(", ")} />
    </div>
    <button class="primary" onclick={run} disabled={!project.root}>Validate</button>
  </div>

  {#if ran}
    <div class="section">
      {#each results as result}
        <div class="finding {result.severity}">
          <div class="finding-head"><span class="finding-id">{result.id}</span><span class="finding-msg">{result.message}</span></div>
          <p class="finding-note">{result.explanation}</p>
        </div>
      {/each}
    </div>
  {/if}
</div>
