<script lang="ts">
  import { onMount } from "svelte";
  import TopMenu from "./components/TopMenu.svelte";
  import ToolRail from "./components/ToolRail.svelte";
  import ProjectBrowser from "./components/ProjectBrowser.svelte";
  import PreviewPane from "./components/PreviewPane.svelte";
  import BottomPanel from "./components/BottomPanel.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import WelcomePanel from "./panels/WelcomePanel.svelte";
  import ImageStudioPanel from "./panels/ImageStudioPanel.svelte";
  import PatchworkPanel from "./panels/PatchworkPanel.svelte";
  import FaviconPanel from "./panels/FaviconPanel.svelte";
  import SeoPanel from "./panels/SeoPanel.svelte";
  import ImageWorkshopPanel from "./panels/ImageWorkshopPanel.svelte";
  import ToolboxPanel from "./panels/ToolboxPanel.svelte";
  import ValidatorPanel from "./panels/ValidatorPanel.svelte";
  import CloudflarePanel from "./panels/CloudflarePanel.svelte";
  import SettingsPanel from "./panels/SettingsPanel.svelte";
  import SourcePanel from "./panels/SourcePanel.svelte";
  import { ui, project } from "./lib/state.svelte.js";
  import { setupBridge } from "./lib/bridge.js";

  const panelMap: Record<string, any> = {
    welcome: WelcomePanel,
    "image-studio": ImageStudioPanel,
    patchwork: PatchworkPanel,
    favicon: FaviconPanel,
    seo: SeoPanel,
    "image-workshop": ImageWorkshopPanel,
    toolbox: ToolboxPanel,
    validator: ValidatorPanel,
    cloudflare: CloudflarePanel,
    settings: SettingsPanel,
    source: SourcePanel,
  };

  let propertiesVisible = $state(true);
  let diagnosticsVisible = $state(true);
  let browserVisible = $state(true);

  const ActivePanel = $derived(panelMap[ui.activePanel] ?? WelcomePanel);

  onMount(() => {
    setupBridge();
  });
</script>

<div class="shell">
  <TopMenu>
    <button class="toggle" class:active={browserVisible} onclick={() => (browserVisible = !browserVisible)}>Browser</button>
    <button class="toggle" class:active={propertiesVisible} onclick={() => (propertiesVisible = !propertiesVisible)}>Panel</button>
    <button class="toggle" class:active={diagnosticsVisible} onclick={() => (diagnosticsVisible = !diagnosticsVisible)}>Diagnostics</button>
  </TopMenu>

  <div class="workspace">
    <ToolRail />
    {#if browserVisible && project.root}
      <ProjectBrowser />
    {/if}
    <div class="center">
      <PreviewPane />
      {#if diagnosticsVisible}
        <BottomPanel />
      {/if}
    </div>
    {#if propertiesVisible}
      <aside class="right-panel">
        <ActivePanel />
      </aside>
    {/if}
  </div>

  <StatusBar />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--fg);
    font-family: "Segoe UI", Arial, sans-serif;
    font-size: 13px;
  }

  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .right-panel {
    width: 360px;
    flex-shrink: 0;
    overflow-y: auto;
    background: var(--panel);
    border-left: 1px solid var(--edge);
    padding: 10px 12px 20px;
  }
</style>
