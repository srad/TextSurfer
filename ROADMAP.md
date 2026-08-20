# TextSurf — Roadmap

A terminal text browser in Rust (ratatui). This file is the single source of truth for all work:
architecture decisions, task status, acceptance criteria, test gates, and cross-session handoff.
Update it whenever a decision changes, a task lands, or a flaw is discovered — before writing code
that depends on it.

## Mission (north star)

TextSurf is a terminal-based web browser: the product is the **browser frontend** — the TUI chrome,
keyboard/mouse interaction, and website **rendering** (DOM → style → layout → terminal paint).
Parsing is a means to an end, never in-house craft: every parseable format goes through a mature,
latest-published crate (html5ever, cssparser + selectors, url, encoding_rs, ratatui, boa_engine at
M5). Custom code is reserved for browser behavior — TreeSink glue, cascade/style tree, layout →
terminal grid, painter, chrome/App/event loop — and for parsing only where the ecosystem provably
has no library (audit below). Every hand-rolled parser must be justified against this list before it
is written; the audit is re-run whenever a candidate crate appears.

### Custom-parser audit (2026-08-20)

| Parser | Status | Verdict |
|---|---|---|
| `net/encoding.rs` — WHATWG sniffing: BOM, header, meta prescan, XML fallback, x-user-defined postprocess | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode/encode-only; no sniffer exists in the ecosystem |
| `core/url.rs` — `url_fix` | not a parser | **No change** — delegates all real parsing to the `url` crate; scheme/host/search heuristics are address-bar UX behavior |
| `css/parser.rs` | library adapter | **Keep** — stylesheet/rule/declaration tokenization delegates to cssparser; selector parsing and matching delegate to selectors |
| `css/parser.rs` — type-only media-query grammar/evaluation | custom library adapter | **Keep narrow adapter** — cssparser owns tokens, blocks, delimiters, and recovery; css-mediaquery 0.1.1 is an immature raw-string port without MQ5 grammar/recovery, LightningCSS has no runtime-context evaluator, rdom-tui explicitly excludes `@media`, and Stylo/Blitz/MusKitty/litehtml/Ladybird require replacement DOM/style/rendering stacks |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` fixture format; this stays isolated from production code |

## Status legend

| Marker | Meaning |
|---|---|
| `(open)` | Planned, not started |
| `(in progress)` | Started, not finished |
| `(done)` | Task finished and its tests/gates green |
| `(complete)` | Milestone finished: every task done + acceptance met |
| `(rejected)` | Considered for the design and refused, with reason in Decisions log |
| `(canceled)` | Previously open, dropped |

## Status board

| Milestone | Scope | Status |
|---|---|---|
| M0 — Foundations | Scaffold, traits + contract suites, chrome, I/O-free app, event loop, gates | (complete) |
| M1-A — Parse pipeline | net + encoding + html5ever→arena DOM, `<base>`, scheme routing, tree-dump snapshot | (done — user smoke pending) |
| M1-R — Stabilization | DOM invariants, fetch routing, resource limits, resize/scroll, terminal lifecycle, trustworthy gates | (complete) |
| M1-B — Style, layout, paint | UA cascade, box model, whitespace rules, painter, link list, corpus + goldens + proptest laws | (in progress) |
| M1.5 — Chrome redesign | DOS-style rich UI: menu bar with dropdowns, boxed panels, bordered input field, nav buttons, tab strip, status panel, centralized theme, browser-like startup focus | (complete) |
| M1-C — External styles | Ordered `<link>`/`@import` loading through the per-tab resource scheduler | (open) |
| M2 — Tabs & keyboard | TabManager complete, keyboard link navigation, anchors, `target=_blank`, help overlay, page titles, error pages, start page, in-page search | (open) |
| M3 — Mouse | Full mouse: zones, wheel, link clicks, hover, tab-bar clicks, middle-click new tab, theme pass | (open) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off, pure Rust | (open) |
| M5 — Boa | Boa 0.21.1 behind trait; decision gate Boa vs Deno Core; host bindings subset; job pump | (open) |
| M6 — Stretch | Conformant floats plus Taffy flex/grid, persistence, images, scroll memory, drag, console view, config | (open) |

Cross-cutting: test infrastructure (done: contract suites, snapshots, proptest, fakes) · gates (done:
local only, no CI — see Gates) · coverage floor (open: optional local, 80% overall / 90% css·layout·paint) ·
external conformance corpus (open — see Conformance corpus; the primary driver for M1-A and M5).

## Gates (local only — CI deliberately refused)

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --features js          # M4+; must pass, boa feature compiles
```

First snapshot write: `$env:INSTA_UPDATE = "always"; cargo test`. Coverage (optional):
`cargo llvm-cov --workspace` (needs `rustup component add llvm-tools-preview`; no thresholds enforced).

## Session handoff

1. Read this status board + Roadmap updates log (bottom).
2. Resume the next `(open)`/`(in progress)` milestone.
3. Run the gates before and after; never mark `(done)` with red gates.
4. Manual smoke list (recent — example.com, lite.duckduckgo.com, wikipedia.org) — run by the human
   per milestone close; never part of automated tests.
5. Live repo rule: no commits without explicit user confirmation.

## Architecture (as-built target)

```
 ui (ratatui widgets · Focus · keymap · mouse zones) ──┐
   Action (Load, NewTab, ActivateLink, Scroll, …)     │ UiEvent
   ▼                                                  │
 app — I/O-free composition root · TabManager · event loop · fetch workers · js pump
   │ dispatch                                         ▼
   ▼                                     script: JsEngine trait + JsHost ← per-loaded-document
 net::Fetch ⇒ html::HtmlParser ⇒ core(indextree DOM)   │ noop.rs · boa.rs (feature "js", default off)
   │        │
 css: CssParser(cssparser) → Cascade(selectors) → StyleTree
 layout::LayoutEngine → BoxTree (absolute coords, unbounded height)
 paint::Painter → DisplayList (NodeId hit-tags) → ui content widget
```

