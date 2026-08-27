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

### Custom-parser audit (2026-08-20, re-checked 2026-08-26)

| Parser | Status | Verdict |
|---|---|---|
| `net/encoding/prescan.rs` — WHATWG sniffing | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode-only; no sniffer exists in the ecosystem |
| `core/url.rs` — `url_fix` | not a parser | **No change** — real parsing delegates to `url`; scheme/host/search heuristics are address-bar UX |
| `css/parser.rs` | library adapter | **Keep** — tokenization delegates to cssparser, selector parsing and matching to selectors |
| `css/parser.rs` — terminal media-query grammar | custom adapter | **Keep narrow adapter** — cssparser owns tokens, blocks and recovery; the adapter evaluates media types plus scripting, color scheme and cell viewport dimensions. css-mediaquery 0.1.1 lacks MQ5 grammar/recovery, LightningCSS has no runtime-context evaluator, rdom-tui excludes `@media`, and Stylo/Blitz/litehtml/Ladybird require replacement DOM/style stacks |
| `css/cascade/{content,counters}.rs` + `css/values.rs` | library adapter | **Keep** — cssparser owns tokenization and recovery; the adapter maps tokenized values onto `ComputedStyle` and the counter engine. Components the terminal cannot render (`url()`, quotes) are refused so the declaration drops whole, per spec |
| Table layout | custom, implemented | **Custom is correct** — Taffy 0.14 has no table algorithm; `super-table` 0.3.0 takes string matrices, `iris-layout` 0.4.0 has no integrated CSS table formatter. Neither supplies anonymous-table fixup, spans, captions, border conflict resolution or nested box layout |
| Presentational HTML legacy values | narrow standards adapter | **Custom is correct** — nothing implements WHATWG's legacy integer/dimension/color algorithms. Keep isolated under `css::presentational`; compare with Ladybird `8baf4260d40dd53cd09c21c868d2bd0625a69149`, WHATWG authoritative |
| `font-size` computed-value grammar | narrow standards adapter | **Keep** — cssparser owns tokenization; the adapter maps supported keywords and length-percentage forms onto the frontend-neutral typography model |
| CSS custom properties and `var()` | custom cascade adapter | **Keep** — cssparser 0.37.0 owns tokens and positions; per-element inheritance, cycles and substitution are cascade behavior. LightningCSS exposes a static build-time map and `muskitty-values` is parse-only |
| CSS math values | narrow standards adapter | **Keep** — `muskitty-css-values` 0.1.0 is parse-only with an independent tokenizer and no computed-value type checking, percentage bases, ceilings or `clamp(..., none, ...)` |
| Declarative refresh content | narrow standards adapter | **Keep** — no crate implements WHATWG's `meta[http-equiv=refresh]` microsyntax (searched 2026-08-26); keep the scanner isolated from navigation policy and cap chains in `app` |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` format; isolated from production code |

### Prior-art audit (2026-08-22, re-checked 2026-08-26)

Conformance is evidence, not reimplementation. Terminal browsers have already settled several
questions we were answering ad hoc.

