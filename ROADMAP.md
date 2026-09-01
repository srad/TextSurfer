# TextSurfer — Roadmap

A terminal text browser in Rust (ratatui). This file is the single source of truth for **current
status, decisions in force, and open plans**. It is not a work journal: dated narration lives in
`git log`, and the tests are the record of what is proven.

Update it when a decision changes, a task lands, or a flaw is found — before writing code that
depends on it. Standing rules and gate commands live in [`AGENTS.md`](AGENTS.md).

## Mission (north star)

The product is the **browser frontend** — the TUI chrome, keyboard/mouse interaction, and website
**rendering** (DOM → style → layout → terminal paint). Parsing is a means to an end, never in-house
craft: every parseable format goes through a mature, latest-published crate (html5ever, cssparser +
selectors, url, encoding_rs, ratatui, boa_engine). Custom code is reserved for browser behavior —
TreeSink glue, cascade/style tree, layout → terminal grid, painter, chrome/App/event loop — and for
parsing only where the ecosystem provably has no library. Every hand-rolled parser must be justified
against the audit below before it is written; the audit is re-run whenever a candidate crate appears.

Compatibility advances through **site profiles**: each profile turns the standards exercised by a
real site into generic HTML, CSS, JavaScript and browser-platform work. Specifications and applicable
Web Platform Tests remain authoritative; the site is integration evidence, never a reason for
URL-, class- or markup-specific production behavior. The active profile is **WP-A — Wikipedia
anonymous reader**. Later profiles extend the same engine to other sites.

### Custom-parser audit (2026-08-20, re-checked 2026-09-01)

| Parser | Status | Verdict |
|---|---|---|
| `net/encoding/prescan.rs` — WHATWG sniffing | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode-only; no sniffer exists in the ecosystem |
| `pipeline/render/response.rs` — document MIME sniffing | custom | **Keep narrow adapter** — mediatype owns declared MIME parsing; mimesniff 0.3.0 and mime-sniffer 0.1.3 are stale and incomplete, while infer and tree_magic_mini identify file signatures rather than implement the WHATWG browsing-context rules; custom code is limited to the security-relevant 1,445-byte HTML/text/binary classifier |
| `core/url.rs` — `url_fix` | not a parser | **No change** — real parsing delegates to `url`; scheme/host/search heuristics are address-bar UX |
| CSS parsing, selectors, media queries and computed values | Stylo 0.20.0 | **Adopted in M7** — Stylo owns standards parsing, cascade, media evaluation, computed values and dynamic invalidation; TextSurfer maps the result into its cell-quantised `StyleTree` |
| `css/cascade/counters.rs` | terminal rendering adapter | **Keep** — Stylo supplies computed counter and generated-content values; TextSurfer resolves list scopes, HTML ordinals and terminal marker reservation |
| Table layout | custom, implemented | **Custom is correct** — Taffy 0.14 has no table algorithm; `super-table` 0.3.0 takes string matrices, `iris-layout` 0.4.0 has no integrated CSS table formatter. Neither supplies anonymous-table fixup, spans, captions, border conflict resolution or nested box layout |
| Presentational HTML legacy values | narrow standards adapter | **Custom is correct** — nothing implements WHATWG's legacy integer/dimension/color algorithms. Keep isolated under `css::presentational`; compare with Ladybird `8baf4260d40dd53cd09c21c868d2bd0625a69149`, WHATWG authoritative |
| `font-size` computed-value grammar | narrow standards adapter | **Keep** — cssparser owns tokenization; the adapter maps supported keywords and length-percentage forms onto the frontend-neutral typography model |
| CSS custom properties and `var()` | Stylo 0.20.0 | **Adopted in M7** — substitution, cycles, inheritance and invalidation are browser style-system work |
| CSS math values | Stylo + narrow lowering adapter | **Keep `css/math.rs`** — Stylo owns parsing and computed-value typing; TextSurfer preserves basis-dependent expressions while lowering CSS px into terminal cells |
| Declarative refresh content | narrow standards adapter | **Keep** — no crate implements WHATWG's `meta[http-equiv=refresh]` microsyntax (searched 2026-08-26); keep the scanner isolated from navigation policy and cap chains in `app` |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` format; isolated from production code |
| CSS cascade, computed values and dynamic invalidation | Stylo 0.20.0 | **Adopted in M7** — no hybrid engine remains. Scope kept out of Stylo: cell geometry, palette and contrast correction, markers/counter resolution and table layout |

### Prior-art audit (2026-08-22, re-checked 2026-08-26)

Conformance is evidence, not reimplementation. Terminal browsers have already settled several
questions we were answering ad hoc.