- Single crate, modules: `core`, `net`, `html`, `css`, `layout`, `paint`, `script`, `ui`, `app` + thin `main`.
- Cross-module boundaries via traits; only `app`/`main` know the concrete implementations (composition root).
- `app` never imports crossterm: events come in as `core` types, results via `deliver_fetch`, time injected.
- DOM never crosses threads. One thread owns everything except I/O (fetch worker threads only).
- `Document` owns DOM pre-insertion validation and hides indextree; template contents are detached
  document fragments, and DOM removal detaches rather than invalidating node handles.
- No tokio. `boa_engine 0.21.1` optional dep behind feature `js`; runtime `--js=off` overrides the feature.
- Crate pins: report actual versions used in the updates log at each dependency milestone.

## Decisions log

### Locked
- UI design language: classic DOS text-mode rich UI (boxed panels, bordered input field, button
  widgets, an always-visible menu bar with dropdowns, Norton-Commander palette: blue field, light
  blue frame, cyan dim, yellow accent, black-on-white selected states) around a normal browser
  layout shell — decided by the user; `ui::Theme` centralizes all colors (M1.5), the painter and
  every widget draw from it.
- Product = UI and terminal frontend rendering; every other layer adopts a mature, latest-version
  crate: html5ever (HTML parse incl. tree-builder), cssparser + selectors (syntax + selector
  matching), boa_engine (ECMAScript), ureq (HTTP), encoding_rs (decode), ratatui (widgets/TUI).
  In-house scope: Document arena + TreeSink glue, cascade → style tree, layout → terminal grid,
  paint/DisplayList, chrome/App/event loop. External corpora (html5lib-tests, test262) gate the
  adopted layers' integration — conformance is evidence, not reimplementation.
- Single crate with modules (not a workspace) — fastest iteration; workspace split trivial later.
- Native text renderer (lynx/w3m family), not embedded-Engine (carbonyl/browsh family) — our value is
  small footprint + terminal-native layout; ELinks precedent shows JS-in-native is where bugs live.
- CSS box model from day one (`(rejected)`: lynx-style linear flow — user chose box model).
- `cssparser` + `selectors` for CSS (`(rejected)`: lightningcss — no selector matcher, orphan AST vendor).
- `html5ever` for HTML: html5ever's tokenizer + tree-builder with our direct `TreeSink` building an
  `indextree`-backed `Document`; the crate owns links and checked append/prepend, while `Document`
  supplies the stricter DOM pre-insertion rules. `scraper`/ego-tree/RcDom remain rejected because
  none meets the mutable multiple-root, detached-template, future-JS contract.
- Boa as first real JS engine (`(rejected)`: rquickjs — C toolchain / unsafe FFI; both sealed by trait).
- `ureq` (blocking, rustls native roots) behind `Fetch`; four fixed workers use crossbeam channels,
  the app cancels superseded pending jobs per tab, and both HTTP/file bodies share a 10 MiB limit.
  `mediatype` parses response metadata. Browser-ish UA + override flag.
- UA stylesheet ships as CSS text in `css/ua.rs` (doubles as parser+cascade integration test).
- Per-load pivot invariant: any navigation ⇒ `generation++`, fresh `Document` + fresh `JsEngine`
  (globals never survive), scroll reset to top, stale/gen-tagged fetch results dropped.
- JS host mutation funnel: all DOM changes via a single `MutateOp` enum ⇒ one invalidation path.
- Contract suites are capability-parameterized in the trace
  (`Capabilities { executes_scripts, async_host_ops }`) — Noop passes the inert set, Boa must pass the full set.
- Pressure ceilings: JS job pump (≤256 steps/tick), re-layout only on content-width change, scroll clamp
  after every re-layout, errors surface in status bar; terminal/backend I/O errors are an exit path.
- No CI anywhere (`(canceled)` by user: "leave out anything CI related"). Local gates only.

### Deferred — decision gates with explicit triggers
- M5 gate: if Boa's async (fetch promises / timers / top-level await) can't keep the UI responsive after
  contract suite + fixture, switch to Deno Core (V8; async-native ops; heavier build) — same `JsEngine` trait.
- M1-B adopts Taffy for block box calculation now. Taffy types stay behind `LayoutEngine`; terminal
  inline formatting uses textwrap fragments + Unicode cell/grapheme libraries because Taffy has no
  inline layout. M6 enables Taffy flex/grid; floats wait for line-flow-around-float conformance.
- Basic forms move to M2 so the DuckDuckGo form smoke is real. `addEventListener`/console/config stay
  deferred.

## Milestones

### M0 — Foundations (complete)

Traits exist with capability-parameterized contract suites running against fakes; app is I/O-free;
chrome renders and is snapshot-tested; event loop runs; all gates green.

- [x] `cargo init` (no git), Cargo.toml with initial pins: ratatui, url, slotmap, unicode-width, thiserror;
      optional boa_engine behind `js`; dev: insta, proptest, pretty_assertions. Edition 2024. (done)
- [x] `core`: `focus` (Focus), `geom` (Size/Point), `url` (Scheme + `url_fix` rules), `dom`
      (initial slotmap arena, replaced by indextree in M1-R), `style` (initial placeholder, expanded
      in M1-B). (done)
- [x] Traits: `net::Fetch`, `html::HtmlParser` (parse_document + fragment w/ context element),
      `css::CssParser` + `css::Cascade`, `layout::LayoutEngine`, `script::JsEngine` +
      `script::JsHost`; only actual cross-thread boundaries retain `Send + Sync` after M1-R. (done)
- [x] `script::Capabilities` + contract suite (capability-parameterized) run against `NoopEngine`;
      FakeHost records calls. (done)
