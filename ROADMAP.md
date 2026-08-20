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
| `css/parser.rs` | placeholder only | **No change** — M1-B/M2 decision already locked: cssparser + selectors, never hand-rolled |
| `tests/support/dat.rs` (T2.5) | to be written | **Custom is correct** — no crate parses the WPT `.dat` fixture format; a ~60-line test-support parser justifies no dependency |

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
| M1-B — Style, layout, paint | UA cascade, box model, whitespace rules, painter, link list, corpus + goldens + proptest laws | (open) |
| M2 — Tabs & keyboard | TabManager complete, keyboard link navigation, anchors, `target=_blank`, help overlay | (open) |
| M3 — Mouse | Full mouse: zones, wheel, link clicks, hover, tab-bar clicks, middle-click new tab | (open) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off, pure Rust | (open) |
| M5 — Boa | Boa 0.21.1 behind trait; decision gate Boa vs Deno Core; host bindings subset; job pump | (open) |
| M6 — Stretch | Forms, floats/flex (Taffy option), external stylesheets, persistence, images, scroll memory, drag, console view, config | (open) |

Cross-cutting: test infrastructure (done: contract suites, snapshots, proptest, fakes) · gates (done:
local only, no CI — see Gates) · coverage floor (open: optional local, 80% overall / 90% css·layout·paint) ·
external conformance corpus (open — see Conformance corpus; the primary driver for M1-A and M5).

## Gates (local only — CI deliberately refused)

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
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
 net::Fetch ⇒ html::HtmlParser ⇒ core(DOM arena)       │ noop.rs · boa.rs (feature "js", default off)
   │        │
 css: CssParser(cssparser) → Cascade(selectors) → StyleTree
 layout::LayoutEngine → BoxTree (absolute coords, unbounded height)
 paint::Painter → DisplayList (NodeId hit-tags) → ui content widget
```

- Single crate, modules: `core`, `net`, `html`, `css`, `layout`, `paint`, `script`, `ui`, `app` + thin `main`.
- Cross-module boundaries via traits; only `app`/`main` know the concrete implementations (composition root).
- `app` never imports crossterm: events come in as `core` types, results via `deliver_fetch`, time injected.
- DOM never crosses threads. One thread owns everything except I/O (fetch worker threads only).
- No tokio. `boa_engine 0.21.1` optional dep behind feature `js`; runtime `--js=off` overrides the feature.
- Crate pins: report actual versions used in the updates log at each dependency milestone.

## Decisions log

### Locked
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
- `html5ever` for HTML: html5ever's tokenizer + tree-builder with our direct `TreeSink` building the
  slotmap `Document` (`(rejected)`: `rcdom` detour — we need the arena from day one and the
  `html5lib-tests` corpus is M1-A's landing gate, so the old promotion criterion is already met).
- Boa as first real JS engine (`(rejected)`: rquickjs — C toolchain / unsafe FFI; both sealed by trait).
- `ureq` (blocking, rustls native roots) behind `Fetch`; per-job thread + concurrency cap + timeouts;
  `file://` routed to `std::fs` inside the same trait impl. Browser-ish UA + override flag.
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
- M6 gate: Taffy-backed `LayoutEngine` impl replaces hand-rolled flex/grid work; `StyleTree` is
  layout-crate-agnostic by design (text measurement seam = our unicode-width wrapper).
- Forms/`addEventListener`/console view/config file — stretch, no trigger yet.

## Milestones

### M0 — Foundations (complete)

Traits exist with capability-parameterized contract suites running against fakes; app is I/O-free;
chrome renders and is snapshot-tested; event loop runs; all gates green.

- [x] `cargo init` (no git), Cargo.toml with pins: ratatui, url, slotmap, unicode-width, thiserror;
      optional boa_engine behind `js`; dev: insta, proptest, pretty_assertions. Edition 2024. (done)
- [x] `core`: `focus` (Focus), `geom` (Size/Point), `url` (Scheme + `url_fix` rules), `dom`
      (slotmap arena Document: NodeId, element/text, attrs, id index; Rc<RefCell<Document>> helper),
      `style` (StyleTree placeholder). (done)
- [x] Traits: `net::Fetch`, `html::HtmlParser` (parse_document + fragment w/ context element),
      `css::CssParser` + `css::Cascade`, `layout::LayoutEngine`, `script::JsEngine` +
      `script::JsHost`, all `Send + Sync` where they cross threads. (done)
- [x] `script::Capabilities` + contract suite (capability-parameterized) run against `NoopEngine`;
      FakeHost records calls. (done)
- [x] `ui` widgets: TabBar (truncate + overflow), Address bar (`EditBuffer` with char-index cursor),
      Content (lines + scroll clip), Status (url/message). TestBackend + insta snapshots. (done)
- [x] `ui::keymap`: Action enum + `Keymap` trait + DefaultKeymap; focus-aware routing (printables in
      Address go to the buffer exclusively; `q` never quits while typing). (done)
- [x] `ui::mouse`: zone model + injected `ChromeGeometry`; zones Tabs/Address/Content only; pure tests.
      (done) — capture/events wiring deferred to M3.
- [x] `app`: TabManager (open/close/cycle, close-last→fresh tab, history dedup, generation, scroll,
      layout-width cache), I/O-free `App` (handle_key/on_resize/step(now)/deliver_fetch/should_quit/
      dirty flag), start-page placeholder content, status messages for M1-bound actions. (done)
- [x] `main`: try_init/try_restore, poll(50ms) loop, draw-if-dirty, resize, terminal-error exit path,
      `--url` arg accepted (navigation lands M1-A). (done)
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
      decode/parse in the app. UreqFetch tested against a loopback-only local server. (done)