| Source | What it establishes | Adopted here |
|---|---|---|
| [chawan](https://github.com/sourcehut-mirrors/chawan) (`doc/css.md`) | The terminal CSS contract: author colours contrast-corrected against the background; binary `border-*-width`; `font-weight > 500` = bold; underline/line-through; sub-cell inline margins ignored; overflow-x displays, overflow-y clips; `::before`/`::after` + counters for markers; link hints for keyboard navigation | All locked below. `font-size` ignored is our one departure: the VGA profile scales integer bitmaps, terminal cells stay fixed |
| chawan + [w3m](https://w3m.sourceforge.net/) | A real **table layout** engine is what separates a usable terminal browser from lynx | Delivered in M1-D, ahead of flex/grid |
| lynx · w3m · chawan | Every one ships a **non-interactive dump mode** | `--dump`; doubles as the golden-fixture harness |
| [Blitz](https://github.com/DioxusLabs/blitz) | Mirrors our decomposition — DOM + style + **Taffy for boxes** + a separate text layer | Confirms the architecture; no dependency |
| [image](https://crates.io/crates/image) 0.25.10 | Signature-based raster decoding with explicit format features and decoder limits | The shared decoder for PNG, JPEG, WebP and first-frame GIF, default features off, TextSurfer-owned budgets |
| [ratatui-image](https://crates.io/crates/ratatui-image) 11.0.6 | Terminal-only Sixel/Kitty/iTerm2 output, sliced scrolling, halfblock fallback | The terminal adapter only. VGA blits into its own framebuffer |
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
| M2 — Tabs & keyboard | Link navigation, anchors, titles, error pages, in-page search, forms | (in progress) |
| M3 — Mouse | Zones, wheel, clicks, hover, dynamic pseudo-class state | (in progress) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off | (open) |
| M5 — Boa | Boa 0.21.1 behind the trait; host bindings subset; job pump; test262 slice | (open) |
| M6 — Stretch | Custom properties ✅ · flex ✅ · grid ✅ · CSS math ✅ · images (smoke pending) · floats ✅ · perf | (in progress) |

Cross-cutting: test infrastructure (in progress — static WPT backfill, corpus error-count and
astral attribute-order gaps) · gates (done, local only) · coverage floor (open — optional local,
80% overall / 90% css·layout·paint) · conformance corpora (html5lib tree output 95.16% raw / 100%
with xfail; static WPT crash/reftest pilot complete; test262 at M5).

Test counts at the last green run (2026-08-27): **878 lib · 13 binary · 6 fetch-pipeline · 14
corpus · 48 golden · 3 atlas** with the default VGA frontend, **775 lib · 12 binary · 2 atlas**
with `--no-default-features`; the WPT target adds 6 passing tests (5 without `vga`), plus two
deliberately ignored entries (the child worker and the VGA reference generator).

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
2. Smoke M6 images in both frontends; return to the remaining M2 robustness work afterward.
3. Run the gates before and after; never mark `(done)` with red gates.
4. The manual smoke list (example.com, lite.duckduckgo.com, wikipedia.org) is human-run per
   milestone close and never automated.
5. Live repo: no commits without explicit user confirmation.

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
- DOM never crosses threads. One thread owns all mutable browser state; bounded fetch, image-decode
  and frontend image-preparation workers receive and return only owned immutable bytes, pixels,
  identifiers and value metadata.
- `Document` owns DOM pre-insertion validation and hides indextree; template contents are detached
  fragments, and removal detaches rather than invalidating node handles.
- `StyleTree` carries per-node `ComputedStyle` plus two side tables — pseudo-element boxes keyed by
  `(NodeId, PseudoElement)`, and list markers keyed by node. Generated text is heap-allocated and
  `ComputedStyle` is `Copy`, so the strings live beside it rather than in it.
- No tokio. `boa_engine 0.21.1` is an optional dep behind feature `js`; `--js=off` overrides it.
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
- Boa as the first real JS engine (`(rejected)`: rquickjs — C toolchain / unsafe FFI).
- `ureq` (blocking, rustls native roots) behind `Fetch`; four fixed workers over crossbeam channels,
  superseded jobs cancelled per tab, 10 MiB shared body limit. `mediatype` parses response metadata.
- **Per-load pivot invariant:** any navigation ⇒ `generation++`, fresh `Document` + fresh
  `JsEngine`, scroll reset to top, stale/generation-tagged fetch results dropped.
- JS host mutation funnel: all DOM changes through a single `MutateOp` enum ⇒ one invalidation path.
- Contract suites are capability-parameterized (`Capabilities { executes_scripts, async_host_ops }`).
- Pressure ceilings: JS job pump ≤256 steps/tick, re-layout only on content-width change, scroll
  clamp after every re-layout, errors surface in the status bar; backend I/O errors exit.
- No CI anywhere (`(canceled)` by user). Local gates only.

### Locked — terminal CSS semantics

- **The UA stylesheet is hardcoded Rust** in `css/ua.rs::ua_style`, not CSS text. Revisit only if it
  grows past what a typed function expresses clearly.
- **Colour model:** `ComputedStyle::color` is `Option<Rgba>` with byte alpha; background and border
  colours are `Option<Rgb>`/`BorderColor`. `None` means "the theme decides". The painter composites
  partial foreground alpha against the effective cell background, suppresses fully transparent
  glyphs without changing geometry, then **contrast-corrects** the resolved foreground — fidelity
  never outranks legibility.
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
- **Subresources are same-scheme** (`http`/`https` count as one), so a remote page cannot name
  `file:///…` in a `<link>`. Cross-scheme occurrences count as failed resources.
- **A non-2xx response keeps its body** — a server's own error page is a page — but is never
  accepted as a subresource; a 404 is not a stylesheet.
- Out of scope until a page needs them: relative colours, `background-image`, `line-height`, fonts,
  border-radius, inline-element borders.

### Deferred — decision gates with explicit triggers

- **M5 gate:** if Boa's async (fetch promises / timers / TLA) cannot keep the UI responsive after
  the contract suite + fixture, switch to Deno Core (V8) behind the same `JsEngine` trait.
- Taffy owns block, flex, grid and physical float box calculation; inline formatting uses textwrap
  fragments plus Unicode cell/grapheme libraries because Taffy has no inline layout. TextSurfer
  shapes source-ordered inline content around float bands; table layout remains in-house.
- **Incremental and multi-process rendering** (chawan's model) is deferred, not rejected: our DOM is
  single-thread-owned and the fetch pool delivers whole bodies. Revisit if large-page latency
  becomes a complaint.
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
`Alt+F/N/V/H` dropdowns; raised NC-style tab strip with an aligned active divider; `[‹][›][↻][⌂]`
toolbar wired to per-tab history plus a bordered `URL:` field; context bar; `CHROME_ROWS = 6` with
`MouseZone` mapping Menu 0 / Tabs 1–2 / Address 3–4 / Content 5+.

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

**Deliberate limits:** `z-index`, `clip: rect()`, relative positioning of non-replaced inline boxes,
positioned descendants inside the table-cell formatter, true sticky behavior and a fixed-position
repaint layer remain unimplemented.

## Open milestones

### M2 — Tabs & keyboard navigation (in progress)

Started from the robustness end rather than the keyboard end, because the failure paths were what
the browser did worst. Render robustness, the non-2xx body, the load-status line, declarative
refresh, the designed start page, page screenshots and linear-time DOM child construction are done;
the keyboard and forms work resumes once images close.

- [ ] **Keymap unification** (extends the M0 keymap tests, same file): `Ctrl+L` (+ existing `a`)
      focuses the address bar so `/` is freed; `/` becomes in-page search; `Tab` in the address bar
      moves focus to content; new `FocusTabs` action (`F6`).
- [ ] **`?` help overlay** rendered from the keymap definition as the single source of truth,
      snapshot-tested.
- [ ] **Keyboard link navigation** over the M1-B link list: Tab/Shift+Tab + Enter, focused link
      highlighted, repaint only on target change (one invalidation path, shared with M3 hover),
      per-tab isolation; plus **link marks/hints** (lynx-style numbering) as the discoverable form.
- [ ] **TabManager completion:** page titles from `document.title` with host/URL fallback; in-flight
      loads show the URL. (Ctrl+T/W/N/P cycling exists since M0.)
- [ ] **Anchor links:** `#fragment` → scroll-to-box + status line; no URL rewrite.
- [ ] **`target="_blank"`** links → new tab; per-tab history dedup verified by tests.
- [ ] **In-page search:** `/` opens a prompt (reuses `EditBuffer`), `n`/`N` next/prev with
      scroll-into-view, match highlight distinct from link focus, `x/y` counter, `Esc`/`Enter`
      closes.
- [ ] **Rendered error pages.** *Half landed:* non-2xx responses keep the body, so a server's own
      404 renders with the status in the context bar, and `accepts_stylesheet_response` rejects any
      non-2xx up front. *Open:* the failure screens are still the three-line stub, and the
      unparseable-URL and unknown-scheme paths in `navigation.rs` paint nothing at all — both need
      the real themed page.
- [ ] **Content-type honesty.** A missing or unparseable `Content-Type` currently defaults to HTML,
      so a binary body is parsed and painted as garbage. Sniff (WHATWG minimum: leading `<`,
      BOM/NUL heuristics) and otherwise refuse with the unsupported-type page.
- [ ] **Back/forward without refetching.** A small per-tab document cache keyed by history entry, so
      Back/Forward restore instead of re-issuing a request; the per-load pivot still applies to
      fresh navigations.
- [ ] **Basic forms** *(in progress — controls render and match selectors; nothing is operable)*.
      Target: text/search/hidden/submit, textarea, select, checkbox, radio; GET and
      `application/x-www-form-urlencoded` POST via `url::form_urlencoded`; unsupported
      methods/encodings render a controlled error.
      *Landed:* `core::form` is one model — a `FormState` of **user overrides only**, every unset
      control resolved from its content attributes, so an empty state is exactly the authored page
      and `--dump` needs no seeding. Controls generate their rendering from `layout::replaced`.
      Box-level replaced elements paint from their content rect and take their intrinsic size when
      CSS gives none. `:checked` matches only checkbox/radio/option; `:disabled` reaches through a
      disabled `<fieldset>`/`<optgroup>`.
      *Remaining:* `FormState` mutation and the keyboard/pointer editing model; `:checked` reading
      live state rather than attributes; `FetchRequest` method/body and entry-list serialisation.
- [x] Reload (`R`): generation++, fresh engine + document, scroll top (per-load pivot). *(M1.5)*
- [x] Linear-time construction of new DOM children — indextree 4.9.0's `append_value` for values
      `Document` creates; existing-node attach/insert/move keep validation and checked mutations.
- [x] Reader-facing load status — a rendered document reports that it loaded, not parse-error
      counts or generation numbers; actionable HTTP, fetch, stylesheet and layout failures still
      reach the status bar. *(smoke confirmed)*
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
      four quit paths call `Navigate::shutdown`, which detaches instead of joining.
- [x] Designed start page — `about:blank` is a viewport-aware half-block scene with an exact 78×18
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
robustness and cache items proven by focused tests; manual DuckDuckGo Lite submission works.

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
      a partial edit, content focus. Everything routes through the existing `Action` set — the mouse
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
- [ ] Retain the presented Ratatui buffer and page scene. Pure scrolling moves the content-row
      region and paints only exposed rows. Continuous hover and resize previews defer their
      conservative full render/reflow fallback until 50 ms quiet, so wheel, pointer and resize
      streams cannot repeatedly enter cascade or layout.
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

### M4 — JS seam (open)

- [ ] `JsEngine` + `js` feature wiring in the composition root; runtime `--js=off` wins over the
      feature.
- [ ] `MutateOp` funnel + invalidation-once rule; host subset: document, location, console→status
      buffer, alert→dialog line; no dispatch except `onclick`. Harden the DOM boundary at this first
      non-html5ever caller: repeated template-content creation must not orphan the prior fragment or
      overwrite its mapping, and mutation failures must not reach existing panic paths.
- [ ] Noop contract suite covers the inert set; app behavior byte-identical compiled-off vs on-but-off.
- **Acceptance:** `--js=off` and no-js builds pass identical integration suites.

### M5 — Boa (open)

- [ ] Decision gate: Boa 0.21.1 vs Deno Core on an async responsiveness fixture.
- [ ] `BoaEngine` behind the trait; one engine instance per loaded document; job pump per tick
      (≤256) on the injected clock; fetch/timer promises resolved from the job queue; TLA excluded.
- [ ] Full-suite contract run (script feature); noscript fixture; globals isolation across documents.
- [ ] test262 subset runner: pinned checkout under `testdata/test262`, harness files, YAML
      frontmatter, curated slices, xfail manifest by feature, regressions forbidden. Upstream
      reference: Boa 0.21.1 ≈ 94.12%; our slice must stay within a documented delta.
- **Acceptance:** a fixture page (inline script + onclick + `document.title` + console echo) green;
  no crashes on example.com with JS on; gates green with `--features js`.

### M6 — Stretch (in progress)

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
      *Explicit limits:* `subgrid`, masonry, RTL/writing modes, Grid absolute positioning, aspect
      ratio, `z-index`.
- [ ] **Images** *(in progress — automated work green; human VGA and terminal smoke pending)*. One
      delivery across the shared pipeline and both frontends. Static HTML `<img src>` is the first
      boundary: SVG, animation, `srcset`/`picture`, CSS images, `object-fit`, lazy loading, `data:`
      URLs and cross-page caching remain deferred.
      - [x] **Shared loading and decoding.** `image` 0.25.10 pinned with default features off and
        only PNG, JPEG, WebP and GIF enabled (GIF and animated WebP expose the first frame only);
        `ratatui-image` 11.0.6 pinned as a terminal adapter with only `crossterm` enabled. `PageLoad`
        owns typed image subresource state without changing `net::Fetch`: URLs normalize against the
        document base, obey the subresource scheme policy, fetch once per page, require 2xx and never
        extend the stylesheet blocking window. An `ImageDecoder` contract returns immutable RGBA
        assets with stable IDs, intrinsic dimensions and revisions; signature and enabled decoder
        support decide the format, not an extension or MIME label. Deliveries in one tick coalesce
        into one render.
        One decode worker is injected by the composition root; the terminal adapter owns one bounded
        protocol-preparation worker while VGA samples decoded pixels directly. Queues coalesce by
        work key, hold at most 128 distinct jobs, cancel queued work on navigation, tag results with
        tab/generation/revision/geometry/context, drop stale completions and detach on quit.
        **Budgets:** 128 unique image URLs, 64 MiB fetched bytes and 128 MiB decoded RGBA per page;
        8192 pixels per axis, 8,388,608 pixels and 32 MiB RGBA per image. `image::Limits.max_alloc`
        is set to 64 MiB as defense in depth only, because it is non-strict. Crossing a budget
        refuses that resource and reports one aggregate warning; unlike external CSS, decoded images
        are not discarded atomically.
      - [x] **Replaced layout and shared paint.** Layout consumes image state and intrinsic metadata,
        not pixel buffers. Pending or failed images use the existing fallback geometry; decoded
        assets use intrinsic size and aspect ratio with CSS and legacy constraints. `min-*`/`max-*`
        follow CSS 2.1 §10.4's constraint table, resizing both axes at once, measuring padding and
        border when `box-sizing: border-box` says to, and quantising a natural size the way an
        authored length does. Successful decode reflows only when used geometry changes. Image boxes
        participate in inline, block, table, flex and grid layout, overflow clipping, anchors and hit
        testing. `BoxTree` carries ordered placements; `DisplayList` carries one paint-ordered
        overlay stream for scaled text and images plus an ID/revision-addressed immutable asset store.
      - [x] **Native VGA output.** The adapter nearest-samples only visible target pixels from
        immutable RGBA and alpha-blends through the framebuffer overlay path — no resize allocation
        or encoding. Image and scaled-text overlays share CSS paint order and obey content clipping,
        scroll, partial viewport edges, menu occlusion, retained-surface restoration and damage
        tracking; the cursor still paints last.
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
      - **Human Wikipedia image smoke in both VGA and terminal is the final acceptance gate.**
- [x] **Floats** — Taffy 0.14.0 `float_layout` owns CSS 2 physical placement, clearance and
      shrink-to-fit sizing; source-ordered text wraps through cell-rounded bands, including generated
      clearfixes and legacy HTML `align`/`hspace`/`vspace`/`br[clear]`, with float-aware paint, hits
      and links. *Limits:* horizontal LTR rectangular margin boxes only; no `shape-outside`, logical
      directions/writing modes, `z-index`, deliberate negative-margin overlap, or positioned
      descendants whose containing block crosses the atomic float boundary.
- [ ] **Perf gate:** largest corpus page layout+paint < 200 ms debug. Includes memoizing
      `format_inline`, currently recomputed on every Taffy measure call, again for intrinsic width,
      and again when emitting fragments; it also decides whether `DisplayList` can stay dense by row,
      since the painter allocates one `PaintedRow` for the whole document height even when most rows
      are empty.
- [ ] Persistence/backup · per-history-entry scroll memory · drag input · console view (F12) ·
      config file · `data:` URL scheme · optional Readability-style reader view.

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
  completion and auto-fill height monotonicity. `url_fix`/`EditBuffer` laws continue from M0.
- **Fakes everywhere:** FakeFetch, fake clock, FakeHost; no test touches the network or the real
  clock. The sole exception is the static-WPT parent supervisor's fixed wall-clock watchdog.
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
- test262 (pinned commit) at M5: executed through `boa_engine` 0.21.1 with harness files and YAML
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

Text selection/copy, iframes/`<frame>`, bidi/RTL/writing modes, `line-height`/fonts, border-radius,
inline-element borders, cookies, `addEventListener` DOM events (click-only v0), top-level await,
full CSS/DOM, window-title setting, syscall sandboxing, config files pre-M6, drag input pre-M6.