- [x] `ui` widgets: TabBar (truncate + overflow), Address bar (`EditBuffer`, upgraded to grapheme
      indexing and single-string storage in M1-R),
      Content (lines + scroll clip), Status (url/message). TestBackend + insta snapshots. (done)
- [x] `ui::keymap`: Action enum + `Keymap` trait + DefaultKeymap; focus-aware routing (printables in
      Address go to the buffer exclusively; `q` never quits while typing). (done)
- [x] `ui::mouse`: zone model + injected `ChromeGeometry`; Menu/Tabs/Address/Content with the status
      bar outside interactive zones; pure tests. (done) — capture/events wiring deferred to M3.
- [x] `app`: TabManager (open/close/cycle, close-last→fresh tab, history dedup, generation, scroll,
      layout-width cache), I/O-free `App` (handle_key/on_resize/step(now)/deliver_fetch/should_quit/
      dirty flag), start-page placeholder content, status messages for M1-bound actions. (done)
- [x] `main`: poll(50ms) loop, draw-if-dirty, resize, terminal-error exit path, `--url` arg accepted;
      M1-R replaced manual terminal setup with ratatui lifecycle management and clap. (done)
- [x] Gates green: fmt, clippy -D warnings, cargo test, test --features js. (done)
- Acceptance M0: `cargo run` draws tabs/address/content/status, q quits, keys don't panic. (complete)

### M1-A — Parse pipeline (done — user smoke pending)

- [x] Add deps: html5ever 0.39.0, encoding_rs 0.8.35, ureq 3.4.0 (feature `platform-verifier` =
      OS-native roots; fallback: default webpki-roots if the Windows build fights). Record pins in
      updates log. No `tendril`/`markup5ever` in the manifest — both re-exported by `html5ever`. (done)
- [x] Encoding seam: BOM → header charset → meta-charset scan (first 1024 bytes) → UTF-8 default.
      Fixture: latin-1 page decodes correctly. (done)
- [x] `UreqFetch` + `FileFetch` (+fs) behind `Fetch`; fixed 4-worker pool + timeouts + UA;
      redirects → final URL returned in `Response`; non-2xx → error outcome (error page rendered later).
      `FetchPayload` gains `result: Result<FetchResponse, FetchError>`; generation check precedes
      decode/parse in the app. M1-R replaced loopback coverage with synthetic ureq middleware. (done)
- [x] `Html5everParser`: html5ever's tokenizer + tree-builder with our direct `TreeSink` into the
      `Document` boundary (indextree since M1-R); `<base href>` read first; scripting flag = session JS enablement, fixed
      at parse time. `Node` grows `Comment`/`Pi`/`Doctype` + element/attr namespaces (svg/math/
      xlink/xml/xmlns dump prefixes) + move/reparent ops (foster parenting). Template contents
      use detached fragments since M1-R; the dump emits the `content` pseudo-line for `<template>`.
      Parse fixtures for
      adoption agency, foster parenting, template, svg/math foreign, PI, entities, doctype variants,
      duplicate attrs, base-href (incl. template skip) — 24 tests, insta snapshots.
- [x] `SchemeRegistry` in app: http/https→Fetch, file→Fetch, about:blank→start page, unknown→status
      error. (done)
- [x] `url_fix` hot-wired: load from address bar works for tests via FakeFetch; generation tagging
      done. (done)
- [x] Tree-dump landing gate: fixture pages pretty-print to insta snapshots (no styling yet).
      (done)
- [x] Corpus harness: `tools/fetch-corpus.ps1` (human-run) pins + vendors `*.dat` from WPT
      `html/syntax/parsing/resources/` (the html5lib-tests repo is archived — its README points
      here; tokenizer `.test` files don't exist there and are inherited upstream anyway) into
      `testdata/`; tests never touch the network. `.dat` payload is raw (footer newline removed);
      `#errors`/`#new-errors` counted but ignored; `#script-on`-only cases skipped; dual-mode tests
      run script-off only (session scripting off until M4). Runner feeds each case to our parser,
      serializes our `Document` in the html5lib format, honors an explicit xfail manifest (exact
      test id + reason). Landing gate: ≥90% of run cases green, zero regressions per milestone,
pass rate in the updates log. (done — first run: 1829/1922 raw = 95.16%, 100% with xfail,
      0 panics.)
- [x] Contract suite for FakeFetch hardness (in-crate): hung job doesn't block others; concurrency
      cap measured ≤ pool size; out-of-order responses dropped by stale generation. (done)
- Acceptance: corpus fixtures parse to expected DOM snapshots; manual: `cargo run -- --url https://example.com`
  shows the parsed tree in the content pane (debug-tree view); gates green — all green (fmt,
  clippy -D warnings, 157 lib + 3 pipeline integration + 12 corpus, `--features js`). The
  plumbing is covered hermetically by `tests/fetch_pipeline.rs` (real composition root over
  loopback); the TUI binary also ran the real network without panic. Final eyeball step: the
  smoke run above in a real terminal; milestone done on user sign-off.

### M1-R — Stabilization (complete)

- [x] Replace custom DOM relationship bookkeeping with indextree behind the existing `Document`
      boundary; add detached template fragments, quirks mode, checked DOM pre-insertion, document-
      order duplicate-ID lookup, and iterative traversals. DOM removal preserves detached handles.
- [x] Route fetch completions by stable tab ID + generation rather than active-tab position; make
      status/load state tab-local and skip superseded pending jobs through crossbeam channels.
- [x] Enforce 10 MiB HTTP/file response limits; parse media types/quoted charsets with mediatype;
      render plain text explicitly and reject unsupported valid media types in content.
- [x] Initialize from the real terminal size; use `usize` scroll/extents; invalidate on width only,
      clamp after height/re-layout, and centralize chrome draw/hit/cursor geometry.
- [x] Use ratatui's managed terminal lifecycle and clap CLI (`--url`, `--user-agent`,
      `--js=auto|on|off`); remove JS/host thread bounds because Boa Context is app-thread-owned.
