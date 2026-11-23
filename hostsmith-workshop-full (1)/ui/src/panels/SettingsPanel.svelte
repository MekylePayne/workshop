<script lang="ts">
  import { call, log } from "../lib/state.svelte.js";

  let requiredClasses = $state("");
  let badgeSnippet = $state("");
  let savedBadge = $state("");
  let message = $state("");

  async function load() {
    const config = await call("get_settings");
    requiredClasses = (config.badgeRequiredClasses || []).join(", ");
    badgeSnippet = config.badgeSnippet || "";
    savedBadge = config.badgeSnippet ? "supplied" : "pending";
  }
  load();

  async function saveSignature() {
    await call("set_badge_signature", {
      requiredClasses: requiredClasses.split(",").map((s) => s.trim()).filter(Boolean),
    });
    log("ok", "badge signature saved");
    message = "Badge signature saved.";
  }

  async function saveSnippet() {
    try {
      await call("set_badge_snippet", { snippet: badgeSnippet });
      savedBadge = "supplied";
      log("ok", "exact badge snippet stored verbatim (PR-08 satisfied)");
      message = "Badge snippet stored. The validator now checks the exact signature and Apply Badge inserts it verbatim.";
    } catch {
      /* logged by call() */
    }
  }
</script>

<div>
  <div class="panel-title">Settings</div>

  <div class="section">
    <div class="panel-title">HostSmith footer badge (PR-08)</div>
    {#if savedBadge !== "supplied"}
      <div class="finding warning">
        <p class="finding-note">
          Pending: the exact badge markup has not been supplied yet. The
          validator runs in pending mode (any hs- class). Paste the original
          snippet below - it is stored and applied byte-for-byte; the
          application never invents a replacement.
        </p>
      </div>
    {:else}
      <div class="finding passed"><p class="finding-note">Exact badge snippet stored (applied verbatim).</p></div>
    {/if}
    <div class="field">
      <label>Required class names (comma-separated, used by the strict signature check)</label>
      <input bind:value={requiredClasses} placeholder="hs-footer, hs-footer-link" />
    </div>
    <button onclick={saveSignature}>Save signature</button>
    <div class="field">
      <label>Exact badge markup (HTML + inline styles as supplied)</label>
      <textarea rows="6" bind:value={badgeSnippet} placeholder="paste the original HostSmith footer badge snippet here"></textarea>
    </div>
    <button class="primary" onclick={saveSnippet} disabled={!badgeSnippet.trim()}>Store badge snippet</button>
    {#if message}<p class="muted">{message}</p>{/if}
  </div>

  <div class="section">
    <div class="panel-title">Privacy posture (fixed, not configurable)</div>
    <p class="muted">
      No account, no subscription, no telemetry, no cloud AI. The preview
      server binds 127.0.0.1 only. The only network traffic is Cloudflare
      API calls you explicitly trigger. Website files never leave this
      machine unless you deploy.
    </p>
  </div>

  <div class="section">
    <div class="panel-title">Backups</div>
    <p class="muted">
      Every edit writes a timestamped ZIP outside the website folder before
      anything is modified; retention is capped at 50 backups per project in
      this version. Restores clear the journal so no undo can revert onto a
      restored state unknowingly.
    </p>
  </div>
</div>
