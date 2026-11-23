<script lang="ts">
  import { project, call, log, ui } from "../lib/state.svelte.js";

  let token = $state("");
  let connected = $state(false);
  let accounts = $state([]);
  let selectedAccount = $state("");
  let projects = $state([]);
  let selectedProject = $state("");
  let newProjectName = $state("");
  let productionBranch = $state("main");
  let deployConfirm = $state(false);
  let deploying = $state(false);
  let deployResult = $state(null);
  let workers = $state([]);
  let workerName = $state("");
  let workerCode = $state("export default { fetch() { return new Response('ok'); } }");
  let deleteConfirm = $state("");

  async function connect() {
    await call("cf_save_token", { token });
    connected = await call("cf_has_token");
    if (connected) {
      log("ok", "scoped API token stored in the OS credential store");
      token = "";
      await loadAccounts();
    }
  }

  async function loadAccounts() {
    accounts = await call("cf_accounts");
    if (accounts.length) selectedAccount = accounts[0].id;
    await loadProjects();
  }

  async function loadProjects() {
    if (!selectedAccount) return;
    projects = await call("cf_pages_projects", { account: selectedAccount });
    if (projects.length) selectedProject = projects[0].name;
    workers = await call("cf_workers_scripts", { account: selectedAccount }).catch(() => []);
  }

  async function createProject() {
    await call("cf_create_pages_project", {
      account: selectedAccount,
      name: newProjectName,
      productionBranch,
    });
    log("ok", `Pages project "${newProjectName}" created`);
    newProjectName = "";
    await loadProjects();
  }

  async function deploy() {
    if (!deployConfirm) {
      deployConfirm = true;
      return;
    }
    deployConfirm = false;
    deploying = true;
    deployResult = null;
    try {
      deployResult = await call("cf_deploy_pages", {
        account: selectedAccount,
        project: selectedProject,
      });
      log("ok", `deployment finished: ${deployResult.uploaded} uploaded, ${deployResult.alreadyPresent} already present -> ${deployResult.url || "(no URL returned)"}`);
    } finally {
      deploying = false;
    }
  }

  async function uploadWorker() {
    await call("cf_upload_worker", {
      account: selectedAccount,
      name: workerName,
      mainModule: "index.js",
      code: workerCode,
      bindingsJson: "[]",
    });
    log("ok", `worker script "${workerName}" uploaded`);
    await loadProjects();
  }

  async function deleteWorker(name) {
    if (deleteConfirm !== name) {
      deleteConfirm = name;
      return;
    }
    deleteConfirm = "";
    await call("cf_delete_worker", { account: selectedAccount, name, confirm: true });
    log("ok", `worker script "${name}" deleted`);
    await loadProjects();
  }

  async function loadInitial() {
    try {
      connected = await call("cf_has_token");
      if (connected) await loadAccounts();
    } catch {
      connected = false;
    }
  }
  loadInitial();
</script>

<div>
  <div class="panel-title">Cloudflare - Pages and Workers</div>

  {#if !connected}
    <div class="section">
      <p class="muted">
        Create a <strong>scoped API token</strong> in the Cloudflare dashboard
        (Permissions: Account Read, Cloudflare Pages Edit, Workers Scripts Edit).
        HostSmith never accepts Global API Keys. The token is stored in the
        Windows Credential Manager and never written to a project file or log.
      </p>
      <div class="field">
        <label>API token</label>
        <input type="password" bind:value={token} placeholder="paste the scoped token" />
      </div>
      <button class="primary" onclick={connect} disabled={!token.trim()}>Connect</button>
    </div>
  {:else}
    <div class="section">
      <div class="row">
        <span class="badge">token stored</span>
        <button onclick={async () => { await call("cf_disconnect"); connected = false; accounts = []; projects = []; workers = []; }}>Disconnect</button>
      </div>
      {#if accounts.length}
        <div class="field">
          <label>Account</label>
          <select bind:value={selectedAccount} onchange={loadProjects}>
            {#each accounts as account}<option value={account.id}>{account.name}</option>{/each}
          </select>
        </div>
      {/if}
    </div>

    <div class="section">
      <div class="panel-title">Pages deployment</div>
      <div class="field">
        <label>Project</label>
        <select bind:value={selectedProject}>
          {#each projects as projectItem}<option value={projectItem.name}>{projectItem.name} ({projectItem.subdomain || ""})</option>{/each}
        </select>
      </div>
      <div class="row">
        <input placeholder="new project name" bind:value={newProjectName} />
        <input style="width:90px" placeholder="main" bind:value={productionBranch} />
        <button onclick={createProject} disabled={!newProjectName.trim()}>Create</button>
      </div>
      <p class="muted">
        Deployment review: {project.scan ? `${project.scan.totalFiles} files (${(project.scan.totalBytes / 1024).toFixed(0)} KB)` : "open a project first"}
        → target <strong>{selectedProject || "(none selected)"}</strong>. Hidden
        directories and dependency folders are never uploaded. Production
        deployment needs the second confirmation click; nothing is deleted by
        a deployment.
      </p>
      <button class="primary" class:danger={deployConfirm} onclick={deploy} disabled={deploying || !selectedProject}>
        {deploying ? "Deploying…" : deployConfirm ? "Confirm production deployment" : "Deploy this project"}
      </button>
      {#if deployResult}
        <div class="finding passed">
          <p class="finding-note">
            Deployed {deployResult.totalFiles} files ({deployResult.uploaded} uploaded, {deployResult.alreadyPresent} already on the CDN).
            {#if deployResult.url}Live at <a href={deployResult.url}>{deployResult.url}</a>{/if}
          </p>
        </div>
      {/if}
      <p class="muted">The Rust client runs the documented asset-upload flow natively (upload token → check-missing → upload → upsert-hashes → create deployment). No Wrangler, no Node.js.</p>
    </div>

    <div class="section">
      <div class="panel-title">Workers scripts (separate from Pages)</div>
      {#each workers as workerItem}
        <div class="row">
          <span class="muted">{workerItem.id}</span>
          <span class="spacer"></span>
          <button class:danger={deleteConfirm === workerItem.id} onclick={() => deleteWorker(workerItem.id)}>
            {deleteConfirm === workerItem.id ? "Confirm delete" : "Delete"}
          </button>
        </div>
      {/each}
      {#if workers.length === 0}<p class="muted">No worker scripts found on this account.</p>{/if}
      <div class="field"><label>New script name</label><input bind:value={workerName} placeholder="my-worker" /></div>
      <div class="field"><label>Module code (ES module)</label><textarea rows="5" bind:value={workerCode}></textarea></div>
      <button onclick={uploadWorker} disabled={!workerName.trim()}>Upload script</button>
    </div>
  {/if}

  {#if ui.log.filter((line) => line.kind === "info" && line.message.startsWith("deployment")).length}
    <div class="section">
      <div class="panel-title">Deployment log</div>
      {#each ui.log.filter((line) => line.message.startsWith("deployment")) as line}
        <div class="log-line {line.kind}"><span class="stamp">{line.stamp}</span>{line.message}</div>
      {/each}
    </div>
  {/if}
</div>