- [x] Repair modifier/address/tab overflow behavior, grapheme-safe editing, menu focus restoration,
      and allocation-heavy chrome view cloning.
- [x] Corpus floor counts raw passes only; manifest membership/hashes are mandatory; xfails require
      reasons and never hide panics. Replace loopback/sleep tests with synthetic ureq middleware,
      channels, fake time, tempfile, and sha2.
- Acceptance: all targeted regression tests and every local gate green; `git diff --check` ignores
  only terminal-significant snapshot end spaces; dependency audit has no unrecorded advisory.

### M1-B — Style, layout, paint (in progress)

- [x] `cssparser` 0.37 + `selectors` 0.40 (pin per selectors' range) + own `Atom` conversions;
      selector adapter supports structural matching and specificity without exposing DOM storage.
- [x] CssParser impl + first Cascade impl: UA defaults + embedded author rules + inline `style`
      attr + selector specificity/source order/`!important`; visible HTML now runs through
      parse→cascade→layout→paint rather than the M1-A debug tree dump.
- [x] First `layout`/`paint` slice: Taffy 0.13 owns vertical block placement; textwrap 0.16.2 and
      Unicode grapheme/cell crates own wrapping and painting; `display:none`, block/inline text,
      headings, lists, `<pre>`, simple margins/padding/borders, and link discovery are covered.
- [x] Complete conditional CSS and cascade degradation: type-only `@media` with an injected screen
      context; `@import` ignored+diagnosed; table/inline-block/flex/grid→block, position→static,
      percentage heights→auto, and overflow-wrap break-word enforced. (done)
- [x] Complete the box model: Taffy block/content-size engine stays behind `LayoutEngine`; add CSS
      anonymous block boxes and node-owned textwrap fragments for inline cell layout; support fixed
      author widths plus `content-box`/`border-box` through Taffy while keeping height intrinsic and
      unbounded; model all six supported `white-space` modes with inheritance, normalize segment
      breaks and tab stops, ignore vertical inline edges, and reflow against the final content width.
      Layout emits geometry and text fragments only; paint owns border glyphs and allocates touched
      rows lazily. Individual CSS cell lengths are capped at 65,535. (done)
- [ ] Complete paint: depth-order (bg bottom-up, borders box-drawing ≥2 cells doubled, text clipped);
      DisplayList with NodeId hit-tags; interactive-element list (`<a>`).
- [ ] Corpus fixtures + golden screen snapshots (margins, headings, borders, links, wide chars, `pre`).
      Width monotonicity and painted-row bounds properties are green; laminar-box and engine-backed
      deepest-hit round trips remain before this item can close.
- [ ] Proptest layout laws: text-glyph cells of leaf runs disjoint · boxes laminar per row ·
      hit-test round-trip returns deepest box · widths monotone in viewport · scroll clamp fixed point.
- Conformance note: css-syntax + WPT-selectors corpora are inherited — they are the upstream test
  suites of the `cssparser` 0.37 / `selectors` 0.40 crates themselves. No external corpus exists
  for terminal-grid layout (WPT layout tests need a pixel browser + orchestration); our gate stays
  corpus goldens + proptest laws.
- Acceptance: fixture corpus goldens green; laws green; manual example.com smoke good enough to scroll.

### M1-C — External stylesheets (open)

- [ ] Discover `<link rel=stylesheet>` and recursive `@import` URLs against the effective document
      base; use the same tab/generation scheduler as navigation.
- [ ] Preserve cascade document order independently of completion order; recascade when a resource
      slot changes and lazily re-layout background tabs on activation.
- [ ] Per-load ceilings: 64 subresource requests, 32 MiB aggregate decoded bytes, 10 MiB per
      resource, import depth 8; cycles deduplicate and failures remain tab-local/non-fatal.
- Acceptance: out-of-order, stale-generation, redirect/base, import-cycle, budget, and cascade-order
  fixtures green; manual Wikipedia smoke renders with external author styles.

### M1.5 — Chrome redesign: DOS-style rich UI (complete)

Design language: the classic text-mode "rich UI" (Turbo Vision / Norton Commander / MS-DOS shell):
box-drawing outlines around every chrome region, bordered input fields, button widgets for
navigation, reversed/inverse active states, one accent color over a neutral field. The chrome
becomes a normal browser layout shell: tab strip · toolbar (nav buttons + address field) · content
panel · status panel — each an outlined panel.

- [x] `ui::theme` (new): the centralized `Theme` struct pulled forward from M3's theme task — DOS
      palette (default field background, bright accent, neutral text), every widget and the painter
      consume it, no color literals outside it. (done)
- [x] Boxed panels: outer outline wraps the whole chrome; content and status get their own borders;
      panels render with correct corner/edge chars at any size, degraded safely on tiny terminals.
      (done)
- [x] Toolbar nav buttons: Back / Forward / Reload / Home as bordered button widgets (active =
      reversed), wired to actions — Back/Forward against each tab's history stack (history cursor
      added to Tab), Reload = the per-load pivot, Home = `about:blank`; disabled states dim at the
      stack ends. (done)
- [x] Address input as a bordered, labelled field (`URL:`); cursor math updated for the new inset;
      the whole field reverses when focused. (done)
- [x] Tab strip as raised NC-style boxes (`┌ title ┐` joined by `│`) with an open bottom under the
      active box — the shared `layout_tabs` walker feeds `active_span`, so the drawn boxes and the
      divider gap can never drift; titles capped at 24 cells with ellipsis/`…»` clipping. (done)
- [x] Status/bottom panel: boxed, message left + URL right (existing arrangement inside the box).
      (done)
- [x] `ChromeGeometry`/mouse zones preserved and remapped: row/col budget accounts for the new
      borders so content rows/width match the visible panel interior; `layout_width` cache now keys
      off `content_cols()`. (done)