- [x] `Html5everParser`: html5ever's tokenizer + tree-builder with our direct `TreeSink` into the
      slotmap `Document`; `<base href>` read first; scripting flag = session JS enablement, fixed
      at parse time. `Node` grows `Comment`/`Pi`/`Doctype` + element/attr namespaces (svg/math/
      xlink/xml/xmlns dump prefixes) + move/reparent ops (foster parenting). Template contents
      live inline; the dump emits the `content` pseudo-line for `<template>`.  Parse fixtures for
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

### M1-B — Style, layout, paint (open)

- [ ] `cssparser` 0.37 + `selectors` 0.40 (pin per selectors' range) + own `Atom` conversions.
- [ ] CssParser impl + Cascade impl: UA sheet + inline `style` attr + `!important`; @media crude
      `screen` match; @import ignored+logged; degradation table (table/inline-block/flex/grid→block,
      position→static, percentage heights→auto, box-sizing honored, overflow-wrap break-word default).
- [ ] `layout` box model: block+inline, unbounded height, widths in cells, vertical inline margin/padding
      ignored, whitespace collapse/trim/pre, viewport-width-dependent reflow.
- [ ] `paint`: depth-order (bg bottom-up, borders box-drawing ≥2 cells doubled, text clipped);
      DisplayList with NodeId hit-tags; interactive-element list (`<a>`).
- [ ] Corpus fixtures + golden screen snapshots (margins, headings, borders, links, wide chars, `pre`).
- [ ] Proptest layout laws: text-glyph cells of leaf runs disjoint · boxes laminar per row ·
      hit-test round-trip returns deepest box · widths monotone in viewport · scroll clamp fixed point.
- Conformance note: css-syntax + WPT-selectors corpora are inherited — they are the upstream test
  suites of the `cssparser` 0.37 / `selectors` 0.40 crates themselves. No external corpus exists
  for terminal-grid layout (WPT layout tests need a pixel browser + orchestration); our gate stays
  corpus goldens + proptest laws.
- Acceptance: fixture corpus goldens green; laws green; manual example.com smoke good enough to scroll.

### M2 — Tabs & keyboard navigation (open)

- [ ] Full TabManager UX: Ctrl+T/W, n/p, Tab cycle (plain Tab = link nav; see below), close-last→fresh.
- [ ] Keyboard link navigation: interactive-element list focus, Tab/Shift+Tab, Enter activate,
      highlight; per-tab isolation; keymap + snapshot tests.
- [ ] `?` help overlay (keymap listing), snapshot-tested.
- [ ] Anchor links: `#fragment` → scroll-to-box + status line; no URL rewrite.
- [ ] `target="_blank"` links → new tab; per-tab history stacks stop at same URL (dedup).
- [ ] Reload (key `R`): generation++, fresh engine+document, scroll top (per-load pivot).
- Acceptance: checklist of every keybinding works in a scripted drive; per-tab state isolation tests.

### M3 — Mouse (open)

- [ ] crossterm mouse capture; zones wired to `ChromeGeometry` (Tabs/Address/Content only, wheel acts
      in content, clicks set focus, tab-bar clicks switch, middle-click/`target=_blank` → new tab).
- [ ] Link hover highlight (re-paint on hover-target change only) + status bar URL preview.
- [ ] Hit-test resolution contract: targets resolved by NodeId against live document at dispatch time.
- Acceptance: scripted zone tests + goldens with hover states; manual mouse walkthrough.

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

Forms & submission · floats/flex (Taffy gate) · external stylesheets + @media · persistence/backup ·
images (`[img]`→alt→Kitty protocol later) · per-history-entry scroll memory · drag input · console
view (F12) · config file · iframes/frameset/bidi remain non-goals beyond `. . .`.

## Test infrastructure (standing)

- Contract suites, capability-parameterized, defined in M0, run against every impl (M1-A fakes …
  M5 Boa) — an interface is defined by its tests.
- insta snapshots of ratatui `TestBackend` buffers (widget goldens), corpus goldens (M1-B),
  DOM tree dumps (M1-A).
- proptest layout laws (M1-B) — see milestone list; property tests for url_fix/EditBuffer since M0.
- FakeFetch + fake clock + fake Host; no test touches the network or the real clock.
- `tests/common/` shared fakes; inline `#[cfg(test)]` fakes where module-local.
- Perf smoke guard (M6 gate): largest corpus page layout+paint < 200ms debug.

### External conformance corpus (M1-A / M5)

- WPT `html/syntax/parsing/resources/*.dat` (pinned commit; the html5lib-tests repo is archived and
  points here) is the M1-A landing gate; the tokenizer suite is informational (inherited from
  html5ever upstream; it lives only in the archived repo, so it is never vendored). Compared
  against a serializer that walks our slotmap `Document`.
- test262 (pinned commit): executed through `boa_engine` 0.21.1; harness files + YAML frontmatter
  honored; per-milestone curated slices with an explicit xfail manifest.
- css-syntax + WPT-selectors corpora: inherited by adopting `cssparser` / `selectors` — their
  upstream suites are the conformance evidence; terminal-grid layout has no external suite.
- Rules: corpora live in `testdata/`, fetched once by `tools/fetch-corpus.ps1` (human-run, pinned
  via commit hashes); tests never touch the network; xfail manifest entries name the exact test
  file + reason; pass-rate progression is recorded in the updates log at each milestone.

## Non-goals (locked unless a milestone re-opens them)

Text selection/copy, iframes/`<frame>`, bidi/RTL/writing modes, `line-height`/fonts, border-radius,
inline-element borders, form controls pre-M6, cookies, `addEventListener` DOM events (click-only v0),
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