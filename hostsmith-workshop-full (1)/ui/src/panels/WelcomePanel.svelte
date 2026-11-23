<script lang="ts">
  import { project } from "../lib/state.svelte.js";
</script>

<div>
  <div class="panel-title">HostSmith Workshop</div>
  <div class="section">
    <p class="muted">
      A local-first workshop for static websites. Everything runs on this
      machine: the preview server binds 127.0.0.1 only, website files are
      never touched without a previewed, journalled change, and nothing is
      uploaded anywhere unless you explicitly deploy.
    </p>
  </div>
  <div class="section">
    <div class="panel-title">Start here</div>
    <p class="muted">
      1. <strong>File → Open Project Folder</strong> (or import a ZIP - it
      extracts to a safely named folder on your Desktop).<br />
      2. Click elements in the preview to select them - the tool panel on
      the right follows the selection.<br />
      3. Every change shows a diff first, is backed up, and can be undone -
      even after a restart.
    </p>
  </div>
  {#if project.scan}
    <div class="section">
      <div class="panel-title">Project</div>
      <dl class="kv">
        <dt>Pages</dt><dd>{project.scan.pages.length}</dd>
        <dt>Files</dt><dd>{project.scan.totalFiles}</dd>
        <dt>/image/</dt><dd>{project.scan.imageDirPresent ? "present" : "missing (HostSmith convention)"}</dd>
        <dt>robots.txt</dt><dd>{project.scan.robotsPresent ? "present" : "missing"}</dd>
        <dt>sitemap.xml</dt><dd>{project.scan.sitemapPresent ? "present" : "missing"}</dd>
        <dt>404.html</dt><dd>{project.scan.notFoundPagePresent ? "present" : "missing"}</dd>
        <dt>_headers</dt><dd>{project.scan.headersPresent ? "present" : "missing"}</dd>
      </dl>
    </div>
  {/if}
  <div class="section">
    <div class="panel-title">Open inputs (pending from you)</div>
    <p class="muted">
      - The exact HostSmith footer badge markup is still pending (PR-08).
      Supply it in Settings and the validator and one-click apply use it
      verbatim - the app never invents a replacement.<br />
      - The Galaxy A13 preset uses estimated CSS viewport values
      (360 × 800, PR-05); verify on a physical device before shipping.
    </p>
  </div>
  <div class="section">
    <div class="panel-title">Honest status</div>
    <p class="muted">
      The Rust engine is fully implemented and tested (the test suite covers
      ZIP safety, the rewrite engine, the journal, SEO checks and the
      deployment protocol). The Windows installer build runs in continuous
      integration - this sandbox cannot produce signed Windows binaries.
    </p>
  </div>
</div>