- [x] Norton-Commander palette: `Theme` gains `bg`; the scheme becomes `ui::theme::NORTON` (light
      blue frame, white text, cyan dim, yellow accent reserved for links/hover, blue field); active
      states are explicit black-on-white `Theme::selected()` (REVERSED over an unset background is
      terminal-dependent); every band fills from the theme background. (done)
- [x] Browser-like startup: the app opens focused on the `URL:` field with a "type a URL and press
      Enter" hint instead of sitting in content focus. (done)
- [x] Always-visible interactive menu bar (File / Navigate / View / Help): dropdowns composed from
      ratatui `List` + `Block` + `Clear` (ratatui 0.30 has no menu widget — inventory-verified),
      drawn last as an overlay; F10 toggles, Alt+letter opens a specific menu, arrows/Enter/Esc
      drive selection, items dispatch the existing actions (File→NewTab/CloseTab/Reload/Quit,
      Navigate→Back/Forward/Home, View→theme info, Help→M2 overlay); the chrome gains one band
      (`CHROME_ROWS` 8→10, `MouseZone::Menu`); mouse wiring deferred to M3. (done)
- [x] QBasic/QuickBasic restyle (user, eyeball follow-up): the top and bottom lines stop being frame
      outlines; row 0 becomes a full-width grey menu bar (black text, first letter of each title in
      red — honest `Alt+F/N/V/H` mnemonics) and the bottom-most row a full-width grey context bar
      (message left, URL right, black text) that absorbs the old boxed status panel; the blue field
      darkens to 0x000080. Popup dropdowns match: grey field with red first letters on the items
      (decorative — item letters are not bound) and the opening one row lower, flush under the menu
      bar. `CHROME_ROWS` 10→6 (content rows−6), top and bottom frame glyph rows gone, and the divider
      lines under the menu bar and above the bottom bar removed — the tabs strip sits flush under
      the menu, the content window reaches the bar; address cursor y=5→3; mouse zones unchanged in
      shape (Menu row 0, Tabs 1–2, Address 3–4, Content 5+). `Theme`
      gains `bar_bg`/`bar_text`/`mnemonic`; `bg` is now RGB. (done)
- [x] Snapshots: every widget and the full-chrome goldens updated to the new design via
      `TestBackend`; replaced M0 chrome snapshots reviewed before deletion. (done)
- Acceptance: healthy chrome rows ≈ rows−8 with an outer frame and open panel interiors. Eyeball
  signed off; the QBasic restyle above is the as-built look. Scripted chrome tests green; all
  gates green.

### M2 — Tabs & keyboard navigation (open)

- [ ] Keymap unification (extends M0 keymap tests, same file): `Ctrl+L` (+ existing `a`) focuses the
      address bar so `/` is freed; `/` becomes in-page search; `Tab` in the address bar moves focus
      to content (currently routed to NextLink); new `FocusTabs` action + binding (`F6`) — the tab
      bar currently has no keyboard entry (M0 has only FocusAddress/FocusContent). Back/Forward/
      Reload/Home + Alt bindings already shipped in M1.5.
- [ ] `?` help overlay renders from the keymap definition (single source of truth), snapshot-tested.
- [ ] Keyboard link navigation: Tab/Shift+Tab + Enter (actions exist since M0) drive the
      interactive-element list; focused link highlighted, re-paint only on target change (shared
      discipline with the M3 hover, one invalidation path); per-tab isolation; keymap + snapshot tests.
- [ ] TabManager completion: Ctrl+T/W/N/P cycle + close-last→fresh (exist since M0); page titles in
      the TabBar — per-tab label from `document.title`, fallback host/URL; in-flight loads show the URL.
- [ ] Anchor links: `#fragment` → scroll-to-box + status line; no URL rewrite.
- [ ] `target="_blank"` links → new tab; per-tab history dedup (exists since M0) verified by tests.
- [x] Reload (key `R`): generation++, fresh engine+document, scroll top (per-load pivot). (done — M1.5)
- [ ] In-page search: `/` opens a prompt (reuses EditBuffer), `n`/`N` next/prev (plain — free: tab
      cycle is Ctrl+N/P) with scroll-into-view, match highlight distinct from the link-focus
      highlight, `x/y` match counter in the status line, `Esc`/`Enter` closes.
- [ ] Rendered error pages: DNS/fetch/non-2xx/unknown-scheme failures paint a readable in-content
      error screen + status-line message (replaces bare status text).
- [ ] Designed start page: `about:blank` becomes a real landing with keymap reference and usage
      hints instead of placeholder text.
- [ ] Basic forms: text/search/hidden/submit, textarea, select, checkbox, and radio controls;
      GET and `application/x-www-form-urlencoded` POST serialized with `url::form_urlencoded`;
      unsupported methods/encodings render a controlled error.
- Acceptance: scripted-drive checklist of every keybinding incl. the rebinds; per-tab state
  isolation tests; chrome items (titles, error pages, start page, search, help overlay)
  snapshot-tested; M0 keymap tests updated, not replaced; manual DuckDuckGo Lite submission works.

### M3 — Mouse (open)

- [ ] crossterm mouse capture; zones wired to `ChromeGeometry` (Menu/Tabs/Address/Content only,
      wheel acts in content, clicks set focus, tab-bar clicks switch, middle-click/`target=_blank`
      → new tab).
- [ ] Menu-bar mouse: title clicks open/drive the dropdowns (keyboard paths ship in M1.5), item
      clicks dispatch, hover tracking; the `MouseZone::Menu` band powers hit-testing.
- [ ] Link hover highlight (re-paint on hover-target change only — shared discipline with the M2
      keyboard link highlight, one invalidation path) + status bar URL preview.
- [ ] Hit-test resolution contract: targets resolved by NodeId against live document at dispatch time.
- [ ] Theme extension (the M1.5 `Theme` grows hover/selected states): default scheme + mouse-driven
      focus/hover/selected/link/search-match states with their own accents. Snapshot goldens.