| Source | What it establishes | Adopted here |
|---|---|---|
| [chawan](https://github.com/sourcehut-mirrors/chawan) (`doc/css.md`) | The terminal CSS contract: author colours contrast-corrected against the background; binary `border-*-width`; `font-weight > 500` = bold; underline/line-through; sub-cell inline margins ignored; overflow-x displays, overflow-y clips; `::before`/`::after` + counters for markers; link hints for keyboard navigation | All locked below. `font-size` ignored is our one departure: the VGA profile scales integer bitmaps, terminal cells stay fixed |
| chawan + [w3m](https://w3m.sourceforge.net/) | A real **table layout** engine is what separates a usable terminal browser from lynx | Delivered in M1-D, ahead of flex/grid |
| lynx · w3m · chawan | Every one ships a **non-interactive dump mode** | `--dump`; doubles as the golden-fixture harness |
| [Blitz](https://github.com/DioxusLabs/blitz) | Mirrors our decomposition — DOM + style + **Taffy for boxes** + a separate text layer | Confirms the architecture; no dependency. Its `blitz-dom` is the reference for embedding Stylo outside Servo (M7) |
| [stylo](https://crates.io/crates/stylo) 0.20.0 | Browser-grade cascade, computed values, snapshots and dynamic-state invalidation, on stable Rust with `default = ["servo"]` — no Gecko, bindgen, nightly or C++ toolchain | Adopted in M7 as the cascade. Adds one build prerequisite: **Python 3 on `PATH`** (`build.rs` runs the vendored-Mako `properties/build.py`). Uses `pool: None`, so no rayon threads are spawned |
| [stylo_taffy](https://crates.io/crates/stylo_taffy) 0.3.0-beta.2 | Maps Stylo computed values onto `taffy::Style`, against taffy 0.14 — our exact pin | **Reference only, not a dependency** — it emits px, while our layout is cell-quantised (`AxisCellLength`, per-axis rounding, the 65,535 cap, unresolved-percentage preservation) |
| [typed-arena](https://crates.io/crates/typed-arena) 2.0.2 | One lifetime for every value in an arena, which makes a reference-linked tree expressible without raw pointers | Adopted in M7 for the style mirror. Stylo's sharing cache requires a pointer-sized element handle, so the mirror cannot be index-linked; this is what keeps the links safe. MIT, zero dependencies, `std` feature only |
| [im](https://crates.io/crates/im) 15.1.0 | Thread-safe persistent maps and vectors with structurally shared clones | Adopted in M7 for `StyleTree` and its handle stores so local restyles share untouched structure. Default features off |
| [image](https://crates.io/crates/image) 0.25.10 | Signature-based raster decoding with explicit format features and decoder limits | The shared decoder for PNG, JPEG, WebP and first-frame GIF, default features off, TextSurfer-owned budgets |
| [ratatui-image](https://crates.io/crates/ratatui-image) 11.0.6 | Terminal-only Sixel/Kitty/iTerm2 output, sliced scrolling, halfblock fallback | The terminal adapter only. VGA blits into its own framebuffer |
| [arboard](https://crates.io/crates/arboard) 3.6.1 | Current cross-platform text clipboard access without a UI toolkit | Frontend adapters only, default features off; `app` exchanges owned clipboard requests and remains I/O-free |
| [resvg](https://crates.io/crates/resvg) 0.48.1 | Mature SVG parsing and bounded raster output without a browser DOM | Adopted for static SVG decoding and final-size VGA preparation behind bounded workers and page/frontend budgets; no custom SVG parser |
| [tracing](https://crates.io/crates/tracing) 0.1.44 + [tracing-subscriber](https://crates.io/crates/tracing-subscriber) 0.3.23 + [tracing-chrome](https://crates.io/crates/tracing-chrome) 0.7.2 | Structured logs plus Chrome/Perfetto timeline spans with an explicit flush guard | Opt-in file diagnostics only; no default terminal output and no response bodies, credentials, queries, fragments or form values |
| MediaWiki ResourceLoader | Wikipedia's documented base environment is HTML5, ES2017, Selectors API, local storage, `querySelector`, jQuery and `mediawiki.base`; scripts are asynchronous progressive enhancement | The WP-A JavaScript target. Implement the generic platform underneath Wikipedia's own code; never reproduce ResourceLoader or site modules in Rust |
| `ureq` 3.4.0 cookies + `cookie_store` 0.22.1 | Current latest HTTP-client cookie path, with persistence hooks | **Candidate only.** `ureq`'s internal store has no configured public-suffix list, so it is not the final browser jar. The browser-state tranche must prove RFC6265bis request context, SameSite, prefixes, HttpOnly and public-suffix rejection before selecting the library adapter |
| Every browser since Firefox 3 | `:visited` must never be observable to page styling | `:visited` parses and never matches |

## Status legend

`(open)` planned · `(in progress)` started · `(done)` finished with gates green · `(complete)`
milestone finished and accepted · `(rejected)` refused, reason in the decisions log · `(canceled)`
dropped.

`(done — smoke pending)` means code and gates are green but the human manual smoke has not been
signed off.

## Status board

| Milestone | Scope | Status |
|---|---|---|
| M0 — Foundations | Scaffold, traits + contract suites, chrome, I/O-free app, event loop, gates | (complete) |
| M1-A — Parse pipeline | net + encoding + html5ever→arena DOM, `<base>`, scheme routing | (done — smoke pending) |
| M1-R — Stabilization | DOM invariants, fetch routing, resource limits, terminal lifecycle | (complete) |
| M1.5 — Chrome redesign | DOS/QBasic rich UI: menu bar, tab strip, toolbar, themes | (complete) |
| M1-B — Style, layout, paint | UA cascade, box model, whitespace, styled paint seam, `--dump` | (done — smoke pending) |
| M1-C — External styles | Ordered `<link>`/`@import` loading, selector bucketing, `@media` | (done — smoke pending) |
| M1-D — Layout completeness | Tables, generated content + markers, length units, presentational attrs, VGA typography | (done — VGA smoke pending) |
| M1-E — Overflow and positioning | Element overflow clipping, inherited visibility, CSS positioning | (done — smoke pending) |
| M1-F — Practical rendering fidelity | Cross-context correctness, stacking, inline geometry, horizontal RTL | (in progress) |
| M2 — Tabs & keyboard | Link navigation, anchors, titles, error pages, in-page search, forms | (in progress) |
| M3 — Mouse | Zones, wheel, clicks, hover, dynamic pseudo-class state | (in progress) |
| M4 — JS seam | Runtime-neutral engine/factory traits + Noop impl + typed host layer | (done) |
| M5 — JavaScript and Web APIs | Boa 0.22.0; script processing; DOM, events and bounded host APIs | (in progress) |
| M6 — Browser platform | Images · browser state/request policy · storage/cache/security · floats ✅ · perf | (in progress) |
| M7 — Stylo cascade | Stylo 0.20.0 as the sole full and incremental cascade | (done — smoke pending) |

Cross-cutting: test infrastructure (in progress — static WPT backfill, corpus error-count and
astral attribute-order gaps) · gates (done, local only) · coverage floor (open — optional local,
80% overall / 90% css·layout·paint) · conformance corpora (html5lib tree output 95.16% raw / 100%
with xfail; static WPT crash/reftest pilot complete; test262 at M5).

### Active compatibility profile — WP-A Wikipedia anonymous reader (in progress)

WP-A closes when Wikipedia is a productive anonymous-reading browser in VGA and a productive
terminal fallback. Server-rendered content must be complete with JavaScript off; JavaScript on adds
enhancements without becoming necessary to read. Login, editing, watchlists and account preferences
belong to a later profile.

**Fidelity policy:** required behavior preserves content, logical reading order, layout, navigation,
interaction, state and security. Adapted behavior preserves those semantics while using the cell grid
or frontend-specific image path. A capability exception needs a named reason and usable fallback:
`border-radius`, for example, paints square corners. Deferred means feasible but not required by the
active profile; it is not a permanent rejection. Quantization never excuses missing or reordered
content, unintended overlap, a wrong target, unsafe behavior, or resize/resource divergence.

**Acceptance surface:** the resource-closed static corpus covers the Wikipedia portal/search path,
ordinary long articles, dense tables/lists, disambiguation/category navigation, SVG and responsive
media, galleries, code/preformatted and mathematical content, plus horizontal RTL and mixed-direction
text. Static evidence runs at 40, 100 and 160 columns in VGA and terminal. Interaction uses one
canonical viewport per frontend plus focused resize cases instead of multiplying every state across
the static matrix.

Required workflows are search, titles, links, fragments, table of contents, citations/backlinks,
language switching, collapsibles, sortable tables, history and scroll restoration, reload, in-page
search, selection/copy, persistent anonymous appearance preferences and explicit image/document
downloads. Audio/video exposes a usable link or download; embedded playback is deferred.

The existing script-disabled captures remain the static integration corpus. Rendering uses applicable
WPT reftests; DOM and Web APIs use focused local contracts and applicable testharness cases. Live
JavaScript-enabled Wikipedia/ResourceLoader verification is human-run: do not commit captured site
JavaScript bundles or let Rust tests touch the network. Every confirmed failure gets a generic
regression and stays classified in its owning milestone or existing corpus manifest rather than a
duplicate roadmap ledger.

M1-B through M1-E are completed component contracts. Their combined behavior on real pages remains
open until M1-F practical-fidelity and M6 image acceptance pass.

Test counts at the last green matrix run (2026-09-01): default/VGA **975 total**
(971 passing, 4 ignored), JavaScript+VGA **987 total** (983 passing, 4 ignored), and
terminal/no-default-features **856 total** (853 passing, 3 ignored). The WPT target contributes 7
tests (6 passing and 1 ignored; 5 passing and 1 ignored without `vga`).
Deliberately ignored: the WPT child worker, the VGA reference generator, and the M7 Stylo perf
measurement, which reports rather than asserts.

### Open risk register

Confirmed regressions become acceptance items in their owning milestone. These static candidates
were not promoted to bugs without an executable product reproduction; closed rows live in Git
history.

| Owner | Unconfirmed risk or test gap | Required disposition |
|---|---|---|
| M1-B | `text-decoration` accepts known tokens from an otherwise-invalid value; inline edge cells take the parent run style; overwriting one cell of a wide glyph can retain the old style | Add focused cascade/paint cases before changing behavior; close as disproved if no reachable layout producer can expose it |
| M2 | The address edit buffer is global across tab switches; cursor placement and toolbar writes lack sub-24-column coverage | Resolve with the per-tab-state, tiny-chrome and link-navigation tests M2 already owns |
| M4 | Template-content replacement is not exercised by html5ever | Exercise it at the first mutation-capable DOM caller; reject orphaning/overwriting |
| M6 | Extreme injected `Size` values can make the start page allocate `cols × rows × 2`; painter output stays dense by document row; inline-precise hover adds ~one linear-scanned hit region per text fragment | Resource ceilings, sparse-vs-dense evidence and indexed hit resolution behind the perf gate |
| Test infra | `tree_dump` is recursive on untrusted depth; UI clipping walks scalar values rather than grapheme clusters | Add bounded-depth and emoji/ZWJ cases; neither establishes a product crash today |

## Gates

Local only — CI deliberately refused. The command list is in `AGENTS.md`; all eight must be green
before any item is marked `(done)`.

## Session handoff

1. Read this status board, then recent `git log` entries for historical context.
2. Reproduce each visible rendering failure as a resource-closed fixture or minimal standards case;
   classify it under M1-F, M6 images or M4/M5 before changing production code.
3. Advance WP-A in this order, one test-first standards slice at a time:
   1. finish M1-F static flow, stacking, inline geometry and horizontal international layout, plus
      M6's remaining static image contracts;
   2. finish M2/M3 reader navigation, history/scroll restoration and page selection;
   3. add M6 browser request context, state, storage, cache and security policy;
   4. finish M5 classic-script processing, test262 and the DOM/event/network APIs ResourceLoader
      actually exercises;
   5. prove downloads and live progressive Wikipedia behavior, then close the profile.
4. Every defect gets a focused generic regression plus rendering-atlas or applicable WPT coverage;
   never add URL-, class- or site-specific production behavior.
5. Keep the existing script-disabled corpus offline and use its three-width/two-frontend matrix.
   Keep live JavaScript-enabled Wikipedia/ResourceLoader verification human-run.
6. Run the generic app benchmark after each correctness tranche. Re-baseline the cold target only
   after rendering acceptance; preserve the owner-slice and warmed interaction budgets.
7. Smoke retained row-local paint and stationary-pointer scrolling in VGA and terminal; its warmed
   release state-change-through-apply gate is 2.6 ms p95.
8. Run the gates before and after an implementation item; never mark `(done)` with red gates.
9. The manual smoke list (example.com, lite.duckduckgo.com, wikipedia.org) is human-run per
   milestone close and never automated.
10. Live repo: no commits without explicit user confirmation.

## Architecture (as-built)

```
 main.rs — frontend adapter only: CLI · VGA/terminal selection · event mapping and loops
   ▼
 ui (ratatui widgets · Focus · keymap · mouse zones) ──┐
   Action (Load, NewTab, ActivateLink, Scroll, …)      │ UiEvent
   ▼                                                   │
 app — I/O-free composition root · TabManager · event loop · fetch workers · js pump
   │ dispatch                                          ▼
   ▼                                     script: JsEngine trait + JsHost ← per-loaded-document
 pipeline — PageLoad resource graph · render facade · --dump
   │                                                   │
 net::Fetch ⇒ html::HtmlParser ⇒ core(indextree DOM)   │ noop.rs · boa.rs (feature "js", default off)
   │        │
 css: CssParser(cssparser) → Cascade(selectors) → StyleTree (+ pseudo boxes · list markers)
 layout::LayoutEngine → BoxTree (absolute coords, unbounded height, styled text fragments)
 paint::Painter → DisplayList (styled spans · NodeId hit-tags · link rects) → ui content widget
```

- Single crate, modules: `core`, `net`, `html`, `css`, `layout`, `paint`, `script`, `pipeline`,
  `ui`, `app` + thin `main`, plus `vga` behind its feature.
- Cross-module boundaries are traits; only `app`/`main` know concrete implementations.
- `app` is the composition root **only** — controller, `Tab`/`TabManager`, the `Navigate` adapter
  and the start page. The rendering pipeline (`PageLoad` resource graph, cascade→layout→paint
  facade, `--dump`) is `pipeline`, so `main` and the golden tests never import into `app`.
- `app` never imports crossterm: events arrive as `core` types, results via `deliver_fetch`, time
  is injected. It writes no files either — a screenshot key leaves a request the frontend takes,
  performs and reports back through `App::flash`.
- Mutable DOM never crosses threads. One owner sequence holds browser state and JavaScript; bounded
  fetch, image-decode and frontend image-preparation workers receive and return only owned immutable
  bytes, pixels, identifiers and value metadata. Layout/paint may additionally receive an owned,
  read-only render projection with no shared identity or mutation API and publishes
  generation-tagged artifacts atomically.
- `Document` owns DOM pre-insertion validation and hides indextree; template contents are detached
  fragments, and removal detaches rather than invalidating node handles.
- `StyleTree` carries per-node `ComputedStyle` plus two side tables — pseudo-element boxes keyed by
  `(NodeId, PseudoElement)`, and list markers keyed by node. Generated text is heap-allocated and
  `ComputedStyle` is `Copy`, so the strings live beside it rather than in it.
- No tokio. `boa_engine 0.22.0` is an optional dep behind feature `js`; `--js=off` overrides it.
- Module structure rules (a module is a responsibility, not a file) live in `AGENTS.md`.

## Decisions log

### Locked — product and architecture

- **UI design language:** classic DOS/QBasic text-mode rich UI (grey menu and context bars, boxed
  panels, bordered input field, raised tab strip, Turbo Vision palette on a blue desktop) around a
  normal browser layout shell. `ui::Theme` centralizes all colors; no color literals outside it.
  The View menu selects one session-scoped theme from Turbo Vision (default), Norton, Amber CRT,
  Green Phosphor and Paper White; `App` maps its appearance into `prefers-color-scheme`.
  Persistence belongs to M6 configuration.
- **Library-first everywhere but the frontend.** In-house scope: Document arena + TreeSink glue,
  cascade → style tree, layout → cell grid, paint/DisplayList, chrome/App/event loop, and table
  layout. Everything else adopts a mature crate.
- **Single crate with modules**, not a workspace — fastest iteration; a split is trivial later.
- **`#![deny(unsafe_code)]` at the crate root, with exactly one permitted exception:
  `css::stylo::dom`** (M7). Stylo's `TElement` declares seven `unsafe fn` methods for Gecko's
  benefit; under edition 2024 their bodies are ordinary safe code, so the trait impls contain no
  `unsafe` block — the lint fires on the token alone. `forbid` cannot be overridden by `#[allow]`
  anywhere in the crate, which is why the root relaxes to `deny`. The module also wraps those
  methods in safe functions, so nothing outside it needs an `unsafe` block; those wrappers are the
  only `unsafe` blocks in the tree. Tests assert exactly one `allow(unsafe_code)` and that every
  `unsafe` block is a call to one of Stylo's own `unsafe fn` trait methods — a raw-pointer
  dereference or a transmute fails them. *Rejected:* a separate adapter crate — same code, but it breaks the
  single-crate decision above and forces the DOM mirror, the style mapper and `RenderContext` into a
  public cross-crate API.
- **Stylo state never leaves the render worker, and the type system says so.** Servo's `Device` owns
  a `Box<dyn FontMetricsProvider>`, a `Sync`-but-not-`Send` trait object, and `unsafe impl Send for
  Device` exists only under the `gecko` feature. `Stylist` is therefore `!Send` on our path, so the
  retained style session cannot cross a channel even by mistake. Render jobs and results carry owned
  inputs only — stylesheet *source text*, never a parsed sheet.
- **The style engine sees a shadow DOM, not `core::dom`.** Stylo needs per-element interior-mutable
  `ElementData`, state bits, selector flags and stable pointer identity; putting any of that on
  `core::dom::Node` would drag `style::` types into `core` and break `Document`'s derives. The mirror
  arena lives in `css::stylo::dom`, is rebuilt on a hard revision and patched through the existing
  `MutateOp` funnel.
- **`app` is the composition root only; `pipeline` is the rendering subsystem; `main.rs` is the
  frontend adapter only.** Keeping `PageLoad`, the render facade and `--dump` in `app` forced
  `main`, the golden tests and `css`/`layout` unit tests to import into the composition root.
  `pipeline` takes its palette by injection rather than reaching up to `ui::theme`.
- Native text renderer (lynx/w3m/chawan family), not embedded-engine (carbonyl/browsh family) —
  our value is small footprint + cell-native layout.
- **Two frontends over one engine, behind ratatui's `Backend` trait.** `ui::chrome::draw` takes a
  backend-agnostic `Frame` and `app` never imports a terminal library, so a second frontend costs a
  `Backend` impl and an event adapter — nothing in `css`/`layout`/`paint` moves. VGA is the active
  default and owns native bitmap typography; the terminal frontend is frozen as a compatibility
  fallback (`--terminal`), stays available in no-default-feature builds, and its non-image rendering
  must remain byte-for-byte stable.
  *Why a window at all:* the DOS look is mostly the font, and inside a terminal the font belongs to
  the user. Owning a framebuffer is the only way to own the face, the cell metric and the palette
  together; it also doubles usable columns (1280×800 = 160×50) and makes images a native blit.
  *Rejected:* `mousefood` 0.5.2 (fixed-width `MonoFont` cannot express Unifont's 16×16 wide glyphs),
  `ibm437` 0.5.0 (ships 8×8 and 9×14 only), `minifb` 0.28 (polling keyboard model).
- **Font tiers are CP437 first, then Unifont** (`vga`). CP437 keeps the chrome authentically DOS;
  Unifont covers the rest of the BMP, which the web needs. Both are 8×16 (Unifont's wide glyphs are
  exactly two cells), so the tiers share one metric and one blitting loop.
  *Licensing:* CP437 bitmaps come from pcface's **Modern DOS 8x16** set (MIT or CC0); pcface's
  *Oldschool PC* bitmaps are GPL/CC-BY-SA and deliberately unused. Unifont arrives via
  `unifont-bitmap` (crate MIT/Apache-2.0; font data OFL 1.1 / GPLv2+ with the embedding exception).
- **`src/vga/font/table.rs` is generated and pinned.** `tools/gen_cp437.py` re-derives it from
  upstream at a pinned commit and refuses to write unless its re-render matches pcface's published
  `glyph.txt` byte for byte; the emitted SHA-256 is asserted by a test.
- CSS box model from day one (`(rejected)`: lynx-style linear flow — user chose the box model).
- `cssparser` + `selectors` for CSS (`(rejected)`: lightningcss — no selector matcher).
- `html5ever` with our own `TreeSink` building an `indextree`-backed `Document`; `scraper`/ego-tree/
  RcDom rejected (none meets the mutable multiple-root, detached-template, future-JS contract).
- Boa 0.22.0 as the first real JS engine behind the runtime-neutral adapter (`(rejected)`:
  rquickjs — C toolchain / unsafe FFI). The adapter boundary remains the Deno Core replacement seam.
- `ureq` (blocking, rustls native roots) behind `Fetch`; four fixed workers over crossbeam channels,
  superseded jobs cancelled per tab, 10 MiB shared body limit. `mediatype` parses response metadata.
  The default user agent is `TextSurfer/<version> (+https://github.com/srad/TextSurfer)`;
  `--user-agent` remains an exact override. HTTP 429 is reported without automatic retries.
- **Diagnostics are opt-in and file-only.** `--log-file` appends structured `tracing` events under
  `RUST_LOG` (default `textsurfer=debug`). `--diagnostics <prefix>` is exclusive with it and creates
  new `<prefix>.log` and `<prefix>.trace.json` files under the fixed
  `textsurfer=debug,textsurfer::perf=trace` filter; existing captures are never overwritten. The
  timeline records correlated owner, render-worker and presentation work plus typed coalesced
  invalidation causes. URLs retain origin and path but drop credentials, query and fragment; bodies,
  cookies and form values are never logged.
- **Per-load pivot invariant:** any navigation ⇒ `generation++`, fresh `Document` + fresh
  `JsEngine`, scroll reset to top, stale/generation-tagged fetch results dropped.
- JS host mutation funnel: all DOM changes through a single `MutateOp` enum ⇒ one invalidation path.
- Contract suites are capability-parameterized (`Capabilities { executes_scripts, async_host_ops }`).
- Pressure ceilings: JS job pump ≤256 steps/tick, re-layout only on content-width change, scroll
  clamp after every re-layout, errors surface in the status bar; backend I/O errors exit.
- No CI anywhere (`(canceled)` by user). Local gates only.

### Locked — terminal CSS semantics

- **The UA stylesheet is Stylo CSS at `Origin::UserAgent`.** Palette-dependent link colours are a
  user-origin sheet rebuilt on theme change; terminal policy CSS cannot express stays in
  `css::stylo::map`.
- **Colour model:** `ComputedStyle::color` is `Option<Rgba>` with byte alpha; background and border
  colours are `Option<Rgb>`/`BorderColor`. `None` means "the theme decides". The painter composites
  partial foreground alpha against the effective cell background, suppresses fully transparent
  glyphs without changing geometry, then **contrast-corrects** the resolved foreground — fidelity
  never outranks legibility.
  Stylo always computes a concrete `color`; the `None` sentinel survives as a **reserved colour**
  the mapper turns back into `None`,
  declared by an `html { color: … }` rule in the UA sheet rather than by mutating the
  embedder-supplied `Device`'s default computed values — `initial_values_with_font_override` builds
  every other field from `get_initial_value()`, so the sheet route needs no `Arc::get_mut` and
  reaches every element by inheritance. It also gives `BorderColor::CurrentColor` for free: a
  resolved border colour equal to the sentinel is `currentColor`. Its only false negative is an
  author declaring literally that RGB; the exact fallback is walking the element's rule node for an
  author/UA-origin `color`. `background` needs no sentinel — Stylo's initial `background-color` is
  `transparent`, which is `None` exactly.
- **Borders are binary:** any non-zero width paints one box-drawing frame.
- `font-weight > 500` = bold; `text-decoration` maps to underline/line-through; reverse video is a
  style bit. `font-size` computes in every frontend: terminal cells keep their one-cell appearance,
  VGA uses the computed size for native bitmap scaling.
- Sub-cell margins and padding are ignored on inline boxes; CSS lengths are capped at 65,535.
- Absolute, font-relative and viewport-relative lengths resolve through the shared 8×16 cell metric.
  The terminal profile keeps fixed 16px/8px font approximations; the VGA profile uses element and
  root computed font sizes for `em`/`ex`/`ch`/`rem`. Layout rounds per axis; media queries compare
  unrounded CSS pixels.
- The viewport clips x and leaves the document y axis unbounded. Element `overflow` supports
  `visible | hidden | clip | scroll | auto`; a specified `visible` axis computes to `auto` when the
  other is scrollable, while `clip` stays distinct. Element clips use the padding box. `scroll` and
  `auto` create no terminal scrollbar; root/body values propagate to the viewport.
- Positioned layout supports `static | relative | absolute | fixed | sticky` with signed insets.
  Absolute descendants resolve against the nearest positioned ancestor's padding box, fixed against
  the viewport. `sticky` degrades to `relative`, stacking stays source/depth order without
  `z-index`, and negative origins clip rather than translate.
- `:visited` parses and **never matches** — page styling must not observe history.
- Colour values are parsed by `cssparser-color` 0.5.0 (same cssparser 0.37 pin): keywords, hex,
  `rgb()/rgba()`, `hsl()`, `hwb()`. CIE spaces parse but do not convert to sRGB, so those
  declarations are ignored rather than guessed.
- **`opacity` is honoured only at `0`**, where it computes to `visibility: hidden` — identical
  layout behaviour, so it rides existing machinery. It earns its place because `opacity: 0` over a
  styled box is how the web builds a custom control. Two accepted divergences: a descendant
  declaring `visibility: visible` reappears, and the element stops being hit-testable.
- **A replaced element keeps room for its own rows against a smaller `max-height`/`height`.** A 1px
  border costs a whole cell here, so honouring a 2rem budget literally would render a bordered field
  blank. Implemented as a height-axis `min_size`; width stays freely settable.
- **A form control is a replaced element, not a box of text.** Its rendering is generated to fit the
  box it ends up with — from `content_rect`, never from `FlowBox::inline`, which is emitted at the
  border-box origin and reserved for anonymous boxes. Brackets delimit a control only when nothing
  else does; the field's extent is carried by reverse video, never a filler glyph. `<img>` is the
  exception: its `alt` is the author's prose, so it wraps like ordinary text.
- **Generated content is inline-level:** `display` on a pseudo-element is not honoured, and
  `list-style-type` accepts keyword counter styles only.
- **List markers are outside markers** by default: the item reserves a left field, shared and
  right-aligned across siblings so numbers meet one text column and wrapped lines align under the
  item text. `list-style-position: inside` renders the marker as inline content. `ul`/`ol` add no
  indent of their own — nesting indents because each level starts after its own marker field.
- **Remote subresources never gain local-scheme access.** HTTP(S) documents cannot load `file:` or
  another local scheme. Until M6's mixed-content policy lands, HTTP and HTTPS are treated as one
  remote family; rejected cross-family occurrences count as failed resources.
- **A non-2xx response keeps its body** — a server's own error page is a page — but is never
  accepted as a subresource; a 404 is not a stylesheet.
- Remote `@font-face` and custom font families fall back to built-in fonts; border radii render as
  square corners because the cell grid cannot represent them faithfully. Relative colours remain
  deferred until a compatibility profile promotes them. M1-F and M6 own `line-height`, inline box
  decoration and CSS background images.

### Deferred — decision gates with explicit triggers

- **M5 gate:** if Boa's async (fetch promises / timers / TLA) cannot keep the UI responsive after
  the contract suite + fixture, switch to Deno Core (V8) behind the same `JsEngine` trait.
- Taffy owns block, flex, grid and physical float box calculation; inline formatting uses textwrap
  fragments plus Unicode cell/grapheme libraries because Taffy has no inline layout. TextSurfer
  shapes source-ordered inline content around float bands; table layout remains in-house.
- **Multi-process rendering** (chawan's model) remains deferred. Interactive rendering is
  incremental and epoch-based: input precedes bounded owner-sequence parse/cascade/snapshot work,
  while one bounded worker lays out and paints immutable projections without owning the DOM.
- Syscall-filter sandboxing is a **non-goal** — Windows is the primary target.
- A Readability-style reader view is a candidate differentiator (M6), not a commitment.

## Shipped milestones

`(done)` here means code and gates are green; the human smoke on the manual list may still be
outstanding. Detail on how each item landed is in `git log`.

### M0 — Foundations (complete)
Scaffold, `core` types, every module trait with capability-parameterized contract suites against
fakes, `ui` widgets with TestBackend snapshots, focus-aware `ui::keymap`, `ui::mouse` zone model,
I/O-free `App` + `TabManager`, poll/draw loop.

### M1-A — Parse pipeline (done — smoke pending)
html5ever 0.39 through our own `TreeSink` into the `Document` boundary; WHATWG encoding sniffing;
`UreqFetch`/`FileFetch` behind `Fetch` with a 4-worker pool, timeouts, redirects and generation
tagging; `<base href>`; scheme routing; tree-dump snapshots. 24 parse fixtures plus the WPT
html5lib corpus at **1829/1922 raw (95.16%)**, 100% with the xfail manifest, zero panics.

### M1-R — Stabilization (complete)
indextree-backed DOM behind the `Document` boundary (detached template fragments, quirks mode,
checked pre-insertion, iterative traversals); fetch delivery routed by stable tab ID + generation;
10 MiB body limits; mediatype classification; usize scroll/extents with width-only invalidation;
ratatui-managed terminal lifecycle + clap CLI; grapheme-safe editing; corpus floor counting raw
passes only.

### M1.5 — Chrome redesign (complete)
`ui::theme` with a zero-literal rule and five session-selectable palettes; full-width menu bar with
`Alt+F/N/V/H` dropdowns; raised NC-style tab strip with an aligned active divider; three-row framed
navigation buttons wired to per-tab history; a framed `URL:` field; one-cell outer toolbar padding;
themed enabled-button hover; context bar. Main-menu titles and enabled rows follow pointer hover,
open dropdowns switch across titles, unavailable navigation commands stay dim, and commands support
click-release and press-drag-release with the normal arrow cursor. The one-row compact layout
remains below 47 columns or nine rows. Drawing, hit-testing and URL editing share one geometry, and
closed menu-bar hover repaints only chrome.

### M1-B — Style, layout, paint (done — smoke pending)
cssparser 0.37 + selectors 0.40 adapters with specificity and structural matching; the first
`Cascade` (UA + embedded author + inline `style`, `!important`, source order); `@media` with an
injected screen context; Taffy 0.14 block geometry with anonymous boxes, margin collapse, padding,
one-cell borders and content-box/border-box; six inheriting `white-space` modes over node-owned
textwrap fragments; the styled paint seam (`DisplayList` of styled spans + hit tags + link rects,
backgrounds → borders → clipped text, alpha compositing and contrast correction); dynamic
pseudo-classes parsing against injected state; width-and-category line-break opportunities; pager
keys; `<img>` alt fallbacks and `<hr>`; `--dump`; the golden corpus and the six proptest laws.

### M1-C — External stylesheets (done — smoke pending)
Ordered `<link rel=stylesheet>` and recursive `@import` discovery against the effective base, on the
navigation scheduler; cascade document order preserved independently of completion order, with one
coalesced late repaint and lazy background-tab rendering; selector bucketing by rightmost simple
selector; `@media` `scripting`, `prefers-color-scheme`, `width`/`height`; late-repaint scroll clamp.

**Locked implementation details:** the render-blocking window starts after parsing and lasts five
seconds, and only currently applicable occurrences block. Resource keys are `(tab, generation,
resource)` without changing the URL-only `FetchRequest` API. Discovery uses the first non-template
HTML base, strips fragments, and resolves external imports against final response URLs. Logical
occurrences preserve order while normalized URLs fetch once. Import depth is 8; the 64 limit counts
occurrences. Separate 32 MiB retained-raw and unique-decoded ceilings atomically disable external
CSS when crossed; the per-response 10 MiB failure stays local. Missing or invalid MIME defaults to
CSS; other valid MIME is rejected except for same-origin quirks documents.

### M1-D — Layout completeness (done — VGA smoke pending)
Sequenced before M2 because a terminal browser is judged on whether real pages are readable.

- **Table layout** — CSS anonymous-table fixup beside Taffy's block geometry; auto and fixed column
  resolution, `colspan`/`rowspan`, nested block and inline tables, top/bottom captions, separate and
  collapsed borders, per-edge border width/style/colour, sparse paint. HTML tables get compact UA
  spacing (one horizontal cell, zero vertical); authored `display: table` starts at zero. Fixed-layout
  overflow clips at the inner edge on grapheme boundaries. Percent constraints evaluate once against
  the selected width. Resource limits degrade an oversized table to block flow, preserving content.
  Visible collapsed-border glyphs use the table background, or the ancestor/canvas when that is
  transparent; column-group, column, row-group, row and cell backgrounds stop at those glyphs.
  Borderless and transparent segments retain normal table-layer ownership, separate borders retain
  normal CSS background painting, and border glyphs never inherit text modifiers.
  Horizontal table repair is active: backgrounds paint once for the table and through each real
  cell's column-group, column, row-group, row and cell layers; sparse slots expose only the table;
  collapsed-border conflicts preserve authored-width priority independently from one-cell geometry;
  row spans cannot cross row groups. Retained painting rebuilds collapsed table geometry when the
  winning border changes between visible and invisible ink. The Linux-layers fixture, focused table
  contracts and static WPT cases are the acceptance evidence; vertical writing and general bidi
  remain M1-F.
- **Generated content and markers** — `::before`/`::after`/`::marker`; `content` with strings,
  `counter()`, `counters()`, `attr()`, `none`/`normal`; counters over a depth-scoped stack;
  `list-style-*`; `Display::ListItem`; shared right-aligned outside marker fields; `<ol start>`,
  `<ol reversed>`, `<li value>`, `ol`/`ul` `type`. Generated fragments carry the originating
  `NodeId`, so links, hit-testing and search work through them.
- **Outer/inner display modes** — computed display retains outside, inside, box-generation and
  table-internal categories including legacy and multi-keyword grammar. `contents` elides its
  principal box after inheritance; inline flow-root/table/grid boxes are atomic with shrink-to-fit
  and a last-content-line baseline; inline flex uses Taffy's first flex-line baseline. Anonymous
  table wrappers group consecutive internal roles with no fabricated DOM owner.
- **Length units and the cell metric** — a nominal 8×16 CSS-pixel cell and 16px root font;
  `px`/`in`/`cm`/`mm`/`Q`/`pt`/`pc`, `em`/`rem`, `ex`/`ch`, `vw`/`vh`/`vmin`/`vmax`; axis-aware
  rounding with positive halves upward; MQ4 feature-first, value-first and chained ranges beside the
  legacy colon/min/max forms. Intrinsic sizing measures the widest unbreakable segment, not the
  widest grapheme.
- **Presentational HTML** — `align`, `bgcolor`, `width`, `cellspacing`, `cellpadding`, `border`,
  `rules`, `frame`, `valign`, `<center>`, `<font color>` enter the normal author origin at zero
  specificity; attribute-dependent UA defaults stay at UA origin. Inherited `text-align`, table-cell
  `vertical-align`, reusable auto margins, WHATWG integer/dimension/color parsing.
  `table[align=center]` uses auto margins; left/right table alignment maps to physical floats.
- **VGA-native bitmap typography** — inherited `font-size` from the supported grammar, absolute and
  relative keywords, CSS-wide keywords and UA heading sizes. VGA rasterizes CP437/Unifont at integer
  1×–4× cell scales (thresholds 32px, 24px, 18.72px, 16px); positive smaller text stays 1× and dim,
  zero has no glyph or advance. Scaled runs own full-cell rectangles, bottom-align mixed runs,
  degrade a common line scale until definite widths fit, wrap at 1× and clip whole graphemes.

### M1-E — Overflow and positioning (done — smoke pending)
Cascaded `overflow`/`overflow-x`/`overflow-y` and inherited `visibility` including pseudo-elements,
anonymous boxes, CSS-wide keywords and the cross-axis fixup; mapped to Taffy with zero-width
scrollbars, clipping every layout producer on all four edges without moving negatively positioned
content; cascaded and laid out `position`, `inset` and the four longhands, blockifying absolute/fixed
boxes and resolving containing blocks through a source-order-preserving flow-tree pass; constrained
definite-width auto-layout tables with a one-cell column floor and grapheme-boundary clipping.

**Deliberate limits:** M1-F owns `z-index`, relative positioning of non-replaced inline boxes,
positioned descendants inside the table-cell formatter, true sticky behavior and a fixed-position
repaint layer. `clip: rect()` remains deferred until a concrete readability case requires it.

## Open milestones

### M1-F — Practical web rendering fidelity (in progress)

M1-B through M1-E remain completed component contracts; this milestone owns their interaction on
real static pages. CSS specifications and applicable Web Platform Tests define correctness;
resource-closed browser captures are integration evidence rather than a substitute oracle.
TextSurfer projects that behavior onto its fixed cell grid without accepting structural divergence
as quantization. Script-dependent missing content belongs to M4/M5 and later site-compatibility
work when their host bindings are insufficient.

- [x] **Defect intake and offline corpus (done).** `tools/browser-reference.json` owns a
  resource-closed six-page corpus covering the required shapes at 40, 100 and 160 columns in both
  renderers. The isolated 36-case matrix verifies every byte by SHA-256, uses exact occurrence-aware
  LCS and direction-aware browser bands, forbids A3 overlap exceptions, and locks each classified
  A1/A2 finding group by count, digest, owner and reason. Capture is human-run and manifest-driven;
  replacement is explicit and staged atomically, while Rust tests remain offline. Parent watchdogs
  report assertion mismatches, harness errors, crashes and timeouts separately.
- [x] **Formatting contexts and containment** *(done).* Paint containment clips overflow through
  `ClipRegion`; layout passes only authored `contain: layout` and `contain: paint` to Taffy and maps
  `flow-root` directly. Ordinary blocks remain in their
  parent's formatting context so line boxes can widen below floats. Size and style containment stay
  explicit open limits; paint containment's positioned-containing-block and stacking effects remain
  owned by the stacking item below.
- [ ] **General flow correctness** *(in progress).* Close block-formatting-context, margin-collapse, anonymous-box
  and replaced-element interactions across block, float, table, flex and grid layout before
  advancing to paint-order work.
  *Classified from the browser corpus, in priority order:*
  1. **Nested spanning-table intrinsic width (done).** Table atoms propagate min-content separately
     from used width, captions enforce their min-content contribution, and percentage tracks remain
     intrinsic percentage constraints clamped to 100% until final distribution. Nested `rowspan` +
     `colspan` content wraps without disappearing at 40, 100 and 160 columns.
  2. **Float/image cross-context correctness (done).** Shrink-wrap right-floated table and
     flex image containers, let authored paragraphs reclaim the full measure below a float, and
     carry decoded image atoms through table cells, captions, anonymous fixup, nested/degraded
     output and link geometry. A table caption's max-content width must not widen an image-backed
     figure beyond its grid; only the caption's min-content contribution may do so. The Linux
     article's figures and footer navboxes are integration fixtures; the implementation remains
     markup-agnostic.
  3. **Bounded float exclusion geometry (done).** Invalid or over-budget float slots fall back to
     ordinary inline shaping; Taffy edges and offsets are bounded before integer conversion, and
     saturated zero-extent intervals never enter the paint index.
- [ ] **Stacking and positioned content.** Implement `z-index` and bounded stacking contexts,
  relative positioning for inline boxes, positioned table descendants, and true fixed/sticky
  behavior. Paint order and hit testing must agree on the topmost box.
- [ ] **Inline geometry and typography.** Preserve CSS-pixel `line-height`, inline borders and
  backgrounds, and `aspect-ratio` through layout and paint. CP437/Unifont and integer VGA scaling
  remain the font contract; this item does not introduce remote fonts or arbitrary font families.
- [ ] **Horizontal international layout.** Add bidi/RTL ordering, direction-aware physical behavior
  and the common logical sizing, spacing and inset properties. Vertical writing modes remain
  deferred.

A critical corpus failure is missing or reordered in-flow content, unintended overlap, a wrong
topmost hit/link target, divergence after resize or late resource completion, or an abort/hang.
M1-F remains open until the offline corpus has no unclassified critical failures, the authoritative
rendering profiles do not regress, the WP-A static families pass at 40/100/160 columns in VGA and
terminal, and the manual example.com, lite.duckduckgo.com and wikipedia.org smokes pass in both
native frontends. Run the generic app benchmark after each tranche and report the numbers, but treat
performance as secondary to correctness until this acceptance gate closes.

Remote/custom fonts use a built-in fallback and border radii use square corners as explicit WP-A
adaptations. Relative colours, transforms, multicolumn layout and vertical writing remain deferred
unless a compatibility profile promotes them; a reachable content or interaction effect cannot be
waived merely because its visual presentation needs adaptation.

### M2 — Tabs & keyboard navigation (in progress)

Started from the robustness end rather than the keyboard end, because the failure paths were what
the browser did worst. Render robustness, the non-2xx body, the load-status line, declarative
refresh, the designed start page, page screenshots and linear-time DOM child construction are done;
basic form operation is done. After the current M1-F and remaining static-image tranche closes,
resume keymap unification and finish keyboard link hints. Help, in-page search, history caching and
other secondary polish follow that usable browsing path. WP-A then adds page selection and the
explicit download surface; M6 owns the shared browser profile and transport policy beneath it.

- [ ] **Keymap unification** *(next after the static-fidelity/image tranche; prerequisite to keyboard links)* (extends the M0
      keymap tests, same file): `Ctrl+L` (+ existing `a`)
      focuses the address bar so `/` is freed; `/` becomes in-page search; `Tab` in the address bar
      moves focus to content; new `FocusTabs` action (`F6`).
- [ ] **`?` help overlay** *(after forms)* rendered from the keymap definition as the single source
      of truth, snapshot-tested.
- [ ] **Keyboard link navigation (in progress)** *(finish after keymap unification)* over the M1-B
      link list: Tab/Shift+Tab + Enter and focused-link scrolling are shared with the forms focus
      ring. Remaining work: the default focused-link highlight, repaint only on target change (one
      invalidation path shared with M3 hover), and **link marks/hints** (lynx-style numbering) as
      the discoverable form.
- [ ] **TabManager completion:** page titles from `document.title` with host/URL fallback; in-flight
      loads show the URL. (Ctrl+T/W/N/P cycling exists since M0.)
- [ ] **Anchor links:** `#fragment` → scroll-to-box + status line; no URL rewrite.
- [x] **`target="_blank"`** links → new tab from pointer or keyboard activation; per-tab history
      dedup is verified by tests.
- [ ] **In-page search:** `/` opens a prompt (reuses the text-field widget), `n`/`N` next/prev with
      scroll-into-view, match highlight distinct from link focus, `x/y` counter, `Esc`/`Enter`
      closes.
- [ ] **Rendered error pages.** *Half landed:* non-2xx responses keep the body, so a server's own
      404 renders with the status in the context bar, and `accepts_stylesheet_response` rejects any
      non-2xx up front. *Open:* the failure screens are still the three-line stub, and the
      unparseable-URL and unknown-scheme paths in `navigation.rs` paint nothing at all — both need
      the real themed page.
- [x] **Content-type honesty** *(done).* Declared supported types are authoritative; missing,
      malformed and generic types use a bounded WHATWG-minimum HTML/text/binary classifier, and
      unsupported content reaches the controlled failure page in both interactive and dump modes.
- [ ] **Back/forward without refetching and scroll restoration.** A small per-tab history cache keeps
      immutable response/resource inputs plus scroll state. Back/Forward rebuild a fresh `Document`
      and `JsEngine` under a new generation without network refetch, then restore the clamped scroll
      position after the coherent layout. Entry and byte caps with deterministic eviction bound the
      cache; cached mutable DOM or script state never crosses the per-load pivot.
- [ ] **Page-content selection and copy.** Keyboard and pointer-drag selection use rendered fragment
      geometry in both frontends, survive scrolling and clip correctly, and copy logical document
      order rather than paint order, including mixed bidi text. Selection clears safely when a hard
      revision invalidates its anchors and reuses the existing frontend clipboard request path.
- [ ] **Explicit downloads.** Link/image actions may create the same typed browser request used by
      navigation, while `app` carries request metadata, suggested name and status only. The terminal
      adapter streams the policy-checked response to a temporary file under separate limits,
      sanitizes `Content-Disposition`/URL names and atomically publishes success. Unsupported
      documents offer download instead of a blank page; automatic script downloads stay inert.
- [x] **Basic forms** — mixed DOM-order link/control focus; the URL bar, text/password inputs and
      multiline textarea share one ratatui text-field widget with selection, clipboard commands,
      pointer placement and an opaque context menu painted with the active chrome theme; enabled
      menu rows follow pointer hover with that theme's selected foreground/background, while
      disabled rows stay dim, and the popup keeps the normal arrow cursor while hiding the focused
      field's caret. A retained field owns and clears its full paint area, so authored
      placeholder glyphs disappear as soon as the edited value is non-empty; its
      end-of-text caret always occupies a visible blank cell rather than covering the last glyph.
      Page-field cursor coordinates include the content frame's left rail, so the widget and native
      caret address the same cell. Text edits repaint only their retained field rows.
      Checkbox/radio toggles, single-select, reset and successful-control serialization are live; GET and
      URL-encoded POST are bounded and transactional, POST history is non-replayable, and
      validation, multiple-select, file/image inputs and non-URL-encoded encodings remain inert.
- [x] Reload (`R`): generation++, fresh engine + document, scroll top (per-load pivot). *(M1.5)*
- [x] Linear-time construction of new DOM children — indextree 4.9.0's `append_value` for values
      `Document` creates; existing-node attach/insert/move keep validation and checked mutations.
- [x] Reader-facing load status — a rendered document reports that it loaded, not
      parse-error counts or generation numbers; actionable HTTP, fetch, stylesheet and layout
      failures still reach the status bar. The bottom-right progress widget uses measured byte or
      resource counts where totals are known and animated named phases for indefinite work, including
      elapsed cascade/layout/paint time; it never presents a fabricated overall percentage. The
      five-second coherent initial-resource window coalesces styles and images into one first paint;
      resources arriving after it repaint normally.
- [x] Declarative refresh navigation — the first valid WHATWG `meta[http-equiv=refresh]` in document
      order, including `<noscript>` markup while scripting is disabled; zero-delay navigation through
      the per-load pivot, trampoline history entries replaced, chains capped at eight, `--dump` on
      the same route. Delayed refreshes stay inert until M2 has a timer/cancel interaction.
      *(smoke confirmed)*
- [x] Render robustness — block nesting capped at `MAX_BLOCK_DEPTH = 256`, past which the flow tree
      stops and paints `[nesting too deep to render]`. The cap is measured: on the 1 MB stack
      Windows gives the main thread, an uncapped debug build overflows between 460 and 480 levels.
      Taffy `expect()` calls became a degraded tree; `LayoutLimits` rides the `BoxTree` and then the
      `DisplayList` rather than letting `layout` write the status bar. `FetchPool::try_recv`
      separates `Disconnected` from `Empty`, `submit` returns `Queued`/`Duplicate`/`Closed`, and all
      four quit paths call `Navigate::shutdown`, which detaches instead of joining. The VGA window
      close path enters an irreversible closing state, releases frontend resources and performs no
      later app ticks before the event loop returns.
- [x] Designed start page — `about:blank` is a viewport-aware half-block scene with an exact 78×16
      default canvas that scales and centers with the content viewport. It deliberately carries no
      instructional copy; the help overlay owns the keymap reference.
- [x] Page screenshots — `F12` writes the rendered page, not the screen: every painted row, no
      chrome, into `screenshots/<utc-stamp>-<frontend>.png`, rasterised by `vga::capture` through the
      same glyph table, palette and overlay path the window draws with. *Contracts a future reader
      must honour:* the frontend name is part of the file name because the two rasterise a page
      differently, and a same-second repeat takes a `-2` suffix; `app` stays I/O-free, so the key
      leaves a request the frontend answers with `App::flash`; a live flash notice forces a full
      content repaint instead of a retained scroll and joins `occlusion_rects`; a capture stops at a
      32 MP budget and reports how many rows it got; a `--no-default-features` build has no glyph
      table and says so instead of writing a file.

**Acceptance:** a scripted-drive checklist of every keybinding including the rebinds; per-tab state
isolation tests; snapshot-tested chrome items (titles, error pages, search, help overlay); the
robustness, cache, selection and download-request items proven by focused tests; manual DuckDuckGo
Lite submission and the WP-A search/anchor/copy/download walkthrough work in both frontends.

### M3 — Mouse (in progress)

Sequenced ahead of M2 at the user's request. VGA-first: the window frontend is the default, so winit
is the primary adapter and the crossterm mapping keeps the frozen terminal fallback aligned.

**Hit-test resolution contract:** hover and link activation use one row-indexed,
topmost-in-paint-order resolver against the live document; DOM-ordered link storage remains the
keyboard-navigation contract.

**Slice 1 — navigation (done — partial smoke: VGA wheel, links, toolbar and menu confirmed; hover
preview and hand cursor, tab chips and `+`, address caret, middle-click and `target="_blank"`, side
buttons, and the whole terminal frontend not yet exercised by a human)**

- [x] One content origin shared by paint and hit-testing (`ChromeGeometry::content_view`).
- [x] Pointer plumbing: `MouseKind` carries its button, `MouseButton` gains winit's `Back`/`Forward`,
      both frontends map pointer events into `core::event`; terminal mouse capture is enabled and
      released around the loop plus a panic hook, since `ratatui::restore` does not clear it.
- [x] Zone dispatch over `ChromeGeometry::target_at`: menu titles, popup rows, tab chips, the `+`
      box, toolbar buttons including dimmed states, address focus with caret placement that survives
      a partial edit, mouse-drag range selection whose theme colors cover only the selected URL
      segment, and content focus. Everything routes through the existing `Action` set — the mouse
      is not a second command set. Wheel scrolls only in content.
- [x] Link activation on release against the tracked press node (WHATWG `handle_mouseup`),
      middle-click and `target="_blank"` to a new tab, `<base href>`-aware resolution, same-document
      fragments deferred to M2's anchor item.
- [x] Hover: status-bar URL preview, repaint only on target change, re-derived after scroll,
      navigation, tab switch and resize; `CursorIcon::Pointer` over links in the window.

**Slice 2 — live dynamic state (in progress — interactive fluency regression under diagnosis)**

The correctness defects that made VGA scrolling render as a frozen bulk with drifting fragments are
fixed and guarded by real-`Surface` pixel-equivalence and page-down tests. What remains open is
fluency: the native window still feels loaded and laggy.

- [ ] Replace callback-driven redraws with one cross-frontend frame transaction: coalesced domain
      input, one final viewport/dynamic-state commit, semantic chrome/content damage, one
      presentation opportunity. The event budget is a fairness bound, not a frame-rate limiter. The
      injected-clock scheduler gives both adapters an immediate idle frame and a 16.667 ms sustained
      cadence, carries discrete backlog losslessly, and never wakes while idle. *(synthetic cadence
      contract is green; native smoke still fails)*
- [x] Retain the presented Ratatui buffer and page scene. Pure scrolling moves the content-row
      region and paints only exposed rows. Continuous dynamic state coalesces to the next 16.667 ms
      presentation deadline without sliding under later input; resize previews retain their 50 ms
      settle because they can require full layout.
- [ ] Bound VGA raster and presentation work with batched cell invalidation, retained scaled-text
      overlays, pixel damage, buffer-age-correct partial copies, and softbuffer resize only on
      physical size change. Damage is at most 32 merged regions with a 50% full-damage threshold.
      *Note:* softbuffer's Win32 backend is a single retained DIB — `age()` is always 1 — so the
      buffer-age/`damage_history` path is effectively dead there and a future cleanup can drop it.
- [x] `:hover`, `:active` and pointer focus are live inputs to the dynamic-state evaluation, gated
      by dependency without touching load progress or status messages.
- [x] `:focus`, `:focus-visible` and `:focus-within` are distinct, with separate selector contracts
      and a pointer/keyboard source ready for M2.
- [x] Theme extension: a distinct hover accent with snapshot goldens. Selected and search-match
      accents stay in M2, where their consumers arrive.
- [x] Inherited CSS `cursor`, including every predefined cursor winit 0.30.13 can represent and
      `none` through native cursor visibility. Custom cursor images use their mandatory predefined
      fallback but are not loaded.

**Slice 3 — chrome affordances (done — smoke pending)**

- [x] **Tab close box** — every whole chip carries a Turbo Vision `[■]` (CP437 0xFE) before its
      right corner; the `+` hint and the clipped `…»` stub carry none. `layout_tabs` owns the
      columns so drawing and hit-testing widen together. `TabManager::close_active` became
      `close(index, fresh)` with a lazily-built `FreshTab`, so any tab closes without dragging the
      selection. Like `Tab(index)`, the close box calls the session directly: no `Action` can name a
      tab index.
- [x] **Page scrollbar** — the content frame's right rail becomes `▲` cap, `▒` track, `█` thumb,
      `▼` cap, always drawn, full-track thumb when the document fits. It takes over the rail rather
      than claiming a column, so `content_cols` is unchanged. `ChromeLayout::scrollbar` is the one
      derivation the painter and the pointer read. Caps and trough dispatch existing `Scroll*`
      actions; the thumb drag is new because no action can name an absolute position, and it
      round-trips exactly because placement rounds down while its inverse rounds up. A drag owns the
      pointer, keeps tracking off the bar, and ends when the pointer leaves the window.
- [x] **Retained composition repaints the bar** — a retained scroll moves a full-width region, thumb
      included, so every frame that touched the content owes the column a repaint.
- [x] **Closing a tab renders its replacement** — `close_tab` never called `activate_current`, so
      the tab that came forward kept showing a stale page.

**Acceptance:** scripted zone/state, loop-budget and stateful render tests green, including the
layout-changing hover fixed-point regression; native and terminal launch confirmation plus the
manual mouse walkthrough remain pending.

### M4 — JS seam (done)

Runtime-neutral `JsEngine`/factory/typed-host contracts, capability-parameterized Noop coverage and
the single checked `MutateOp` invalidation funnel are wired only from the composition root. The `js`
feature remains default-off and runtime `--js=off` always wins. Template-content creation is
idempotent, template scripts stay inert, and host failures surface without entering DOM panic paths.

### M5 — JavaScript and Web APIs (in progress)

- [x] Decision gate: retain Boa 0.22.0. A 600-job Promise fixture is owner-sequence sliced at the
      requested 256-job budget; the loop-iteration ceiling bounds a single synchronous evaluation.
- [x] `BoaEngine` behind the runtime-neutral factory/engine/host contracts; one engine instance per
      loaded document; job pump per tick
      (≤256) on the injected clock; fetch/timer promises resolved from the generation-tagged page
      resource graph; at most 32 script fetches and 256 timers per load; TLA excluded.
- [x] Full executing contract suite under the script feature; scripting-aware `noscript`, inert
      template contents and globals isolation across documents.
- [x] Practical rendering bindings: document/title/location, ID and simple-selector lookup,
      checked DOM tree mutation, attributes/properties, `classList`, inline `style`, console/alert,
      classic external scripts, click handlers with cancellation/bubbling, fetch text/JSON and
      injected-time timeout/interval scheduling. This click-only v0 is the baseline for the open
      browser API slices below.
- [ ] test262 subset runner: pinned checkout under `testdata/test262`, harness files, YAML
      frontmatter, curated slices, xfail manifest by feature, regressions forbidden. Upstream
      reference must be refreshed against Boa 0.22.0 before the runner lands; our slice must stay
      within a documented delta.
- [ ] **Classic script processing.** Implement parser-blocking inline and external scripts,
      `async`/`defer` ordering, `DOMContentLoaded`/`load`, and dynamic script and stylesheet
      insertion through the existing `PageLoad` resource graph. The DOM remains single-owner,
      generation checks reject stale work, and module scripts stay deferred until a profile
      promotes them.
- [ ] **WP-A DOM, event and browser APIs.** Add standards-driven slices for selector collections,
      `querySelector`/`querySelectorAll`, `matches`/`closest`, attributes and reflected properties,
      `classList`/`dataset`, and HTML fragment insertion parsed by html5ever. Add
      `addEventListener`/`removeEventListener`, capture/target/bubble propagation, default actions,
      and the input/change/submit events exercised by reader workflows. Expose `localStorage` and
      `document.cookie` only through the injected M6 browser profile, and route `fetch`/XHR through
      M6's typed, bounded request policy rather than a second network path.
      - Before each API tranche, refresh a capability inventory from current MediaWiki browser
        requirements and the human-run live smoke. Every observed missing global, member or event is
        classified as required, adapted or deferred, then represented by a generic standards fixture
        or applicable WPT before production code uses it.
- **Acceptance:** generic offline fixtures cover ResourceLoader-shaped loading and the required DOM,
  event, storage and network slices; the existing `--js=off` page remains complete. Human-run live
  Wikipedia with JavaScript enabled proves progressive interaction without committing captured site
  JavaScript, and all `--features js` gates are green.

### M6 — Browser platform (in progress)

- [x] **Taffy 0.14 baseline upgrade** — pinned with block-only features, 0.14 leaf-measure callback
      adapted without changing render output.
- [x] **Flexbox** — Taffy owns block, inline and nested flex geometry. The cascade supports
      direction/wrap/flow, grow/shrink/basis/shorthand, order, justify/align, gap/place-content and
      the sizing families with atomic invalid-value handling and blockification through
      `display: contents`. Constraint-keyed deferred atoms share table and nested inline-flex
      formatting across intrinsic measurement and fragment emission; text baselines reach Taffy's
      baseline alignment. Order-modified layout leaves DOM order intact; paint builds a row index
      over one interaction order shared by hover and link activation.
- [x] **CSS custom properties and `var()`** — stable Level 1 custom names and arbitrary token
      values, case-sensitive inherited per-element environments, dependency-cycle invalidation,
      token-boundary-safe fallback substitution, and every supported style, typography, counter and
      generated-content consumer. Computed custom values and substituted declarations are capped at
      2 MiB with bounded component nesting; `ComputedStyle` and `StyleTree` are unchanged.
      *Out of scope:* CSSOM, `@property`, animations, `env()`, Level 2 dynamic names/units,
      `revert-layer`.
- [x] **CSS math, render metrics and signed margins** — bounded Values 4 `calc()`, `min()`, `max()`
      and `clamp()` parsed and type-checked after custom-property substitution, unresolved
      percentages preserved until the owning layout axis is known, physical units resolved through a
      frontend-injected render context (VGA injects its 8×16 bitmap metric with scaled text;
      terminal and dump inject the nominal cell metric). Taffy's `calc` and `strict_provenance`
      features sit behind a layout-owned adapter rather than its zero-resolving built-in tree.
      Reaches size/min/max, margins, padding, insets, gaps, `flex-basis` and `font-size`, with
      signed margins in block, flex, inline and the supported table subset. Unsupported dimensions,
      division by zero, non-finite results, incompatible types and exhausted limits invalidate the
      declaration atomically.
- [x] **Grid** — Taffy-backed block, inline and nested Grid containers; Level 1 template, placement,
      implicit-track, flow, gap and alignment properties and shorthands including legacy `grid-gap`;
      named lines and areas; shared layout-time CSS math; order-modified auto-placement; anonymous
      text, generated content, tables, flex items and `display: contents`; rollback-safe bounded
      interning; topmost paint/hit/link behavior. Math-valued tracks use the shared math store except
      inside auto-repeat, where Taffy's fixed-component contract cannot represent them.
      M1-F owns Grid absolute positioning, aspect ratio, `z-index` and horizontal RTL/logical
      integration. *Explicit limits:* `subgrid`, masonry and vertical writing modes.
- [ ] **Browser profile, HTTP request policy, storage and cache.** One browser-owned profile is
      shared across tabs. Interactive sessions persist non-session cookies and origin-scoped
      `localStorage`; session cookies die on exit, and `--dump` uses an ephemeral profile unless a
      future explicit profile option says otherwise. The terminal adapter performs snapshot I/O
      through the injected profile-store boundary while `app` remains I/O-free.
      - Every fetch carries top-level site, initiator origin, request destination,
        navigation/subresource mode, credentials mode and referrer policy. The shared policy layer
        owns redirects, credential attachment, CORS, mixed-content checks, referrer calculation and
        the WP-A-relevant CSP, Subresource Integrity and `nosniff` response checks; script hosts
        cannot bypass it.
      - The cookie service must enforce RFC6265bis domain/path/expiry rules, `Secure`, `HttpOnly`,
        `SameSite`, name prefixes and a current public-suffix list before any library adapter is
        accepted. Cookie values never enter logs or diagnostics, and `document.cookie` uses the same
        synchronous store and visibility rules as network requests.
      - Origin-keyed `localStorage` has an explicit quota, atomic persistence and controlled
        corruption recovery. HTTP caching implements the validation and freshness subset exercised
        by WP-A with bounded storage and generation-safe delivery.
      - Acceptance uses fake-clock, fake-fetch and temporary-profile contracts for expiry, path and
        site boundaries, SameSite navigation/subresource cases, public-suffix rejection, session
        shutdown, cross-tab visibility, storage quota/recovery, cache validation, redirects, CORS,
        mixed content, referrers, CSP/SRI/`nosniff` and ephemeral dump isolation. No test touches the
        network or the user's real profile.
- [ ] **Images** *(in progress — loading, decoding, layout integration, static SVG and automated
      VGA image preparation green; CSS-pixel precision, responsive sources, replaced-fit painting,
      CSS backgrounds, bounded data sources and human VGA/terminal smoke pending)*. One
      delivery across the shared pipeline and both frontends. Static HTML `<img src>` is the first
      boundary. Animation, lazy loading and cross-page caching remain deferred; the remaining
      static-web contracts are open below.
      - [x] **Shared loading and decoding.** `image` 0.25.10 pinned with default features off and
        only PNG, JPEG, WebP and GIF enabled (GIF and animated WebP expose the first frame only);
        `ratatui-image` 11.0.6 pinned as a terminal adapter with only `crossterm` enabled. `PageLoad`
        owns typed image subresource state without changing `net::Fetch`: URLs normalize against the
        document base, obey the subresource scheme policy, fetch once per page, require 2xx and never
        extend the stylesheet blocking window. An `ImageDecoder` contract returns immutable RGBA
        assets with stable IDs, intrinsic dimensions and revisions; signature and enabled decoder
        support decide the format, not an extension or MIME label. Deliveries in one tick coalesce
        into one render. Failures retain separate rate-limit, transport, address, unknown-format,
        unsupported-format, corrupt-data, resource-limit and unavailable counts; the status line
        calls out rate limiting and format support instead of collapsing every cause into one total.
        One decode worker is injected by the composition root; the terminal adapter owns one bounded
        protocol-preparation worker while VGA samples decoded pixels directly. Queues coalesce by
        work key, hold at most 128 distinct jobs, cancel queued work on navigation, tag results with
        tab/generation/revision/geometry/context, drop stale completions and detach on quit.
        **Budgets:** 128 unique image URLs, 64 MiB fetched bytes and 128 MiB decoded RGBA per page;
        8192 pixels per axis, 8,388,608 pixels and 32 MiB RGBA per image. `image::Limits.max_alloc`
        is set to 64 MiB as defense in depth only, because it is non-strict. Crossing a budget
        refuses that resource and reports one aggregate warning; unlike external CSS, decoded images
        are not discarded atomically.
      - [x] **Image layout integration.** Layout owns raster placements as atomic boxes across inline,
        block, table, flex and grid; fallback-to-decoded and resize reflow preserve anchor, hit and
        clip geometry, float formatting contexts exclude text, and both frontends present the
        reserved rectangle without moving, resizing or paint-masking text.
      - [x] **Native VGA overlay foundation.** Immutable RGBA travels through the framebuffer
        overlay path without encoding. Image and scaled-text overlays share CSS paint order and obey
        content clipping, scroll, partial viewport edges, menu occlusion, retained-surface
        restoration and damage tracking; the cursor still paints last.
      - [x] **Terminal output.** Capability detected once after setup; fixed-size
        `ratatui_image::sliced::SlicedProtocol` variants built off the UI thread so scrolling never
        resizes or encodes during presentation. Protocols are used only for an unobscured rectangular
        placement, falling back per placement to halfblocks for horizontal clipping and occlusion,
        and globally if detection fails. While protocol images are present, scrolling composes fresh
        content rather than trusting the retained shortcut. `--dump` always retains the textual
        fallback.
        A protocol carries its whole picture in one cell's escape and marks the rest
        `CellDiffOption::Skip`; because `FrameComposer` replaces ratatui's diff with its own damage
        tracking, it owes that rule too and must never hand a skipped cell to the backend.
      - [x] **Static SVG decoding.** `resvg` 0.48.1 runs without default features in the existing
        decode worker, preserves the shared size/byte budgets and straight-RGBA fallback contract,
        and keeps external resources and system-font scanning inert.
      - [ ] **VGA image preparation** *(in progress — automated contracts green; human Wikipedia
        VGA smoke open).* Native overlays paint once over
        the actual page background. A bounded frontend worker prepares raster images at their final
        framebuffer size and rasterizes retained SVG sources directly at that size; cache and work
        keys include document generation, surface epoch, asset revision and destination geometry.
        Filtering and compositing preserve premultiplied-alpha invariants, and a failed or pending
        preparation keeps a usable intrinsic-pixel fallback. Interactive and captured VGA output use
        the same preparation policy.
      - [ ] **Responsive sources and replaced-fit painting.** Parse `<picture>`, `srcset` and `sizes`,
        select deterministically from the CSS viewport at an effective density of 1 dppx independent
        of the frontend, and re-evaluate on width changes without stale placements. Implement
        `object-fit` and `object-position` inside the laid-out content rectangle; clipping or scaling
        may not change that rectangle or cover neighboring text.
      - [ ] **CSS background images.** Route bounded static `background-image: url(...)` resources
        through the existing per-page image graph, decoder, generation checks and budgets. Support
        size, position and repeat within the owning box's clip; paint backgrounds below borders,
        content and descendants. Gradients remain deferred until a readability case requires them.
      - [ ] **Bounded data image URLs.** Accept base64 and percent-encoded `data:image/...` sources
        only through the shared decoder and existing raw/decoded budgets, with the same MIME allowlist
        and failure reporting as fetched images; they never become top-level navigation or bypass
        per-page accounting.
      - **Human acceptance:** the representative image corpus and Wikipedia image smoke pass in both
        VGA and terminal without missing in-flow content, overlap or resize/resource divergence.
- [x] **Floats** — Taffy 0.14.0 `float_layout` owns CSS 2 physical placement, clearance and
      shrink-to-fit sizing; source-ordered text wraps through cell-rounded bands, including generated
      clearfixes and legacy HTML `align`/`hspace`/`vspace`/`br[clear]`, with float-aware paint, hits
      and links. M1-F owns horizontal RTL/logical behavior, `z-index` and positioned descendants
      whose containing block crosses the atomic float boundary. *Limits:* rectangular margin boxes
      only; no `shape-outside`, vertical writing or deliberate negative-margin overlap.
- [ ] **Perf gate (in progress; optimization deferred until rendering correctness).** The current
      cold baseline is recorded below and remains authoritative until WP-A static correctness and
      image acceptance close; then rerun `app_render` and set a stage-specific cold target. Do not
      claim the earlier <250 ms commit or <200 ms layout+paint goals as passing while measured cold
      layout remains higher. No owner-sequence work slice may exceed 8 ms, and a warmed paint-only
      interaction must complete through owner application in <16.667 ms p95. Input, DOM and
      JavaScript remain on one
      priority-ordered owner sequence; responses use incremental `encoding_rs` decoding and
      html5ever parsing. One bounded render worker publishes revision-tagged artifacts atomically;
      hard revisions reject stale output, while stylesheet/image arrivals may publish one coherent
      intermediate soft revision before the coalesced latest render.
      - [ ] **Retained paint for dynamic state (implemented; native smoke pending).** Layout caches
            source-to-primitive and row-interval indices. A paint-compatible state change updates
            only primitives whose computed style changed, reconstructs each dirty row from every
            overlapping fill, stroke and fragment in original order, and patches changed rows and
            scaled-text runs against an immutable base display. Revision-tagged
            `Unchanged`/`Patch`/`Replace` publication rejects a mismatched base before PageLoad caches
            are committed; form/resource/theme/geometry/generated-content changes and ambiguous
            source identity retain the full replacement path. Hits, links, images, overlays and
            image assets remain untouched on the layout-compatible path. Owner damage carries at
            most 32 merged document-row ranges; terminal protocol images are recomposed only when
            intersected. Exact fresh-paint equivalence and 1,000 → 10,000 sibling locality cover
            layout primitives, painter contributors, rebuilt rows and owner damage. Stylo may still
            traverse unrelated descendants when an ancestor's `:hover` state changes, but only
            genuinely changed style entries reach layout and paint. The 1,355-row live fixture's
            100-transition release gate measures **0.2 ms restyle p95**, **0.2 ms retained-worker
            p95** and **2.6 ms state-change-through-owner-apply p95**. Native VGA and terminal smoke
            must confirm responsive stationary-pointer scrolling and hover before this slice is done.
      - [ ] **Remaining cold/owner work (in progress).** The five-second initial resource window
            starts at parse completion; stylesheets and images settling inside it produce one
            coherent initial layout. Table measurement is reused across Taffy callbacks and measured
            table output moves into emission. Next move inline and external CSS parsing into a
            bounded owned-value processor, keep blank-cell generation allocation-free and finish
            indexed hit resolution. Dense painted rows stay only if the fixture proves them cheaper
            than a sparse representation. `cargo bench --bench app_render -- --url <URL> --runs 5`
            measures the production-composed App; `--fixture` selects the delayed pinned fixture.
            Correlated timeline diagnostics are already available. The generic production-App
            benchmark's five-run release medians are one initial publication,
            2,070.6 ms to settled first paint, 85 ms cascade, 1,160 ms layout and 98 ms paint on the
            live Linux URL; settled resize is one publication in 1,172.5 ms with 84 ms cascade,
            876 ms layout and 82 ms paint. The
            pinned fixture likewise publishes once, with 1,797.4 ms settled and 807 ms initial
            layout; resize settles in 896.4 ms with 593 ms layout.
            Layout remains the dominant cold-worker cost. The committed fixture is revision
            `1371530035`, SHA-256
            `9f75eb3fe747cd8d2ef786705f3f45b97f071a0b03f77507cdf64143cf6deebd`.
- [ ] Console view (F12) · config file · top-level `data:` URL navigation · optional
      Readability-style reader view.

### M7 — Stylo cascade (done — smoke pending)

[`stylo`](https://crates.io/crates/stylo) 0.20.0 is the sole full and incremental cascade. `StyleTree`
remains the boundary, so `layout`, `paint` and `app` contain no `style::` types. Stylo computes in
CSS px and `css::stylo::map` converts to cells through the injected `RenderContext`, memoised on
`Arc<ComputedValues>` pointer identity.

Each page's retained Stylo session stays inside its render worker. Owned stylesheet source graphs
cross the queue; dynamic and form state patch the mirror through snapshots, Stylo restyle hints and
persistent `StyleTree` stores. The adoption and locality gates are green. The three-page native
VGA/terminal smoke is the only M7 close item; full-worker paint latency remains an M6 concern.

- [x] **S1 — build spike (passed).** `stylo` 0.20.0 builds on Rust 1.90 / Windows MSVC with Python
      3.13.15 as `python`; no Gecko, bindgen, nightly or C++ toolchain, and `pool: None` means no
      rayon threads. Python remains the only operational caveat.
- [x] **S3a — DOM adapter (done).** `css::stylo::dom` mirrors `Document` into an owned arena and
      implements `TDocument`, `TNode`, `TElement` and `selectors::Element` over `Copy` handles, with
      16 tests covering navigation, interning, case sensitivity, attributes, state bits, dirty and
      snapshot bits, `ElementData`, opaque identity and depth truncation. The crate root moved from
      `forbid(unsafe_code)` to `deny`; `tests/unsafe_code.rs` pins `css::stylo::dom` as the only
      module that opts out and constrains every `unsafe` block in it to a Stylo trait call.
      *Two findings worth keeping:* html5ever 0.39 and stylo 0.20 share one `web_atoms` 0.2.6, so
      `LocalName`/`Namespace` need no conversion and no second atom table; and Stylo's own
      `ElementDataWrapper` replaces the `AtomicRefCell<ElementData>` an embedder would otherwise
      hand-roll. The mirror drops comments, PIs, doctypes and fragments — they match no selector and
      inherit nothing — and caps build depth at `MAX_MIRROR_DEPTH` because the source is untrusted.
- [x] **S3b — `Device`, font metrics and the first real cascade (done).** Stylo now resolves real
      computed styles: UA sheet, author override, specificity, inheritance, and `ex`/`ch` through a
      cell-metric `FontMetricsProvider`. The `Device` viewport reuses `CellMetric::viewport_css_pixels`
      so VGA and terminal cannot diverge, and `StyloEngine::new` flushes the `Stylist` internally so
      no caller can observe an un-flushed rule database.
      **Hard constraint found here:** Stylo's style sharing cache requires the element handle to be
      *exactly pointer-sized* — `FakeCandidate` declares `_element: usize` and
      `StyleSharingCache::new` asserts the two cache layouts match. The S3a index-plus-arena handle
      was two words and tripped it at runtime. The mirror is now reference-linked through
      `typed-arena` 2.0.2, which gives every node one lifetime and keeps the links safe shared
      references rather than raw pointers; a test pins the handle width where the message is legible.
      **Defect fixed from S3a:** `has_data`/`borrow_data` claimed data unconditionally, but Stylo
      allocates `ElementData` lazily and `with_default_parent_styles` reads
      `borrow_data().styles.primary()` — an `unwrap` — for any parent reporting data. Resolving a
      child before its parent panicked. Allocation is now tracked and resolution is top-down.
      **Also required:** `thread_state::initialize(ThreadState::LAYOUT)` on every thread touching the
      style system, or `SequentialTaskList::drop` trips a debug assertion. S4/S6 must do this on the
      render worker.
- **Stylo mapping limits and accepted behaviour:**
  - `cap`, `rcap`, `ic`, `ric` length units use Stylo's fallback metrics because the terminal cell
    metrics provider has no cap/ic measurements.
  - **`vertical-align: top | bottom` does not parse.** Stylo's shorthand expands to
    `alignment-baseline`/`baseline-shift`/`baseline-source`, and under `default = ["servo"]`
    `alignment-baseline` accepts only `baseline | middle | text-top | text-bottom`. Those four map
    onto our `VerticalAlign`; the `valign` presentational hint uses `text-top`/`text-bottom`, while
    author CSS loses `top`/`bottom`. Accepted regression.
  - `min-content`/`max-content`/`fit-content`/`stretch` sizes map to `Auto`
    (`CssMaxSize::None`).
  - CIE colour spaces convert to sRGB.
  - Math functions outside our `calc`/`min`/`max`/`clamp` grammar (`round()`, `mod()`, `rem()`,
    trig, `pow()`, `hypot()`) fall back to the property's initial value rather than invalidating the
    declaration, because by mapping time there is no declaration left to invalidate.
  - `list-style-type` naming a `@counter-style` or a `<string>` falls back to `Disc`.
  - `white-space-collapse: preserve-breaks | break-spaces` combined with `text-wrap-mode: nowrap`
    has no `WhiteSpace` variant; the collapse mode wins and `nowrap` is dropped.
  - Elements deeper than `MAX_MIRROR_DEPTH` (512) are absent from the mirror and resolve to
    `ComputedStyle::default()` — `display: inline`. Layout's `MAX_BLOCK_DEPTH` (256) normally cuts
    in first, so this is reachable only through deep inline or non-block nesting.
  - Prefs default-on in stylo's servo build that this roadmap had declared out of scope:
    `layout.css.relative-color-syntax.enabled` and `layout.css.properties-and-values.enabled`
    (`@property`) both parse now.
  - Invalid overflow-position combinations such as `unsafe stretch`, `unsafe baseline` and
    `safe normal` are discarded by Stylo.
  - `AlignFlags` values with no counterpart fold rather than drop: `LEFT`/`RIGHT` become
    `Start`/`End` on the item side (matching what our own parser already did), `LAST_BASELINE`
    becomes `Baseline`, and `align-content: baseline` — valid on the block axis — becomes `Normal`,
    which is all Taffy's `AlignContent` can express.
  - `CssPadding` spells zero twice: `Zero` is the enum default an undeclared edge keeps, and
    `Cells(0)` is what resolving `padding: 0` gives. The mapper emits one spelling uniformly; the
    two are equal in meaning (`CssPadding::cells()` returns `Some(0)` for both) but not under
    `PartialEq`.
  - A `style` attribute uses the document's quirks mode, so unitless lengths are lengths in a
    quirks document.
  - **Resolved:** mirror construction seeds `:checked`, `:disabled` and `:enabled` from
    `core::form`, and retained form mutations patch affected mirrored controls through snapshots.
- **Stylo preference obligations.** Stylo gates properties behind `stylo_static_prefs` booleans that
  no compiler check can catch; the defaults live in `stylo_static_prefs-0.20.0/preferences.toml`.
  **`layout.grid.enabled` defaults to `false`**, and every grid longhand plus `display: grid` is
  gated by it, so `css::stylo::prefs` must set it before any sheet is parsed or the whole M6 Grid
  milestone silently disappears. `counter-reset`/`counter-increment` are gated by
  `layout.unimplemented`, also `false`; S5 enables that broad pref and accepts the resulting Stylo
  semantics, including `zoom` changing computed lengths. Tests pin every newly enabled property
  that affects the mapped terminal style. Servo Stylo does not expose author `counter-set`; HTML
  list-item value overrides remain supported as terminal counter policy.
  Pin: `stylo_static_prefs` 0.20.0, matching stylo's own transitive resolution; exactly one copy may
  be linked.
- [x] **S2 — interface and ownership refactor (done).** Owned recursive `StyleInput` graphs and
      session identities cross the queue; the worker alone owns retained non-`Send` Stylo state.
      `StyleTree` stores are persistent, and blocking tests use the production worker protocol.
- [x] **S4a — whole-tree traversal (done; gate passed decisively).** `RecalcStyle` implements
      `DomTraversal` and `StyloEngine::cascade` runs `driver::traverse_dom(_, _, None)` — the
      sequential breadth-first walk, no rayon pool. On the committed live page, release profile,
      reference machine:

      | | custom cascade | Stylo |
      |---|---|---|
      | cascade | 457 ms | **26.7 ms** |
      | stylesheet parse + rule database | (in the 41 ms parse/discovery) | 13.4 ms |
      | mirror build | — | 8.4 ms |

      **A 17× improvement on the cascade**, styling 7,789 elements out of 15,897 mirrored nodes. The
      gate wanted cascade + mapping under ~130 ms; the cascade alone uses 27 ms of that, leaving
      ~100 ms for the S4b mapper — a wide margin.
      *Scope of the win, stated honestly:* this takes the live page's first render from 2,337 ms to
      roughly 1,900 ms, because layout is 1,798 ms of it. The larger prize is still S6.
      The measurement is `css::stylo::tests::measure_the_live_page_cascade`, `#[ignore]`d because a
      timing assertion in a standing gate row would be flaky; its other half is
      `benches/linux_live.rs`. It cannot be a bench: `css::stylo` is private, and benches see only
      the public surface.
- [x] **S4b — mapper (done).** The full `ComputedValues` → `ComputedStyle` mapping in
      `css::stylo::map`, converting CSS px to cells through the injected `RenderContext` and
      memoised on `ComputedValues` pointer identity. The layout, paint, golden and mapper suites
      cover the sole cascade directly.
      - [x] **S4b-1 — engine prerequisites, value core, scalar families (done).**
            `css::stylo::prefs`; `Lengths` (px → cells per axis, percentages, and `calc()`
            serialised through `ToCss` and re-parsed by `css::math`, because
            `CalcLengthPercentage`'s node is private and probing it would lie about
            `min`/`max`/`clamp`); colours and the `None` sentinel; the keyword and text families;
            `style_tree`'s pre-order walk and the `ComputedValues`-pointer memo.
            *Two rules landed here rather than in S4b-3, because they correct properties this step
            owns:* `opacity: 0` ⇒ `visibility: hidden`, and text-decoration propagation.
            **Contracts a future reader must honour:** the walk descends from the mirror's document
            node — `StyleDom::elements` is unordered and `root_element` sees only the first root —
            and it must stay pre-order, because each element's decorations fold into its
            descendants. The memo key is the `ComputedValues` *data* pointer, never
            `ServoArc::heap_ptr`, which is null for a static `Arc`. A percentage whose basis axis
            differs from its output axis cannot be a bare `Percent`: it carries the column/row ratio
            and must go through the calc store.
      - [x] **S4b-2 — box, flex, alignment and grid families (done).** Display, sizing, edges and
            borders, flex, box alignment, and the grid families that intern through `GridStore`.
            On the committed live page, release profile, reference machine: **14.1 ms to map**
            7,895 elements into 1,844 distinct styles (76.6% memo hits), for **34.6 ms cascade +
            map** against S4a's ~130 ms budget — the custom cascade's equivalent is 457 ms. (The
            element count fell from 7,940 when S4b-3 taught the mirror to read `style` attributes:
            Stylo does not traverse into a `display: none` subtree, and the page hides eleven of
            them inline.)
            **Contracts a future reader must honour:** `border-*-width` is binary, never quantised
            — `BorderSide::width` is `usize::from(px > 0.0)`, and running Stylo's 3px `medium`
            through `cells_from_px` would erase the frame from every box that declares a style
            without a width. Margins and paddings resolve percentages against the **inline** axis
            (`Axes::inline_basis`) while sizes and insets resolve against their own
            (`Axes::same`). A bare `<track-breadth>` is not `minmax(t, t)`: the min side is `auto`
            for `auto` *and* `<flex>`. `GridTemplateData::is_valid` moved to `core::style` because
            both cascades need it and CSS's `<fixed-breadth>` admits `calc()` where Taffy does not.
      - [x] **S4b-3 — form-control policy and the remaining cascade inputs (done).** `reverse`, the
            one mapped value with no CSS property behind it, is applied outside the memo from the
            mirror's `ControlKind`; the mirror now also carries every element's `style` attribute,
            which it had been dropping silently. S4c adds the remaining source-aware
            `legacy_align` projection outside that memo.
            **Contracts a future reader must honour:** the mirror and the engine share one
            `SharedRwLock` and the mirror is built *second* — `StyloEngine::mirror` is the only
            constructor that guarantees both, and `StyleDom::build` is for mirror-only tests. Each
            matters for a reason no call site shows: `Locked::read_with` asserts on a foreign guard
            in every profile, and the sharing cache's `have_same_style_attribute` reaches it for
            every same-tag sibling pair; and `prefs::enable` runs in the engine's constructor while
            Stylo reads preferences at *parse* time, so a mirror built first drops every gated
            property an author wrote inline. Inline blocks are interned by source text, because
            `StyleSource` equality and the rule tree's key are both `Arc::ptr_eq` — a block per
            element is a rule node per element, 290 where 41 will do on the live page — and the
            shared block must never be mutated in place.
- [x] **S4c — UA sheet and presentational hints (done).** Stylo owns the structural/form UA sheet,
      palette link colours at user origin and interned zero-specificity presentational hints at
      `CascadeOrigin::PresHints`; source-aware `legacy_align` stays outside the computed-value memo.
      Unknown input types are text controls, and fourth-level list cycling remains terminal policy.
- [x] **S5 — source graph, media queries and generated content (done).** Stylo discovers and parses
      recursive imports with media/supports/layer context, evaluates media against `Device`, and
      supplies pseudo/marker content to TextSurfer's counter resolver.
- [x] **S6a — invalidation measured (done; the milestone's central claim, confirmed).** Snapshot the
      previous `ElementState`, mark the ancestor chain dirty, re-run the traversal. On the committed
      live page, hovering a link:

      | | custom engine | Stylo |
      |---|---|---|
      | restyle | 105 ms | **0.1 ms** |
      | elements visited | all 7,895 | **26** |

      The 26 are the hovered link's own ancestor chain. This is the difference between walking a
      document and consulting a dependency map, and it is what M7 is now for.
      *The link is the deepest one in the document, chosen in document order.* S4b-3 changed that:
      it had been the first `StyleDom::elements` yielded, which iterates a `HashMap`, so the chain
      length — and therefore the headline number — differed on every run.
      *Read as an upper bound:* the pre-adoption path also produced `ComputedStyle` and cloned the
      whole `StyleTree`; the retained benchmark below includes Stylo mapping.
      *Learned here:* Stylo never clears `has_snapshot` — only `set_handled_snapshot` is ever called
      — so the embedder resets the snapshot bits between restyles or the next one diffs against a
      stale snapshot. And there is **no manual invalidation call**: `DomTraversal::pre_traverse` and
      `note_children` both run `ElementData::invalidate_style_if_needed`, so driving
      `TreeStyleInvalidator` by hand would duplicate the engine.
      Hover is a chain: the link and every ancestor are hovered, so all of them are snapshotted.
- [x] **S6b — dynamic state in production (done).** Static and dynamic `ElementState`, snapshots,
      form patches and Stylo restyle hints drive per-element mapping and damage classification;
      generated-content structure may explicitly fall back to full counter resolution.
- [x] **S7 — flip and delete (done).** Stylo is unconditional and the custom cascade, selector,
      declaration, value and variable modules are deleted. `css/math.rs`, presentational hints and
      counter resolution remain narrow terminal adapters.

**Gates.** Run the eight standing commands plus `cargo test --all-features` for adoption.
Adoption additionally requires: the cascade contract suite green on `StyloCascade`; golden, atlas and
static-WPT output equivalent or re-baselined with written justification; incremental styling equal to
a fresh full Stylo cascade across hover enter/leave, active, focus, focus-visible, focus-within,
`:checked` sibling selectors, inherited and custom-property changes, descendant selectors and
`::before`/`::after`; paint-only hover returning `Paint` with zero layout; the changed style entries
delivered to layout unchanged when unrelated siblings grow 1,000 → 10,000; and warmed release-mode
retained restyle plus mapping under one 16.667 ms frame at p95. Raw Stylo traversal and mapped-entry
counts remain visible separately so timing cannot hide conservative descendant work. No `style::`
type may appear in `core`, `layout`, `paint`, `app` or any render job/result message.

`benches/linux_live.rs` measures 100 warmed enter/leave transitions. The current reference run is
0.2 ms p95 for retained Stylo work and the retained worker, and 2.6 ms p95 through owner application.
Stylo can conservatively visit unrelated descendants when an ancestor's `:hover` state changes;
publication filters that traversal to genuinely changed style entries before layout and paint.

**Pre-adoption baseline retained for comparison:**

| | cascade | restyle | layout | paint | worker |
|---|---|---|---|---|---|
| live page, first render | 457 ms | 0 | **1,798 ms** | 82 ms | 2,337 ms |
| live page, link hover | **0** | 105 ms | 0 | 82 ms | 187 ms |
| pinned fixture, first render | 59 ms | 0 | 144 ms | 83 ms | 287 ms |
| pinned fixture, link hover | **0** | 36 ms | 0 | 65 ms | 101 ms |

**No hybrid engine.** Full and incremental styling both go through Stylo.

## Test infrastructure (standing)

- **Contract suites**, capability-parameterized, run against every trait impl — an interface is
  defined by its tests.
- **Snapshots:** insta over ratatui `TestBackend` buffers (widgets), corpus goldens (fixture pages
  under `tests/fixtures/`), DOM tree dumps. `buffer_string` compares symbols only; style assertions
  use the style-aware helper.
- **Property laws — ten green.** From M1-B: viewport-width monotonicity, painted-row bounds,
  disjoint leaf glyph cells, laminar per-row box families, engine-backed deepest-hit round trip, and
  scroll clamping as a fixed point under arbitrary key sequences. From M1-D tables: generated spans
  never overlap, and a wider viewport never increases height. Grid adds bounded declaration/layout
  completion and auto-fill height monotonicity. `url_fix` and text-field state laws continue from M0.
- **Fakes everywhere:** FakeFetch, fake clock, FakeHost; no test touches the network or the real
  clock. The sole exception is a parent conformance-corpus supervisor's fixed wall-clock watchdog;
  product code and child renderers still use injected time.
- `tests/support/` holds shared corpus helpers; module-local fakes stay inline under `#[cfg(test)]`.
- `--dump` is the scriptable end-to-end harness: fixture in, golden text out.

### Static WPT rendering profiles

A pinned, vendored WPT-derived slice, not WPT's browser runner. One isolated child owns a complete
test/reference graph, every page gets a fresh `PageLoad`, and a fixed ten-second parent watchdog
contains aborts and hangs. The hermetic worker resolves only manifest-listed files below
`https://wpt.test/` and runs parse → cascade → layout → paint with injected zero time, scripting
disabled and production limits. A typed manifest records `run`/`skip`/`xfail`, capabilities, exact
reference relations and the transitive resource allowlist. Only an assertion mismatch may xfail;
unexpected pass, crash, timeout and harness errors fail. Vendoring uses a dedicated
failure-preserving fetch tool with license and SHA-256 membership under `testdata/wpt/`; Rust tests
never use the network.

- [x] **`terminal-cell-v1` pilot** *(done)* — a 100×38 content viewport with terminal 8×16 metrics,
      Paper White light appearance and inert dynamic state. Reftests compare actual ratatui content
      cells, not internal boxes. Pinned at WPT `797589c8452b14ba448ba77819427ff3d743e37f`:
      `box-sizing-003`/`005` against `box-sizing-001-ref`, plus three must-complete crashtests. Of
      24 audited `box-sizing-*` tests, `001`/`026` skip for `z-index`, `007`–`025` for SVG/image
      intrinsic sizing, `027` for `testharness.js`. Result: audited=27, eligible=5, run=5, pass=5,
      xfail=0, skip=22.
- [x] **`vga-pixel-v1` profile** *(done)* — a second typed profile in the same manifest, inheriting
      the hash, isolation, watchdog and resource contracts. It renders test and reference at the
      fixed 100×38 viewport through the real VGA software surface, crops the chrome and compares
      exact RGB with no tolerance; mismatches write actual/expected/diff PNGs below `target/wpt-vga/`.
      Admitted: `css/css-sizing/box-sizing-replaced-001..003.xht` with references and 20 support
      files. Result: audited=3, eligible=3, run=3, pass=0, **xfail=3** — and since the profile
      reports a passing case as `unexpected`, it is a two-way tripwire.
- [ ] **CSS pixel precision for replaced sizing** *(open — found by `vga-pixel-v1`)*. Lengths convert
      to whole cells at computed-value time, so a replaced element's `min-*`/`max-*` reach layout
      already rounded; when a constraint pins one axis, the ratio-preserving other axis is derived
      from the rounded value (`max-height: 85px` → five 16px rows → width 80px where the reference's
      authored `width: 75px` is nine 8px columns). That one-column drift is why the three admitted
      reftests are `xfail`. Closing it means carrying CSS pixel precision into replaced sizing and
      quantising once, at the end; those cases turning into `unexpected` passes is the proof.
- [ ] **Incremental terminal-cell backfill** *(open; non-blocking)*. Import one bounded, fully
      inventoried tranche at a time from supported box, sizing, values, alignment, flex, grid and
      table suites. Existing case statuses may not regress, though the reported raw rate may fall as
      new known failures expand the denominator. Script, server, cross-origin, fuzzy, font/image,
      print/manual, variant, native-widget and viewport-sensitive cases stay excluded until a
      capability-specific decision admits them.

**Honest label:** these are TextSurfer terminal-cell and deterministic-VGA slices, never browser
pixel conformance. Specifications remain authoritative; Ladybird is a manual comparison for
disputed cases.

### Rendering regression atlas

- [x] *(done)* `tests/fixtures/render_atlas.html` is one deterministic offline document split into
      11 stable named panels — `ua-flow`, `inline-state`, `lists`, `tables`, `controls`,
      `search-flex`, `box-layout`, `flex-grid`, `floats`, `images`, `presentational` — covering the
      rendering-special element families and the supported block, table, flex, grid, float,
      form-control and replaced-image constellations. Image requests are answered synchronously from
      fixed RGBA fixtures; no network, worker, clock, OS font or JS is involved. Every panel carries semantic
      and geometry assertions, and a manifest rejects duplicate, missing, oversized or untested
      panels.
      **Inventory:** 36 insta snapshots (33 per-panel styled-cell, 3 whole-document structure) at
      40, 100 and 160 columns, plus 33 exact VGA PNG references under
      `tests/reference/vga/render_atlas/`.
      **Update rules:** ordinary runs compare and never rewrite. Terminal references need insta's
      explicit update mode; VGA references need both the ignored generator and
      `TEXTSURFER_UPDATE_ATLAS=1`. Failures write actual and diff PNGs only below
      `target/render-atlas/`.

**Standing rule — rendering regressions.** Every rendering defect gains a focused minimal
regression, and a visually relevant one also gains or amends an atlas panel. Every newly supported
rendering-special element or layout context extends the atlas manifest before its roadmap item can
be marked done. Compatible standards behavior is backfilled into the pinned WPT profiles
incrementally.

### External conformance corpora

- WPT `html/syntax/parsing/resources/*.dat` (pinned commit `ed37f83e`; the html5lib-tests repo is
  archived and points here) is the M1-A landing gate; the tokenizer suite is informational.
- test262 (pinned commit) at M5: executed through `boa_engine` 0.22.0 with harness files and YAML
  frontmatter honored; per-milestone curated slices with an explicit xfail manifest.
- css-syntax + WPT-selectors corpora are inherited by adopting `cssparser` / `selectors`.
- Corpora live in `testdata/`; HTML parsing uses `tools/fetch-corpus.ps1`, rendering its own
  human-run fetch tool. Both pin exact commits and keep Rust tests offline. Xfail entries name an
  exact case and reason.
- [ ] **Corpus-harness audit follow-up** *(open)* — `DatCase.error_count` is parsed but never
      compared with `ParseOutcome.parse_errors`, so the published 95.16% is tree-output conformance
      only; compare error counts or explicitly justify the exclusion. The attribute-order test named
      for UTF-16 covers BMP names only while `tree_dump` uses Rust scalar-value sorting; add an
      astral-vs-BMP case against the pinned reference serializer.

## Non-goals (locked unless a milestone re-opens them)

WP-A excludes account authentication, editing, watchlists and account preferences; those require a
later signed-in profile. Iframes/`<frame>`, embedded audio/video playback, JavaScript modules and
top-level await, vertical writing, native OS-window title setting and syscall sandboxing remain
deferred. Audio/video resources remain reachable as links or explicit downloads.

Remote `@font-face` and custom families use built-in font fallback, and border radii render as square
corners. Transforms, multicolumn layout and other unexercised platform slices are deferred rather
than permanently rejected and must be promoted when a compatibility profile requires them. “Full
CSS/DOM” is not a useful acceptance target: generic, specification-backed slices exercised by active
profiles and applicable WPT are.
