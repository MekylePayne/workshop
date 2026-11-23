# HostSmith Workshop

Local-first Windows desktop application for static website developers:
visual website editing, image and video replacement, SEO inspection,
favicon tools, responsive previews, developer utilities, structure
validation and Cloudflare deployment - built on Tauri 2, Rust and Svelte 5.

**Status: fully implemented engine, tested; Windows installer pending in CI.**
The complete Rust engine (10 crates, 154 passing tests, clippy clean at
`-D warnings`) and the complete Svelte 5 interface (Vite production build
green) are implemented. The Windows compilation/installer gate runs in
`.github/workflows/windows.yml`; this sandbox cannot produce Windows
binaries, and that verification is honestly outstanding.

## Build and run (Windows 10/11)

1. Install the Rust toolchain (stable, MSVC) and Node.js 20+.
2. From the repository root:

```bash
cargo test --workspace                 # engine tests (all 10 crates)
cargo clippy --workspace --all-targets -- -D warnings
cd ui && npm ci && npm run build       # interface -> ui/dist
cd ../src-tauri && cargo check         # shell compiles on the target platform
cargo tauri dev                        # run the desktop app in development
cargo tauri build                      # NSIS installer (needs `cargo install tauri-cli`)
```

3. Or push to GitHub: the CI workflow runs everything above on
   `windows-latest` and produces the bundle on every push.

## Layout

| Path | Purpose |
| --- | --- |
| `crates/hsw-core` | Project scan, safe ZIP import (traversal guard), deterministic naming, byte-range patch model, restart-safe journal, backups, file watcher |
| `crates/hsw-edit` | Streaming rewrite engine (lol_html), CSS reference rewriting, asset import, unified diffs |
| `crates/hsw-server` | Localhost preview server: 127.0.0.1 only, `_headers` support, selection bridge injection, element annotation |
| `crates/hsw-imaging` | Resize, crop, rotate, convert, lossless PNG optimization (oxipng), lossless WebP, honest JPEG handling |
| `crates/hsw-html` | Read-only document queries (scraper/html5ever) for SEO, favicon and validators |
| `crates/hsw-seo` | Check catalog with severities, duplicate detection, broken internal links, auto/guided fix planners |
| `crates/hsw-favicon` | Detection, generation (multi-size ICO, PNGs, manifest), manifest/robots checks |
| `crates/hsw-validators` | HostSmith + generic structure profiles, configuration-driven badge signature (PR-08 pending mode) |
| `crates/hsw-tools` | JSON format/validate, HTML/CSS/JS minification, QR (SVG), palette extraction |
| `crates/hsw-cloudflare` | Native Pages asset-upload flow (upload token, check-missing, upload, upsert-hashes, create deployment), Workers scripts, OS credential store |
| `src-tauri` | Thin Tauri 2 shell: 50+ commands, events, dialogs, per-project state |
| `ui` | Svelte 5 interface: menu bar, tool rail, project browser, responsive preview, panels, diagnostics |
| `fixtures/site` | HostSmith-shaped fixture website used by tests |

## Testing

```bash
cargo test --workspace
```

154 tests across 12 suites, all passing on Linux with Rust stable; the same
suite runs in CI on Windows. Coverage highlights: ZIP traversal/rejection
paths, the streaming rewrite contract (element/text targeting), journal
undo/redo across restarts, external-change invalidation, preview-server
containment (path traversal refused), SEO findings and fix planners, the
Cloudflare deployment protocol against a local mock API (auth split,
manifest shape, bucketing), imaging operations with before/after sizes,
and the validator profiles.

## Honesty notes

- **Windows installer**: not produced here (Linux sandbox, no MSVC). The
  CI `windows` job compiles the Tauri shell and builds the NSIS bundle on
  every push.
- **Footer badge (PR-08)**: the exact markup is still pending from the
  project owner. The validator runs in pending mode (any `hs-` class) and
  Settings accepts the snippet, stores it verbatim and applies it
  byte-for-byte. The application never invents a replacement badge.
- **Galaxy A13 preset (PR-05)**: 360 x 800 CSS pixels, ratio 3.0/2.0 per
  variant, derived from published panel specifications and labelled as an
  estimate in the interface. Physical-device verification is outstanding.
- **Cloudflare**: the native REST upload flow is implemented and verified
  against a mock API reproducing the documented endpoints; a first run
  against the real service needs a scoped token and is the one remaining
  integration check (TL-03 revised).
- **Preview fidelity**: the responsive viewer is emulation (CSS viewport +
  transforms), as documented (TL-05).