- Acceptance: scripted zone tests + goldens with hover states and theme variants; manual mouse
  walkthrough.

### M4 — JS seam (open)

- [ ] `JsEngine` + `js` feature wiring in composition root; runtime `--js=off` wins over feature.
- [ ] `MutateOp` funnel + invalidation-once rule; host subset: document, location, console→status buffer,
      alert→dialog line; no dispatch except `onclick` handlers.
- [ ] Noop contract suite institue the inert set; app byte-identical behavior compiled-off vs on-but-off.
- Acceptance: `--js=off` and no-js builds pass identical integration suites.

### M5 — Boa (open)

- [ ] Decision gate: Boa 0.21.1 vs Deno Core on async responsiveness fixture (see Decisions log).
- [ ] `BoaEngine` behind trait; engine instance per loaded document; job pump per tick (≤256),
      injected clock; fetch/timer promises resolved from job queue; TLA out of scope (classic scripts).
- [ ] Full-suite contract run (script feature); noscript fixture; globals-isolation test across documents.
- [ ] test262 subset runner: pinned checkout under `testdata/test262` (via `tools/fetch-corpus.ps1`);
      harness files `assert.js` + `sta.js` (+ `doneprintHandle.js` for async); YAML frontmatter
      parsed (`negative` phase/type, `includes`, `flags`, `features`); curated per-milestone slices
      starting with language core + basic built-ins; xfail manifest by feature, regressions
      forbidden. Upstream reference: Boa 0.21.1 ≈ 94.12% (boajs.dev/conformance); our slice must
      stay within a documented delta.
- Acceptance: fixture page (inline script + onclick + document.title + console echo) green; no crashes
  on example.com with JS on; gates green with `--features js`.

### M6 — Stretch (open)

Conformant float flow plus Taffy flex/grid · persistence/backup · images (`[img]`→alt→Kitty
protocol later) · per-history-entry scroll memory · drag input · console
view (F12) · config file · iframes/frameset/bidi remain non-goals beyond `. . .`.

## Test infrastructure (standing)

- Contract suites, capability-parameterized, defined in M0, run against every impl (M1-A fakes …
  M5 Boa) — an interface is defined by its tests.
- insta snapshots of ratatui `TestBackend` buffers (widget goldens), corpus goldens (M1-B),
  DOM tree dumps (M1-A).
- proptest layout laws (M1-B) — width monotonicity and painted-row bounds landed; laminar boxes and
  engine-backed deepest-hit round trips remain. Property tests for url_fix/EditBuffer continue from M0.
- FakeFetch + fake clock + fake Host; no test touches the network or the real clock.
- `tests/common/` shared fakes; inline `#[cfg(test)]` fakes where module-local.
- Perf smoke guard (M6 gate): largest corpus page layout+paint < 200ms debug.

### External conformance corpus (M1-A / M5)

- WPT `html/syntax/parsing/resources/*.dat` (pinned commit; the html5lib-tests repo is archived and
  points here) is the M1-A landing gate; the tokenizer suite is informational (inherited from
  html5ever upstream; it lives only in the archived repo, so it is never vendored). Compared
  against a serializer that walks our indextree-backed `Document` boundary.
- test262 (pinned commit): executed through `boa_engine` 0.21.1; harness files + YAML frontmatter
  honored; per-milestone curated slices with an explicit xfail manifest.
- css-syntax + WPT-selectors corpora: inherited by adopting `cssparser` / `selectors` — their
  upstream suites are the conformance evidence; terminal-grid layout has no external suite.
- Rules: corpora live in `testdata/`, fetched once by `tools/fetch-corpus.ps1` (human-run, pinned
  via commit hashes); tests never touch the network; xfail manifest entries name the exact test
  file + reason; pass-rate progression is recorded in the updates log at each milestone.

## Non-goals (locked unless a milestone re-opens them)

Text selection/copy, iframes/`<frame>`, bidi/RTL/writing modes, `line-height`/fonts, border-radius,
inline-element borders, cookies, `addEventListener` DOM events (click-only v0),
top-level await, full CSS/DOM, window-title setting, config files pre-M6, drag input pre-M6.

## Roadmap updates log

Log of decisions, pins, and plan changes only — task status lives in the plan markers above.

- 2026-08-20 — M0 scope accepted; delivered complete (gates green; rustc 1.97.1, edition 2024).
- 2026-08-20 — Library-first directive: adopt the latest ready-made crate per layer (html5ever,
  cssparser, selectors, boa_engine, ureq, encoding_rs, ratatui); rcdom rejected; in-house focus is
  the UI and terminal frontend. Conformance gates: WPT parsing corpus (M1-A), test262 slice (M5);
  css-syntax + selectors corpora inherited.
- 2026-08-20 — TDD process formalized: contract suite first, gates per item.
- 2026-08-20 — Corpus source pinned: html5lib-tests repo is archived; tests moved to WPT
  `html/syntax/parsing/resources/` (vendored at commit `ed37f83e`). Serializer must match the
  reference `etree.py` format. Script-off until M4; PASS_FLOOR 0.90 with an explicit xfail manifest
  and zero regressions per milestone. Deps pinned: html5ever 0.39.0 (no direct tendril/markup5ever),
  encoding_rs 0.8.35, ureq 3.4.0 with `platform-verifier` (fallback webpki-roots). Fetch = fixed
  4-worker pool; pool + fetcher contract suites live in-crate.
