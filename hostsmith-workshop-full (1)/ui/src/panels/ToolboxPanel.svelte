<script lang="ts">
  import { call, log, project } from "../lib/state.svelte.js";
  import { format as prettierFormat } from "prettier/standalone";
  import prettierHtml from "prettier/plugins/html";
  import prettierPostcss from "prettier/plugins/postcss";
  import prettierBabel from "prettier/plugins/babel";
  import { marked } from "marked";

  let tool = $state("json");
  let input = $state("");
  let output = $state("");
  let message = $state("");

  // Diff tool state
  let diffBefore = $state("");
  let diffAfter = $state("");

  // Contrast tool state
  let fgColor = $state("#202124");
  let bgColor = $state("#ffffff");

  const tools = [
    ["json", "JSON"],
    ["html", "HTML"],
    ["css", "CSS"],
    ["js", "JavaScript"],
    ["diff", "Diff"],
    ["markdown", "Markdown"],
    ["palette", "Palette"],
    ["contrast", "Contrast"],
    ["qr", "QR code"],
    ["url", "URL codec"],
    ["counter", "Counter"],
    ["lorem", "Lorem Ipsum"],
    ["og", "Open Graph"],
  ];

  function luminance(hex) {
    const value = hex.replace("#", "");
    const channels = [0, 2, 4].map((offset) => {
      const channel = parseInt(value.slice(offset, offset + 2), 16) / 255;
      return channel <= 0.03928 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
  }

  const contrastRatio = $derived.by(() => {
    try {
      const l1 = luminance(fgColor);
      const l2 = luminance(bgColor);
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      return ratio.toFixed(2);
    } catch {
      return "invalid";
    }
  });

  async function runTool(kind) {
    message = "";
    try {
      if (kind === "json-format") output = await call("tool_format_json", { text: input });
      else if (kind === "json-minify") output = await call("tool_minify_json", { text: input });
      else if (kind === "json-validate") {
        const diagnostic = await call("tool_validate_json", { text: input });
        output = diagnostic.message;
        message = diagnostic.line > 0 ? `first problem at line ${diagnostic.line}, column ${diagnostic.column}` : "JSON is valid";
      } else if (kind === "html-minify") output = await call("tool_minify_html", { text: input });
      else if (kind === "css-minify") output = await call("tool_minify_css", { text: input });
      else if (kind === "js-minify") output = await call("tool_minify_js", { text: input });
      else if (kind === "html-format") output = await prettierFormat(input, { parser: "html", plugins: [prettierHtml] });
      else if (kind === "css-format") output = await prettierFormat(input, { parser: "css", plugins: [prettierPostcss] });
      else if (kind === "js-format") output = await prettierFormat(input, { parser: "babel", plugins: [prettierBabel] });
      else if (kind === "markdown") output = await marked.parse(input);
      else if (kind === "qr") output = await call("tool_qr_svg", { text: input, dark: "#000000", light: "#ffffff", width: 220 });
      else if (kind === "url-encode") output = encodeURIComponent(input);
      else if (kind === "url-decode") output = decodeURIComponent(input);
    } catch (error) {
      message = String(error);
    }
  }

  function simpleLineDiff(before, after) {
    const a = before.split("\n");
    const b = after.split("\n");
    // Classic LCS table (fine at tool scale).
    const m = a.length;
    const n = b.length;
    const table = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
    for (let i = m - 1; i >= 0; i--) {
      for (let j = n - 1; j >= 0; j--) {
        table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
      }
    }
    const lines = [];
    let i = 0;
    let j = 0;
    while (i < m && j < n) {
      if (a[i] === b[j]) { lines.push([" ", a[i]]); i++; j++; }
      else if (table[i + 1][j] >= table[i][j + 1]) { lines.push(["-", a[i]]); i++; }
      else { lines.push(["+", b[j]]); j++; }
    }
    while (i < m) { lines.push(["-", a[i]]); i++; }
    while (j < n) { lines.push(["+", b[j]]); j++; }
    return lines;
  }

  function countWords(text) {
    return (text.trim().match(/\S+/g) || []).length;
  }

  function lorem(count = 3) {
    const words = ["lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eius", "tempor", "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua"];
    const sentences = [];
    for (let s = 0; s < count; s++) {
      const sentence = [];
      const length = 8 + Math.floor(Math.random() * 8);
      for (let w = 0; w < length; w++) sentence.push(words[Math.floor(Math.random() * words.length)]);
      let text = sentence.join(" ");
      text = text.charAt(0).toUpperCase() + text.slice(1) + ".";
      sentences.push(text);
    }
    return sentences.join(" ");
  }
</script>

<div>
  <div class="panel-title">Developer Toolbox</div>
  <div class="tabbar">
    {#each tools as [id, label]}
      <button class="tab" class:active={tool === id} onclick={() => { tool = id; output = ""; message = ""; }}>{label}</button>
    {/each}
  </div>

  {#if ["json", "html", "css", "js"].includes(tool)}
    <div class="field"><textarea rows="8" bind:value={input} aria-label="{tool} input"></textarea></div>
    <div class="row">
      <button class="primary" onclick={() => runTool(`${tool}-format`)}>Format</button>
      <button onclick={() => runTool(`${tool}-minify`)}>Minify</button>
      {#if tool === "json"}<button onclick={() => runTool("json-validate")}>Validate</button>{/if}
    </div>
    {#if output}
      <div class="field"><label>Result</label><pre class="tool-output">{output}</pre></div>
    {/if}
    {#if message}<p class="muted">{message}</p>{/if}
    <p class="muted">Formatting runs locally via bundled Prettier; minification runs in the Rust engine.</p>
  {:else if tool === "diff"}
    <div class="field"><label for="diff-before">Before</label><textarea id="diff-before" rows="5" bind:value={diffBefore}></textarea></div>
    <div class="field"><label for="diff-after">After</label><textarea id="diff-after" rows="5" bind:value={diffAfter}></textarea></div>
    <pre class="diff">{#each simpleLineDiff(diffBefore, diffAfter) as [marker, line]}<span class={marker === "+" ? "add" : marker === "-" ? "del" : "ctx"}>{marker} {line}</span>{/each}</pre>
  {:else if tool === "markdown"}
    <div class="field"><textarea rows="8" bind:value={input} aria-label="Markdown input"></textarea></div>
    <button onclick={() => runTool("markdown")}>Render</button>
    {#if output}<div class="tool-output">{@html output}</div>{/if}
  {:else if tool === "palette"}
    <button onclick={async () => {
      const path = await call("pick_file");
      if (!path) return;
      const palette = await call("tool_palette", { sourcePath: path, maxColors: 6 });
      output = "";
      message = palette.map((entry) => `${entry.hex} ${(entry.share * 100).toFixed(0)}%`).join("   ");
    }}>Choose an image…</button>
    {#if message}
      <div class="row">
        {#each message.split("   ") as chunk}
          <span class="color-chip" style="background:{chunk.split(" ")[0]}"></span>
          <span class="muted">{chunk}</span>
        {/each}
      </div>
    {/if}
  {:else if tool === "contrast"}
    <div class="row">
      <label class="inline-label">Text <input type="color" bind:value={fgColor} /></label>
      <label class="inline-label">Background <input type="color" bind:value={bgColor} /></label>
      <span class="muted">contrast {contrastRatio}:1</span>
    </div>
    <div class="tool-output" style="background:{bgColor};color:{fgColor};padding:16px;">Sample text - WCAG AA needs 4.5:1 for body text (3:1 for large text), AAA needs 7:1.</div>
  {:else if tool === "qr"}
    <div class="field"><input placeholder="text or URL" bind:value={input} /></div>
    <button onclick={() => runTool("qr")}>Generate QR (SVG)</button>
    {#if output}<div class="tool-output">{@html output}</div>{/if}
  {:else if tool === "url"}
    <div class="field"><textarea rows="4" bind:value={input} aria-label="URL codec input"></textarea></div>
    <div class="row">
      <button onclick={() => runTool("url-encode")}>Encode</button>
      <button onclick={() => runTool("url-decode")}>Decode</button>
    </div>
    {#if output}<pre class="tool-output">{output}</pre>{/if}
  {:else if tool === "counter"}
    <div class="field"><textarea rows="6" bind:value={input} aria-label="Counter input"></textarea></div>
    <p class="muted">{input.length} characters (with spaces) · {input.replace(/\s/g, "").length} without spaces · {countWords(input)} words · {input.split("\n").length} lines</p>
  {:else if tool === "lorem"}
    <button onclick={() => (output = lorem(3))}>Generate 3 sentences</button>
    {#if output}<pre class="tool-output">{output}</pre>{/if}
  {:else if tool === "og"}
    <div class="field"><label for="og-title">Page title</label><input id="og-title" bind:value={input} placeholder="HostSmith Workshop" /></div>
    <div class="tool-output" style="padding:0;">
      <div style="background:#35393c;height:70px;display:flex;align-items:center;justify-content:center;color:#8b939a;">og:image preview (1200 x 630)</div>
      <div style="padding:8px;"><strong style="color:#d7dbde;">{input || "og:title goes here"}</strong><p class="muted">og:description and the domain render under the image, as on social platforms. This is a local drawing, not a live scraper.</p></div>
    </div>
  {/if}
</div>