- 2026-08-20 — Mission directive (user): the product is the terminal browser frontend + site
  rendering; custom parsers must be replaced by libraries where one exists (audit table in the
  Mission section; encoding sniffing stays custom — html5ever's extractor is `pub(crate)`).
- 2026-08-20 — xfail decisions for M1-A: 88 PI dumps unreachable (HTML tokenizer yields bogus
  comments), `tests_innerHTML_1#76` spec-undefined input-in-select fragment, `webkit02#45–48`
  `<selectedcontent>` postdates html5ever 0.39.0.
- 2026-08-20 — M1-A shipped: all gates green (157 lib + 3 pipeline integration + 12 corpus,
  js-feature); composition root verified end-to-end over loopback; network + TUI smoke pending user
  sign-off per the acceptance line. Corpus progression: 1829/1922 raw (95.16%) → 100% with xfail.
- 2026-08-20 — M1-A property-law correction: `url_fix` whitespace-only → `""` is the documented
  contract; two M0 laws gained the matching assumption.
- 2026-08-20 — Updates log trimmed to decisions/pins/plan changes only (this entry).
- 2026-08-20 — M2/M3 scope widened (user): chrome & usability folded in — TabBar page titles,
  rendered error pages, designed start page, in-page search, theme pass; console view/config/drag
  stay in M6. Plan review against `src/ui/keymap.rs` fixed M2:
    - `/` is already FocusAddress — rebind: `/` → search, address focus gains `Ctrl+L` (`a` kept)
    - `Tab` in the address bar currently routes to NextLink — becomes FocusContent
    - the tab bar has no keyboard entry — new `FocusTabs` action (`F6`)
    - `R` reload and raw `n`/`N` (search) are unbound — free (tab cycle is Ctrl+N/P, no clash)
  M0 keymap tests to be extended, not replaced. Theme moved to a single `ui::Theme` with a
  zero-literal rule.
- 2026-08-20 — New milestone M1.5 (user): chrome redesigned as a DOS-style rich UI — box-drawing
  outlines, bordered `URL:` field, Back/Forward/Reload/Home button widgets, tab strip and status
  panel as boxed regions, `ui::Theme` pulled forward from M3's theme task (M3's bullet now extends
  it with mouse states). Normal browser layout shell; geometry/zone remapping and snapshot
  replacement are part of the milestone. Can proceed in parallel with M1-B (separate modules:
  `ui` vs `layout`/`paint`).
- 2026-08-20 — M1.5 shipped: frame + divider chrome (chrome rows = rows−8, interior = cols−2),
  `ui::theme::DOS` with a no-literal rule, `[‹][›][↻][⌂]` buttons with history-driven disabled
  states, `URL:`-labelled railed field (reversed when focused), single NC-style frame. History:
  `Tab.history_pos` cursor with forward-stack truncation; history records at load time (the
  requested URL) — `deliver_fetch` no longer touches history; back/forward/reload/home navigate
  in place (Current mode) through the per-load pivot, submits stay NewTab (M0 semantics). Navigation
  action bindings: Alt+Left/Right, R, Alt+Home — suspended while the address field is focused;
  `layout_width` now set from `content_cols()` (M1-B hook). 175 lib + 3 pipeline + 12 corpus tests
  green; fmt/clippy/js gates green; superseded M0 chrome snapshots deleted after review. Remaining
  for sign-off: user eyeball run in a real terminal.
- 2026-08-20 — M1.5 chrome follow-up (user): three additions shipped. (1) Norton-Commander palette:
  `Theme` gains `bg`, scheme becomes `ui::theme::NORTON` (light-blue frame on a blue field, white
  text, cyan dim, yellow accent reserved for links/hover), explicit black-on-white
  `Theme::selected()` replaces terminal-dependent REVERSED; every band fills from the theme
  background. (2) Browser-like startup: the app opens focused on the `URL:` field with a typing
  hint. (3) Always-visible interactive menu bar (File/Navigate/View/Help) with dropdowns composed
  from ratatui `List` + `Block` + `Clear` — ratatui 0.30 has no menu widget (inventory-verified);
  F10/Alt+letter open, arrows/Enter/Esc drive, items dispatch existing actions, mouse deferred to
  M3. Chrome gains one band: `CHROME_ROWS` 8→10, `MouseZone::Menu`, content 14 rows @80×24.
  Startup-focus change: content-driven tests lead with `Esc`; the `q`-quits test split. 191 lib +
  3 pipeline + 12 corpus green; fmt/clippy/js gates green; goldens reviewed individually.
- 2026-08-20 — M1.5 tab strip restored and re-skinned (user): the "start" row (dropped when tabs
  were moved to the address bar) was actually the tab strip — brought back at row 3 as raised
  NC-style boxes (`┌ title ┐` joined by `│` separators) with an open bottom under the active box:
  divider row 4 stops at the active box's wall columns (`┘` blank pocket `└`, knife edges from the
  shared `layout_tabs` walker via `active_span`, so the drawn boxes and the divider gap can never
  drift). Titles capped at `MAX_TAB_TITLE = 24` cells (ellipsis; overflowing strip boxes clip with
  `…»` and report the partial span). Chrome pitch restored: bands 0 frame·1 menu·2 div·3 tabs·4
  div·5 toolbar·6 div·7+ content (rows−10), bottom trio −3/−2/−1; `CHROME_ROWS` 8→10,
  `MouseZone::Tabs` 2→3, collapse guards per band, cursor y=5. 194 lib + 3 pipeline + 12 corpus
  green; fmt/clippy/js gates green; goldens reviewed individually (old flat-chip snapshot deleted).
  Page titles from M2 (M3 widen) inherit the strip; chrome glyph backgrounds (frame strokes sit on
  the terminal default bg, not the theme) flagged for the user-eyeball sign-off.
- 2026-08-20 — Windows duplicate-input fix (user, first eyeball run: one tap typed two chars): on
  Windows crossterm always reports `KeyEventKind` and delivers both a Press and a Release event per
  physical key; the input boundary in `main.rs` ignored `kind`, so key-up inserted/acted a second
  time (chars doubled, F10/Alt-letter menus double-toggled). `from_terminal_key` now returns
  `Option`; it initially mapped only Press, then M1-R restored normal key repeat by accepting
  Press and Repeat while dropping Release. Boundary unit tests cover all three kinds; M3 mouse
  handling must apply the equivalent release-event gate. fmt/clippy/test/js gates green.
- 2026-08-20 — QBasic-style restyle (user, eyeball of the M1.5 chrome): first line is no longer a
  frame outline — it is a full-width grey menu bar (grey `0xAAAAAA`, black text, first letter of
  each title red, matching the real `Alt+F/N/V/H` bindings); bottom-most line is likewise grey and
  outside the frame, showing context (message left, URL right) and absorbing the boxed status panel,
  so chrome rows drop to 6 (content rows−6 again) and the address cursor moves to y=3; blue field
  darkened to rgb(0,0,128) — `Theme::bg` is an RGB value, no `Color::DarkBlue` variant exists in
  ratatui; popup dropdowns grey with red first letters (decorative, not bound) and open flush under
  the menu bar (y+1). Follow-up (same session): the divider lines below the menu bar and above the
  bottom bar are removed as space-wasters — tabs strip flush under the menu, content reaches the
  bar; the tabs' open-bottom divider and the one above content stay. `MouseZone` mapping reshaped to
  Menu row 0, Tabs 1–2, Address 3–4, Content 5+. `Theme` gains `bar_bg`/`bar_text`/`mnemonic`. M1.5
  marked (complete); 194 lib + 3 pipeline + 12 corpus green; fmt/clippy/test/js gates green;
  chrome/menu/status goldens reviewed individually.
- 2026-08-20 — Full repository audit accepted for implementation. M1-R added before rendering;
  M1-C external styles split from stretch; forms moved to M2. Current audit pins to refresh at use:
  indextree 4.8.1, mediatype 0.23.0, crossbeam-channel 0.5.16, clap 4.6.6,
  unicode-segmentation 1.13.3, Taffy 0.13.0, textwrap 0.16.2, tempfile 3.27.0, sha2 0.11.0.
  Taffy owns block calculations, not inline formatting; flex/grid and conformant floats remain M6.
- 2026-08-20 — M1-R complete and M1-B started. Stabilization shipped the indextree DOM boundary,
  stable tab-ID/generation delivery, fixed/joined crossbeam workers, 10 MiB HTTP/file bodies,
  mediatype response classification, usize scroll/reflow, managed ratatui lifecycle, clap CLI,
  grapheme-safe single-string editing, borrowed redraw views, centralized chrome geometry, and
  deterministic no-network/no-clock tests. The first rendering slice pins cssparser 0.37.0,
  selectors 0.40.0, Taffy 0.13.0, textwrap 0.16.2, and unicode-segmentation 1.13.3 and connects
  embedded/inline CSS through cascade, block layout, and Unicode-cell paint. RustSec audit: zero
  vulnerabilities; one recorded unmaintained transitive warning, `paste` 1.0.15, is reachable only
  through optional Boa 0.21.1 and has no semver-compatible maintained replacement in that release.
  Full default/`js` tests and default/all-feature strict clippy gates green; terminal smoke remains
  human-run.
- 2026-08-20 — M1-B property-law progress recorded: viewport-width monotonicity and painted-row
  bounds are green; laminar per-row box families and engine-backed deepest-hit round trips remain
  milestone completion gates.
- 2026-08-20 — M1-B conditional-CSS plan corrected before implementation: the current slice owns
  type-only `@media`, bounded structured diagnostics, ignored/unfetched `@import`, and explicit
  degradation contracts; `box-sizing` moved to the hierarchical box-model slice where Taffy 0.13.0
  can apply it. Registry/source audit retained cssparser 0.37.0 + selectors 0.40.0 and rejected
  css-mediaquery 0.1.1, LightningCSS 1.0.0-alpha.72, Stylo/Blitz/MusKitty, rdom-tui, litehtml,
  and Ladybird as either incomplete media evaluators or incompatible whole-engine stacks. Ladybird's
  anonymous block construction is reference material only; no dependency or copied code.
- 2026-08-20 — M1-B conditional CSS and cascade degradation completed: recursive media-rule groups,
  explicit screen/print evaluation, fail-closed unsupported queries, ignored/unfetched imports,
  bounded structured diagnostics with exact aggregate counts, nested source order, app warning
  reporting, fallback display/position/height behavior, and deliberate long-word splitting are
  covered by contracts. No dependency changes; default and `js` strict gates are green.
- 2026-08-21 — M1-B box-model outline corrected before implementation: `box-sizing` now has the
  required fixed author-width input; author height remains intrinsic to preserve unbounded document
  flow; the two-state whitespace shorthand is replaced by the six accepted CSS modes with
  inheritance; node-owned fragments replace glyph-bearing layout lines so border drawing remains a
  paint responsibility; lengths are capped at 65,535 cells and paint uses sparse touched rows.
  Registry refresh retained Taffy 0.13.0, textwrap 0.16.2, cssparser 0.37.0,
  unicode-segmentation 1.13.3, and unicode-width 0.2.2.
- 2026-08-21 — M1-B box model completed: iterative formatting-tree construction now preserves
  nested block hierarchy and creates private anonymous runs around mixed inline/block content;
  Taffy owns intrinsic block geometry, margin collapse, padding, one-cell borders, fixed widths,
  and content-box/border-box sizing. Layout exports absolute border/content rectangles plus
  text-node-owned Unicode fragments; paint owns border glyphs and allocates cell buffers only for
  touched rows. The six accepted whitespace modes inherit and cover collapse/trim, hard breaks,
  wrapping, 8-cell tabs, segment-break normalization, `<br>`, cross-node graphemes, zero-width
  progress, and unbounded height. No dependency changes; 251 library, 4 binary, 3 pipeline, and 14
  corpus tests pass under default and `js`; default/all-feature strict clippy and fmt are green.
  M1-B remains in progress at complete paint; manual milestone smoke remains human-run.
