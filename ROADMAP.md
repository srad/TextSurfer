# TextSurfer — Roadmap

A terminal text browser in Rust (ratatui). This file is the single source of truth for all work:
architecture decisions, task status, acceptance criteria, test gates, and cross-session handoff.
Update it whenever a decision changes, a task lands, or a flaw is discovered — before writing code
that depends on it.

## Mission (north star)

TextSurfer is a terminal-based web browser: the product is the **browser frontend** — the TUI chrome,
keyboard/mouse interaction, and website **rendering** (DOM → style → layout → terminal paint).
Parsing is a means to an end, never in-house craft: every parseable format goes through a mature,
latest-published crate (html5ever, cssparser + selectors, url, encoding_rs, ratatui, boa_engine at
M5). Custom code is reserved for browser behavior — TreeSink glue, cascade/style tree, layout →
terminal grid, painter, chrome/App/event loop — and for parsing only where the ecosystem provably
has no library. Every hand-rolled parser must be justified against the audit below before it is
written; the audit is re-run whenever a candidate crate appears.

### Custom-parser audit (2026-08-20, re-checked 2026-08-25)

| Parser | Status | Verdict |
|---|---|---|
| `net/encoding/prescan.rs` — WHATWG sniffing: BOM, header, meta prescan, XML fallback, x-user-defined postprocess | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode/encode-only; no sniffer exists in the ecosystem |
| `core/url.rs` — `url_fix` | not a parser | **No change** — delegates all real parsing to the `url` crate; scheme/host/search heuristics are address-bar UX behavior |
| `css/parser.rs` | library adapter | **Keep** — stylesheet/rule/declaration tokenization delegates to cssparser; selector parsing and matching delegate to selectors |
| `css/parser.rs` — terminal media-query grammar/evaluation | custom library adapter | **Keep narrow adapter** — cssparser owns tokens, blocks, delimiters, and recovery; the adapter evaluates media types plus scripting, color scheme and cell viewport dimensions. css-mediaquery 0.1.1 is an immature raw-string port without MQ5 grammar/recovery, LightningCSS has no runtime-context evaluator, rdom-tui explicitly excludes `@media`, and Stylo/Blitz/MusKitty/litehtml/Ladybird require replacement DOM/style/rendering stacks |
| `css/cascade/{content,counters}.rs` + `css/values.rs` — `content`, `counter-*` and `list-style*` value grammar | library adapter | **Keep** — cssparser owns tokenization, functions, blocks and error recovery; the adapter only maps already-tokenized values onto `ComputedStyle` fields and the counter engine. Components the terminal cannot render (`url()`, quotes) are refused so the declaration is dropped whole, per spec, rather than half-rendered |
| Table layout (M1-D) | custom, implemented | **Custom is correct** — Taffy 0.14 implements block/flex/grid and exposes `item_is_table`, but has no table algorithm. `super-table` 0.3.0 accepts string matrices rather than a foreign styled box tree; `iris-layout` 0.4.0 has no integrated CSS table formatter. Neither supplies CSS anonymous-table fixup, spans, captions, border conflict resolution, or nested box layout |
| Presentational HTML legacy values (M1-D) | narrow standards adapter | **Custom is correct** — html5ever owns HTML parsing and cssparser/cssparser-color own CSS syntax, but none implements WHATWG's legacy non-negative integer, dimension, or color-value algorithms. Keep these untrusted-value adapters isolated under `css::presentational`; compare structure and edge cases with Ladybird commit `8baf4260d40dd53cd09c21c868d2bd0625a69149`, with WHATWG authoritative |
| `font-size` computed-value grammar (M1-D) | narrow standards adapter | **Keep narrow adapter** — cssparser owns tokenization, dimensions, percentages, functions and recovery; the adapter maps the supported Fonts/CSS-wide keywords and length-percentage forms onto the frontend-neutral computed typography model. Full font selection remains outside the raster-font scope; M6 CSS math reuses the same token stream and typed evaluator |
| CSS custom properties and `var()` (M6) | custom cascade adapter | **Keep narrow adapter** — cssparser 0.37.0 owns tokens, nesting, escapes and source positions; per-element inheritance, dependency cycles and computed-value substitution are cascade behavior. LightningCSS exposes a static build-time map and `muskitty-values` is parse-only, so neither can supply the runtime element environment. Stable Custom Properties Level 1 is implemented without a new dependency |
| CSS math values (M6) | narrow standards adapter | **Keep narrow adapter** — cssparser 0.37.0 owns tokenization, functions, nested blocks and recovery. `muskitty-css-values` 0.1.0 is parse-only, uses an independent tokenizer, lacks computed-value type checking, percentage-basis evaluation, resource ceilings and `clamp(..., none, ...)`, and therefore cannot replace this bounded evaluator without importing another CSS stack |
| Declarative refresh content (M2) | narrow standards adapter | **Keep narrow adapter** — html5ever owns HTML parsing and `<noscript>` behavior, while url 2.5.8 owns relative resolution. A 2026-08-26 crates.io search found no focused implementation of WHATWG's `meta[http-equiv=refresh]` content microsyntax; keep that scanner isolated from navigation policy and cap automatic chains in `app` |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` fixture format; this stays isolated from production code |

### Prior-art audit (2026-08-22, re-checked 2026-08-26)

Conformance is evidence, not reimplementation — and the same discipline applies to product
behavior. Terminal browsers have already settled several questions we were answering ad hoc.

| Source | What it establishes | Adopted here |
|---|---|---|
| [chawan](https://github.com/sourcehut-mirrors/chawan) (`doc/css.md`) | The terminal CSS contract: author colours **contrast-corrected against the terminal background**; `border-*-width` is **binary**; `font-weight > 500` = bold, `font-size` ignored; `text-decoration` underline/line-through; sub-cell inline margins/padding ignored; overflow-x displays, overflow-y clips, no scrollbars; `::before`/`::after` + counters + `list-style-type` for markers; link markers/hints for keyboard navigation | All locked as decisions below; markers landed in M1-D, link hints scheduled in M2. `font-size` ignored is the one item we depart from: the VGA profile uses cell-aligned integer bitmap scaling while terminal cells remain visually fixed |
| chawan + [w3m](https://w3m.sourceforge.net/) | A real **table layout** engine (colspan/rowspan) is what separates a usable terminal browser from lynx | M1-D, ahead of flex/grid |
| lynx · w3m · chawan | Every one ships a **non-interactive dump mode** | `--dump` in M1-B; doubles as the golden-fixture harness |
| [Blitz](https://github.com/DioxusLabs/blitz) | Mirrors our decomposition — DOM + style + **Taffy for boxes** + a separate text layer (Parley there, textwrap fragments here) | Confirms the M1-B architecture; no dependency |
| [image](https://crates.io/crates/image) 0.25.10 | Mature signature-based raster decoding with explicit format features and decoder limits; Rust 1.88 matches this crate's MSRV | The shared M6 decoder for PNG, JPEG, WebP and the first GIF frame, with default features disabled and strict TextSurfer-owned dimension, pixel and aggregate budgets |
| [ratatui-image](https://crates.io/crates/ratatui-image) 11.0.6 | Terminal-only Sixel/Kitty/iTerm2 output, vertically sliced scrolling, terminal font-size querying and a primitive halfblock fallback; depends on `ratatui ^0.30.1` (we pin 0.30.2) | The terminal adapter only, with default features disabled and `crossterm` enabled. VGA uses its owned framebuffer instead of a terminal protocol |
| Every browser since Firefox 3 | `:visited` must never be observable to page styling | `:visited` parses and never matches (M1-B) |

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
| M1.5 — Chrome redesign | DOS/QBasic rich UI: menu bar, tab strip, toolbar, bordered address field, selectable themes | (complete) |
| M1-B — Style, layout, paint | UA cascade, box model, whitespace, **styled paint seam**, link/hit lists, `--dump`, goldens + laws | (done — user smoke pending) |
| M1-C — External styles | Ordered `<link>`/`@import` loading, selector bucketing, `@media` features | (done — user smoke pending) |
| M1-D — Layout completeness | Table layout, generated content + list markers, length units, presentational attributes, `text-align`, VGA-native bitmap typography | (done — user VGA smoke pending; terminal smoke deferred) |
| M1-E — Overflow and positioning | Element overflow clipping, inherited visibility, CSS positioning, constrained auto-table sizing | (done — human VGA/terminal smoke pending) |
| M2 — Tabs & keyboard | Link navigation, anchors, titles, error pages, start page, in-page search, forms, robustness | (in progress) |
| M3 — Mouse | Zones, wheel, clicks, hover, dynamic pseudo-class state, theme states | (in progress) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off, pure Rust | (open) |
| M5 — Boa | Boa 0.21.1 behind trait; decision gate Boa vs Deno Core; host bindings subset; job pump | (open) |
| M6 — Stretch | Custom properties, flex/grid + conformant floats, images, persistence, scroll memory, console view, config, perf gate | (in progress) |

Test counts at the last green run (2026-08-27): **858 lib · 13 binary · 6 fetch-pipeline ·
14 corpus · 42 golden · 3 atlas** with the default VGA frontend, and **764 lib · 12 binary ·
2 atlas** with `--no-default-features`; the WPT integration target adds 6 passing tests (5 without
the `vga` feature) and one deliberately ignored child-worker entry, and the atlas adds one
deliberately ignored VGA reference generator.
Cross-cutting: test infrastructure (in progress: incremental static WPT backfill, corpus error-count
and astral attribute-order gaps; contract suites, snapshots, proptest and fakes landed) · gates
(done: local only, no CI) · coverage floor (open: optional local, 80% overall / 90%
css·layout·paint) · external conformance corpus (M1-A tree output at 95.16% raw / 100% with xfail;
static WPT crash/reftest pilot complete; test262 at M5).

### Whole-crate audit risk register (2026-08-23)

Confirmed regressions are acceptance items in their owning milestones above. The following static
candidates were not promoted to confirmed bugs without an executable product reproduction:

| Owner | Unconfirmed risk or test gap | Required disposition |
|---|---|---|
| M1-B | `text-decoration` accepts known tokens from an otherwise-invalid value; inline edge cells take the parent run style; overwriting one cell of a wide glyph clears ownership/text but can retain the old style | Add focused cascade/paint cases before changing behavior; close as disproved if no reachable layout producer can expose it |
| ~~M1-D~~ | ~~Non-inherited background ownership on pseudo boxes lacks adversarial coverage~~ | **Closed 2026-08-23 as disproved.** A pseudo box does start from the originating element's computed style, `background` included, but it can never paint a cell that element did not already paint: generated content is inline-level and the outside marker's field is reserved inside the item's own box. Even a pseudo declaring `background: initial` — transparent in CSS — renders the item's background, which is what CSS requires. Pinned by `pseudo_boxes_never_own_a_background_their_element_did_not_paint` in the public render harness |
| M2 | The address edit buffer is global across tab switches; cursor placement and toolbar writes lack sub-24-column coverage | Resolve with the per-tab-state, tiny-chrome and link-navigation tests already owned by M2. M1-E closed the former link/hit clipping gap by clipping layout boxes and fragments before link rectangles are derived |
| ~~M2~~ | ~~`Document::insert_element` is quadratic in depth~~ | **Closed 2026-08-26 as confirmed and fixed.** indextree 4.8.1's `checked_append` walked every ancestor to reject a cycle, so a 100,000-deep chain measured 49 s. `Document::append` now uses indextree 4.9.0's constant-time `append_value` only for values it creates; existing-node attach, insert and move operations retain validation and checked mutations. The isolated 100,000-node worker completes under the ten-second supervisor (0.26 s for the focused parent test), direct tree and cycle cases pass, and the complete feature gate matrix is green |
| ~~M1-B~~ | ~~A block-level replaced element paints nothing~~ | **Closed 2026-08-26 as confirmed and fixed.** `<img style="display:block">` rendered nothing at all: the display branches were tested before the `img` arm, so a block-level replaced element became a block box that then recursed into children it does not have. Found while giving form controls a box, because `input { display: block }` is ordinary CSS and hit the identical path. Box-level replaced elements now generate their content instead of recursing. Pinned by `a_block_level_image_still_renders_its_alt_text` |
| ~~M2~~ | ~~Generated content on a bordered box paints at the wrong origin~~ | **Closed 2026-08-26 as confirmed and fixed.** `FlowBox::inline` is emitted at the *border-box* origin, which is sound only because the boxes that carry one are anonymous and have no border or padding — real text reaches `flush_inline`, which wraps it in exactly such a child. Putting a control's stand-in directly in a real box's `inline` broke that invariant and painted Wikipedia's search field over its own `┌───`; where the box also had `overflow: hidden`, the text fell outside its own padding box and was clipped away entirely. Replaced boxes now paint from `content_rect`, and the invariant is stated on the field. Pinned by `a_bordered_control_paints_inside_its_border_not_on_it` and `a_control_with_overflow_hidden_still_shows_its_label` |
| M4 | Template-content replacement is not exercised by html5ever | Exercise it at the first mutation-capable DOM caller and reject orphaning/overwriting behavior |
| M6 | Extreme injected `Size` values can make the start page allocate `cols × rows × 2`; painter output remains dense by document row; inline-precise hover adds roughly one linear-scanned hit region per text fragment | Put explicit resource ceilings, sparse-vs-dense evidence and indexed paint-order hit/activation resolution behind the perf gate |
| Test infrastructure | `tree_dump` is recursive on untrusted depth; UI clipping walks scalar values rather than grapheme clusters | Add bounded-depth and emoji/ZWJ cases; these do not currently establish a product crash |

Two candidates were closed during the audit: normal-flow `white-space: nowrap` clips rather than
wraps in the public dump harness, and `PageLoad`'s monotonic resource IDs mean the pool's silent
duplicate policy has no demonstrated current data loss (the API/diagnostic gap remains in M2).

## Gates (local only — CI deliberately refused)

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test
cargo test --features js          # M4+; must pass, boa feature compiles
cargo test --features vga         # explicit framebuffer frontend gate
cargo test --no-default-features  # frozen terminal compatibility profile
```

First snapshot write: `$env:INSTA_UPDATE = "always"; cargo test`. Coverage (optional):
`cargo llvm-cov --workspace` (needs `rustup component add llvm-tools-preview`; no thresholds enforced).

## Session handoff

1. Read this status board + Roadmap updates log (bottom).
2. Smoke M6 Images in both frontends; return to the remaining M2 robustness work afterward.
3. Run the gates before and after; never mark `(done)` with red gates.
4. Manual smoke list (example.com, lite.duckduckgo.com, wikipedia.org) — run by the human per
   milestone close; never part of automated tests.
5. Live repo rule: no commits without explicit user confirmation.

## Architecture (as-built)

```
 main.rs — frontend adapter only: CLI · VGA/terminal selection · event mapping and loops
   ▼
 ui (ratatui widgets · Focus · keymap · mouse zones) ──┐
   Action (Load, NewTab, ActivateLink, Scroll, …)     │ UiEvent
   ▼                                                  │
 app — I/O-free composition root · TabManager · event loop · fetch workers · js pump
   │ dispatch                                         ▼
   ▼                                     script: JsEngine trait + JsHost ← per-loaded-document
 pipeline — PageLoad stylesheet resource graph · render facade · --dump
   │                                                  │
 net::Fetch ⇒ html::HtmlParser ⇒ core(indextree DOM)   │ noop.rs · boa.rs (feature "js", default off)
   │        │
 css: CssParser(cssparser) → Cascade(selectors) → StyleTree (+ pseudo boxes · list markers)
 layout::LayoutEngine → BoxTree (absolute coords, unbounded height, styled text fragments)
 paint::Painter → DisplayList (styled spans · NodeId hit-tags · link rects) → ui content widget
```

- Single crate, modules: `core`, `net`, `html`, `css`, `layout`, `paint`, `script`, `pipeline`,
  `ui`, `app` + thin `main`.
- Cross-module boundaries via traits; only `app`/`main` know the concrete implementations (composition root).
- `app` is the composition root **only** — controller, `Tab`/`TabManager`, the `Navigate` adapter and
  the start page. The rendering pipeline (`PageLoad` resource graph, the cascade→layout→paint facade,
  `--dump`) is `pipeline`, so `main` and the golden tests never import into the composition root.
- **Module structure:** a module is a responsibility, not a file. See the AGENTS.md "Module
  structure" rules for when a `foo.rs` becomes a `foo/` directory and what `mod.rs` may contain.
- `app` never imports crossterm: events come in as `core` types, results via `deliver_fetch`, time injected.
- DOM never crosses threads. One thread owns all mutable browser state. Bounded fetch, image-decode
  and frontend image-preparation workers receive and return only owned immutable bytes, pixels,
  identifiers and value metadata.
- `Document` owns DOM pre-insertion validation and hides indextree; template contents are detached
  document fragments, and DOM removal detaches rather than invalidating node handles.
- The `DisplayList → ui content widget` edge lands in M1-B. Before it, the painter's hit and link
  lists were computed and discarded, and the content widget drew one flat colour.
- `StyleTree` carries per-node `ComputedStyle` plus two side tables — resolved pseudo-element boxes
  keyed by `(NodeId, PseudoElement)`, and list markers keyed by node. Generated text is
  heap-allocated and `ComputedStyle` is `Copy`, so the strings live beside it rather than in it.
- No tokio. `boa_engine 0.21.1` optional dep behind feature `js`; runtime `--js=off` overrides the feature.
- Crate pins: report actual versions used in the updates log at each dependency milestone.

## Decisions log

### Locked — product and architecture
- UI design language: classic DOS/QBasic text-mode rich UI (grey menu bar and context bar, boxed
  panels, bordered input field, raised tab strip, Turbo Vision palette on a blue desktop) around a
  normal browser layout shell — decided by the user; `ui::Theme` centralizes all colors, the painter
  and every widget draw from it; no color literals outside it. The View menu selects one global,
  session-scoped theme from Turbo Vision (default), Norton, Amber CRT, Green Phosphor and Paper
  White. `App` owns that choice and maps its dark/light appearance into
  `prefers-color-scheme`; persistence remains with M6 configuration.
- Product = UI, VGA-native bitmap rendering and the terminal compatibility frontend; every other
  layer adopts a mature, latest-version
  crate: html5ever (HTML parse incl. tree-builder), cssparser + selectors (syntax + selector
  matching), boa_engine (ECMAScript), ureq (HTTP), encoding_rs (decode), ratatui (widgets/TUI),
  Taffy (block box geometry), textwrap + unicode-segmentation/width (inline formatting).
  In-house scope: Document arena + TreeSink glue, cascade → style tree, layout → cell grid,
  paint/DisplayList, chrome/App/event loop, and table layout (M1-D — audited crates do not integrate
  with a styled CSS box tree).
- Single crate with modules (not a workspace) — fastest iteration; workspace split trivial later.
  Re-affirmed unchanged by the 2026-08-23 structure pass.
- **`app` is the composition root only; `pipeline` is the rendering subsystem; `main.rs` is the
  frontend adapter only.** `PageLoad`'s stylesheet resource graph, the cascade→layout→paint facade
  and `--dump` are product pipeline stages, not composition wiring — keeping them in `app` forced
  `main`, the golden tests and unit tests in `css`/`layout` to import into the composition root.
  `pipeline` sits above `paint` and below `ui`/`app`; it takes its palette by injection rather than
  reaching up to `ui::theme`.
- **A module is a responsibility, not a file.** Directory modules with one submodule per job, the
  public surface in `mod.rs`, tests in a sibling `tests.rs` or `tests/` directory. Rules live in
  AGENTS.md "Module structure"; line budgets are a prompt to look for a second responsibility, not
  a defect threshold.
- Native text renderer (lynx/w3m/chawan family), not embedded-engine (carbonyl/browsh family) — our
  value is small footprint + cell-native layout.
- **Two frontends over one engine, behind ratatui's `Backend` trait** (2026-08-23). `ui::chrome::draw`
  takes a backend-agnostic `Frame` and `app` never imports a terminal library, so a second frontend
  costs a `Backend` impl and an event-mapping adapter — nothing in `css`/`layout`/`paint` moves. The
  VGA frontend is the active default and owns the native bitmap typography path. The terminal
  frontend is frozen as a compatibility fallback selected with `--terminal`; it remains available
  in no-default-feature builds and its existing non-image rendering must stay byte-for-byte stable.
  *Why a window at all:* the DOS look is mostly the font, and inside a terminal emulator the font
  belongs to the user — `ui::Theme` fixes the palette but every glyph renders in whatever face the
  terminal was configured with. Owning a framebuffer is the only way to own the face, the cell metric
  and the palette together. It also doubles the usable columns (1280x800 = 160x50) and makes image
  support in the default VGA frontend a native blit; only the terminal compatibility frontend needs
  the Sixel/Kitty/iTerm2 capability matrix.
  *Rejected alternatives:* `mousefood` 0.5.2 (ratatui-org, embedded-graphics backend) — solves the
  easy part, and its fixed-width `MonoFont` model cannot express Unifont's 16x16 wide glyphs across
  two cells; `ibm437` 0.5.0 (MIT, softbuffer-ready) — ships 8x8 and 9x14 only, and 8x16 is forced by
  Unifont's narrow metric; `minifb` 0.28 — simpler, but a polling keyboard model that handles
  modifiers and text input poorly.
- **Font tiers are CP437 first, then Unifont** (`vga`). CP437 wins wherever it has a glyph, so the
  chrome stays authentically DOS; Unifont covers the rest of the BMP, which the web needs — curly
  quotes, en/em dashes and every non-Latin script fall outside the code page, and without the second
  tier most real pages would render as replacement boxes. Both are 8x16 (Unifont's wide glyphs are
  16x16 = exactly two cells), so the tiers share one cell metric and one blitting loop.
  *Licensing:* the CP437 bitmaps are generated from pcface's **Modern DOS 8x16** set, which is MIT or
  CC0; pcface's *Oldschool PC* bitmaps are GPL/CC-BY-SA and are deliberately not used. Unifont
  arrives via `unifont-bitmap` (crate MIT/Apache-2.0; font data OFL 1.1 / GPLv2+ with the font
  embedding exception).
- **`src/vga/font/table.rs` is generated and pinned.** `tools/gen_cp437.py` re-derives it from
  upstream at a pinned commit and refuses to write unless its own re-render matches pcface's
  published `glyph.txt` byte for byte; the emitted SHA-256 is asserted by a test, so a regeneration
  that changes any glyph fails a gate rather than silently altering every frame.
- CSS box model from day one (`(rejected)`: lynx-style linear flow — user chose box model).
- `cssparser` + `selectors` for CSS (`(rejected)`: lightningcss — no selector matcher, orphan AST vendor).
- `html5ever` for HTML with our direct `TreeSink` building an `indextree`-backed `Document`;
  `scraper`/ego-tree/RcDom rejected (none meets the mutable multiple-root, detached-template,
  future-JS contract).
- Boa as first real JS engine (`(rejected)`: rquickjs — C toolchain / unsafe FFI; both sealed by trait).
- `ureq` (blocking, rustls native roots) behind `Fetch`; four fixed workers use crossbeam channels,
  the app cancels superseded pending jobs per tab, and both HTTP/file bodies share a 10 MiB limit.
  `mediatype` parses response metadata.
- Per-load pivot invariant: any navigation ⇒ `generation++`, fresh `Document` + fresh `JsEngine`
  (globals never survive), scroll reset to top, stale/gen-tagged fetch results dropped.
- JS host mutation funnel: all DOM changes via a single `MutateOp` enum ⇒ one invalidation path.
- Contract suites are capability-parameterized (`Capabilities { executes_scripts, async_host_ops }`).
- Pressure ceilings: JS job pump (≤256 steps/tick), re-layout only on content-width change, scroll
  clamp after every re-layout, errors surface in the status bar; terminal/backend I/O errors exit.
- No CI anywhere (`(canceled)` by user). Local gates only.

### Locked — terminal CSS semantics (M1-B, prior-art aligned)
- **UA stylesheet is hardcoded Rust** in `css/ua.rs::ua_style`, not CSS text. Note the file history:
  an entry claiming it "ships as CSS text in `css/ua.rs`" described a file that never existed and was
  corrected 2026-08-22 to `css/cascade.rs::ua_style`; the 2026-08-23 structure pass then created
  `css/ua.rs` for real, still holding hardcoded Rust. Revisit only if the UA sheet grows past what a
  typed function expresses clearly.
- **Colour model**: `ComputedStyle::color` is `Option<Rgba>` with byte alpha while background and
  border colours stay `Option<Rgb>`/`BorderColor`; `None` means "the theme decides", so the selected
  palette supplies the default field and author colours override only where declared. The painter
  composites partial foreground alpha against the effective cell background, suppresses fully
  transparent glyphs without changing their geometry, then **contrast-corrects** the resolved
  foreground (chawan's rule) — faithfulness never outranks legibility.
- **Borders are binary**: any non-zero border width paints one box-drawing frame. Replaces the
  earlier "borders ≥2 cells doubled" phrasing, which nothing implemented and which contradicts the
  prior-art model.
- `font-weight > 500` = bold; `text-decoration` maps to underline/line-through; reverse video is
  available as a style bit. `font-size` computes in every frontend. Terminal cell rendering keeps
  its historical one-cell appearance; VGA uses the computed size for native bitmap scaling.
- Sub-cell margins and padding are ignored on inline boxes; CSS lengths are capped at 65,535.
- Supported absolute, font-relative and viewport-relative lengths resolve through the shared 8×16
  cell metric. The terminal profile keeps the fixed 16px/8px font approximations; the VGA bitmap
  profile uses the element and root computed font sizes for `em`/`ex`/`ch` and `rem`. Layout rounds
  per axis while media queries compare unrounded CSS pixels.
- The viewport clips the x axis and leaves the document y axis unbounded. Element `overflow`
  supports `visible | hidden | clip | scroll | auto`: a specified `visible` axis computes to
  `auto` when the other axis is scrollable (`hidden | scroll | auto`), while `clip` stays distinct.
  Element clips use the padding box. `scroll` and `auto` create no terminal scrollbar and render as
  clipped scroll containers; the root/body values are propagated to the viewport rather than
  clipping their own boxes.
- Positioned layout supports `static | relative | absolute | fixed | sticky` and signed inset
  lengths. Absolute descendants resolve against the nearest positioned ancestor's padding box;
  fixed descendants resolve against the viewport. `sticky` degrades to `relative`, stacking remains
  source/depth order without `z-index`, and negative origins clip rather than translating content.
- `:visited` parses and **never matches** — page styling must not observe history.
- Colour **values** are parsed by `cssparser-color` 0.5.0 (same cssparser 0.37 pin), so keywords,
  hex, `rgb()/rgba()`, `hsl()` and `hwb()` all work; CIE spaces (`lab`, `lch`, `oklab`, `oklch`)
  parse but do not convert to sRGB, so such declarations are ignored rather than guessed.
- Out of scope for now, revisit when a page needs them: relative colours, `background-image`,
  `line-height`, fonts, border-radius, inline-element borders.
- **`opacity` is honoured only at `0`, where it computes to `visibility: hidden`.** The two have
  identical layout behaviour — geometry retained, nothing painted — so this rides machinery that
  already exists rather than adding an alpha layer. It earns its place because `opacity: 0` over a
  styled box is how the web builds a custom control; without it Wikipedia's three hidden dropdown
  checkboxes paint over the article chrome. Two divergences follow and are accepted: a descendant
  declaring `visibility: visible` reappears, which CSS forbids, and the element stops being
  hit-testable, where a browser keeps an `opacity: 0` overlay clickable. Any other value renders
  fully opaque.
- **A replaced element keeps room for its own rows against a smaller `max-height`/`height`.**
  A 1px border costs a whole cell here, so an author who budgets `max-height: 2rem` for a bordered
  32px field leaves us zero content rows and the field renders blank. The quantisation is ours, not
  the author's; honouring the number would be faithful to it and not to the intent. Implemented as
  a `min_size` on the height axis only, which works because CSS resolves min over max. Width is
  left freely settable, since that is where the quantisation does not bite.
- **A form control is a replaced element, not a box of text.** Its rendering is generated to fit
  the box it ends up with — from `content_rect`, never from `FlowBox::inline`, which is emitted at
  the border-box origin and is reserved for anonymous boxes. Brackets delimit a control only when
  nothing else does; a border of its own replaces them. The field's extent is carried by reverse
  video rather than a filler glyph, so a value can never be mistaken for padding. An `<img>` is the
  exception: its `alt` is the author's prose, so it wraps inside the box like ordinary text.
- **Generated content is inline-level**: `display` on a pseudo-element is not honoured, and
  `list-style-type` accepts keyword counter styles only — no `@counter-style`, no string types.
- **List markers are outside markers** by default: the item reserves a field on its left, shared
  and right-aligned across sibling items so numbers meet one text column, and wrapped lines align
  under the item text. `list-style-position: inside` renders the marker as ordinary inline content.
  `ul`/`ol` add no indent of their own — nesting indents because each level starts after its own
  marker field.
- **Subresources are same-scheme**: a document may only fetch stylesheets from its own scheme
  (`http`/`https` count as one), so a remote page cannot name `file:///…` in a `<link>` and have
  the browser read local files for it. Cross-scheme occurrences count as failed resources.

### Deferred — decision gates with explicit triggers
- M5 gate: if Boa's async (fetch promises / timers / top-level await) can't keep the UI responsive
  after contract suite + fixture, switch to Deno Core (V8) — same `JsEngine` trait.
- Taffy owns block, flex and grid box calculation; terminal inline formatting uses textwrap fragments +
  Unicode cell/grapheme libraries because Taffy has no inline layout. M1-D adds table layout
  in-house; floats wait for line-flow-around-float conformance.
- **Incremental and multi-process rendering** (chawan's model: paint while the body streams, one
  process per buffer) is deferred, not rejected: our DOM is single-thread-owned by design and the
  fetch pool delivers whole bodies. Revisit if large-page latency becomes a complaint.
- Syscall-filter sandboxing (chawan on Linux/BSD) is a **non-goal** — Windows is the primary target.
- A Readability-style reader view is recorded as a candidate differentiator (M6), not a commitment.

## Shipped milestones

Condensed once their work landed; `(done)` here means the code and gates are green but the human
terminal smoke on the manual list has not been signed off yet.

### M0 — Foundations (complete)
Scaffold, `core` types, every module trait with capability-parameterized contract suites against
fakes, `ui` widgets (TabBar/Address/Content/Status) with TestBackend snapshots, `ui::keymap` with
focus-aware routing, `ui::mouse` zone model, I/O-free `App` + `TabManager`, and a poll/draw loop.
*Acceptance met:* `cargo run` draws the chrome, `q` quits, keys never panic, all gates green.

### M1-A — Parse pipeline (done — user smoke pending)
html5ever 0.39 tokenizer + tree-builder through our own `TreeSink` into the `Document` boundary;
WHATWG encoding sniffing (BOM → header → meta prescan → UTF-8); `UreqFetch`/`FileFetch` behind
`Fetch` with a 4-worker pool, timeouts, redirects and generation tagging; `<base href>`; scheme
routing; tree-dump snapshots.
*Acceptance:* 24 parse fixtures + WPT html5lib corpus at **1829/1922 raw (95.16%)**, 100% with an
explicit xfail manifest, zero panics; the composition root is verified end-to-end hermetically. The
human terminal smoke is still outstanding, so this milestone is `(done)`, not `(complete)`.

### M1-R — Stabilization (complete)
indextree-backed DOM behind the existing `Document` boundary (detached template fragments, quirks
mode, checked pre-insertion, document-order ID lookup, iterative traversals); fetch delivery routed
by stable tab ID + generation; 10 MiB body limits; mediatype classification; usize scroll/extents
with width-only invalidation; ratatui-managed terminal lifecycle + clap CLI; grapheme-safe editing;
corpus floor counting raw passes only with mandatory manifest hashes.
*Acceptance met:* every targeted regression test and local gate green; dependency audit clean apart
from one recorded unmaintained transitive (`paste`, reachable only through optional Boa).

### M1.5 — Chrome redesign: DOS/QBasic rich UI (complete)
`ui::theme` with a zero-literal rule and five session-selectable retro palettes; full-width grey
menu bar (row 0) with working `Alt+F/N/V/H` dropdowns; raised NC-style tab strip with an aligned
active divider; toolbar with
`[‹][›][↻][⌂]` buttons wired to per-tab history plus a bordered `URL:` field; full-width grey context
bar at the bottom; `CHROME_ROWS = 6` with `MouseZone` mapping Menu 0 / Tabs 1–2 / Address 3–4 /
Content 5+; browser-like startup focus on the address field.
*Acceptance met:* eyeball sign-off on the QBasic restyle; every widget and full-chrome golden
reviewed individually; all gates green.

## Open milestones

### M1-B — Style, layout, paint (done — user smoke pending)

Landed: cssparser 0.37 + selectors 0.40 adapters with specificity and structural matching; first
`Cascade` (UA + embedded author + inline `style`, `!important`, source order); type-only `@media`
with injected screen context and bounded diagnostics; temporary degradation contracts
(table/flex/grid→block before their owning milestones,
position→static, percentage heights→auto); Taffy 0.14 block geometry with anonymous boxes, margin
collapse, padding, one-cell borders, fixed widths and content-box/border-box; six inheriting
`white-space` modes over node-owned textwrap fragments; sparse paint with border glyphs.

Items:

- [x] **Styled paint seam.** `ComputedStyle` gained `color`/`background`/bold/underline/strike/reverse
      (`color: Option<Rgba>`, `background: Option<Rgb>`, `None` = theme); `InlinePiece`,
      `TextFragment` and `LayoutBox` carry the
      resolved `CellStyle` from the inline ancestor chain; `DisplayList` is styled spans + hit tags +
      link rects; paint order is backgrounds bottom-up → borders → clipped text; `legible_foreground`
      contrast-corrects author foregrounds against the effective background. *Proven by*
      `author_colors_weight_and_decoration_reach_the_computed_style`,
      `backgrounds_paint_under_text_in_depth_order`,
      `unreadable_author_colours_are_corrected_towards_the_theme_text`. The audit repair retains byte
      alpha through the style seam; at paint time, transparent text becomes same-width blank cells
      and partial alpha composites against the deepest painted background before contrast
      correction. Geometry, hit regions and link rectangles do not change. *Proven by*
      `foreground_alpha_survives_supported_color_forms_and_inheritance`,
      `partial_foreground_alpha_resolves_against_the_deepest_background`,
      `transparent_text_loses_ink_but_keeps_layout_and_interaction_geometry` and
      `foreground_alpha_survives_the_complete_render_path`. (done)
- [x] **Dynamic pseudo-classes parse.** `DynamicPseudoClass` (`:link`, `:any-link`, `:visited`,
      `:hover`, `:focus`, `:active`, `:checked`, `:enabled`, `:disabled`) replaces the uninhabited
      enum and is evaluated against an injected `DynamicState`; `:link` uses `is_link()`, `:visited`
      never matches, form states read attributes, and hover/focus/active stay inert until M2/M3.
      *Proven by* `dynamic_pseudo_classes_parse_instead_of_invalidating_the_whole_selector_list`,
      `visited_never_matches_so_page_styling_cannot_observe_history`,
      `dynamic_pseudo_class_rules_apply_instead_of_being_discarded`. (done)
- [x] **Seam reaches the screen.** `Tab` holds the `DisplayList`; the content widget renders spans
      with theme fallback plus bold/underline/crossed-out/reversed; scroll extent and clamp read the
      painted row count. *Proven by* `span_colours_and_modifiers_reach_the_terminal_buffer` and the
      scroll-clamp law. (done)
- [x] **Line-break opportunities.** The `!is_ascii()` heuristic is replaced by width-and-category
      rules, so accented Latin, Cyrillic and Greek words stop breaking mid-word while wide scripts
      still wrap. *Proven by* `accented_words_never_break_mid_word_while_wide_scripts_still_wrap`. (done)
- [x] **Pager keys.** PageUp/PageDown move a full page; Space/`b` page down/up in content focus;
      the M0 keymap test was extended, not replaced. *Proven by*
      `paging_keys_move_a_screen_at_a_time_not_a_line`. (done)
- [x] **`<img>` and `<hr>`.** `<img>` renders `[alt]` when nonempty, nothing for the deliberate
      empty or whitespace-only fallback, and `[img]` only when `alt` is absent; `<hr>` fills the
      content width. One layout-private resolver is shared by normal flow and table cells. *Proven
      by* `images_render_their_alt_text_and_rules_span_the_content_width` and
      `image_alt_fallbacks_match_inside_table_cells`. (done)
- [x] **`--dump`.** Non-interactive `fetch → parse → cascade → layout → paint → stdout` at `--cols`
      (default 80), no terminal; the shared render path moved into `app::render`, so the TUI and the
      dump cannot diverge. *Proven by* `dump_mode_prints_the_same_page_the_painter_produced`. (done)
- [x] **Goldens + laws.** `tests/fixtures/` holds margins, headings, borders, links, wide-character
      and `pre` pages; `tests/render_goldens.rs` asserts their goldens plus link geometry, contrast
      correction, viewport bounds and dump equivalence. The four remaining proptest laws (leaf glyph
      cells disjoint · boxes laminar per row · engine-backed deepest-hit round trip · scroll clamp
      fixed point) are green beside the original two. (done)
- Conformance note: css-syntax + WPT-selectors corpora are inherited from cssparser/selectors. No
  external corpus exists for terminal-grid layout; our gate stays corpus goldens + proptest laws.
- **Acceptance:** every box above is checked with its proof green and all five gates pass at 347
  library and 17 render-golden tests. The remaining human eyeball smoke in a real terminal is
  pending; the prior 281-lib/10-golden close count is historical.

### M1-C — External stylesheets (done — user smoke pending)

Locked implementation details: the render-blocking window starts after parsing and lasts five
seconds; only currently applicable occurrences block, with one coalesced late repaint after the
applicable graph settles. Dump mode shares the I/O-free page-load driver and takes exact content
`--cols`/`--rows` dimensions (rows default to 24). Resource keys are `(tab, generation, resource)`
without changing the URL-only `FetchRequest` API. Iterative discovery uses the first non-template
HTML base, strips URL fragments, and resolves external imports against final response URLs.
Logical occurrences preserve order while normalized URLs fetch once. Import depth is eight and the
64 limit counts occurrences. Separate 32 MiB retained-raw and unique-decoded ceilings atomically
disable external CSS when crossed; the per-response 10 MiB failure remains local. Missing or invalid
MIME defaults to CSS; other valid MIME is rejected except for same-origin quirks documents.
Nonmatching conditional sheets fetch eagerly, background tabs render lazily, and resize reevaluates
both viewport dimensions without restoring the blank loading state.

- [x] Discover `<link rel=stylesheet>` and recursive `@import` URLs against the effective document
      base; use the same tab/generation scheduler as navigation.
- [x] Preserve cascade document order independently of completion order; coalesce applicable graph
      completion into one late repaint and lazily render background tabs on activation.
- [x] Per-load ceilings: 64 external occurrences, separate 32 MiB retained-raw and unique-decoded
      budgets, 10 MiB per resource, import depth 8; cycles terminate and failures remain
      tab-local/non-fatal. Crossing either aggregate budget discards all external CSS.
- [x] **Selector bucketing.** The cascade indexes every active rule
      (O(elements × rules)) — acceptable for embedded `<style>`, not for real sites' sheets. Bucket by
      the rightmost simple selector (id/class/local name) using the `selectors` crate's own
      machinery. *Proof:* a synthetic 5,000-rule × 2,000-element benchmark stays inside the M6 perf
      budget, and cascade results are identical to the naive path on the fixture corpus.
- [x] **`@media` beyond type-only**: `scripting` (maps to the session JS flag),
      `prefers-color-scheme` (maps to the theme), `width`/`height` (map to the content viewport).
- [x] **Late-repaint scroll clamp.** `apply_rendered_page` clamps `Tab.scroll` after every rendered
      display-list replacement, including immediate late CSS and deferred background-tab
      activation. *Proven by* `late_stylesheet_repaint_clamps_active_tab_scroll` and
      `late_stylesheet_repaint_clamps_on_background_tab_activation`. (done)
- **Acceptance:** out-of-order, stale-generation, redirect/base, import-cycle, budget,
  cascade-order and late-repaint clamp fixtures green; bucketed and naive cascades agree; manual
  Wikipedia smoke renders with external author styles.

### M1-D — Layout completeness (done — user VGA smoke pending)

Sequenced after M1-C and before M2: a terminal browser is judged on whether real pages are readable,
and tables are what separate w3m from lynx. Flex/grid stay in M6.

- [x] **Table layout** *(done)* — `display: table*` stops degrading
      to block. Normalize the
      styled box tree with CSS anonymous-table fixup, then use an in-house formatter beside Taffy's
      block geometry. Support auto and fixed column width resolution, `colspan`/`rowspan`, nested
      block and inline tables, top/bottom captions, separate and collapsed borders, per-edge border
      width/style/colour, and sparse paint primitives. HTML tables receive compact UA spacing of one
      horizontal cell and zero vertical cells; authored `display: table` starts at zero spacing.
      Fixed-layout cell overflow clips at the inner edge on grapheme boundaries without ellipses.
      Percent constraints are evaluated once against the selected table width; a resulting minimum
      may grow the table without recursive percentage reevaluation. Resource limits degrade an
      oversized table to normal block flow while preserving its content. In-house per the audit
      above; Taffy's `item_is_table` is used at its block-layout boundary.
      The audit regressions are closed: normal flow and cells share one Unicode/white-space
      formatter; nested tables retain source order and inline-table baseline placement; styled and
      empty captions own real geometry, paint and hit regions; fixed tracks clip only after
      formatting; column groups and inherited table properties contribute normally; limited
      anonymous fixup groups consecutive improper children without synthetic DOM nodes; and clipped
      nested strokes cannot relocate a far edge. Full misparented-role wrapper construction remains
      explicitly owned by **Outer/inner display modes** below. Public render regressions sit beside
      the occupancy, glyph, hit-box, width, depth-limit and fixture proofs.
- [x] **Generated content and markers** *(done)* —
      `::before`/`::after`/`::marker` parse and match,
      `content` supports strings, `counter()`, `counters()`, `attr()` and `none`/`normal`, CSS
      counters (`counter-reset`/`counter-increment`/`counter-set`) run over a depth-scoped stack,
      and `list-style-type`/`list-style-position`/`list-style` drive markers. `Display::ListItem`
      exists; `append_prefix` and its hardcoded `"• "` / `"# "` prefixes are gone. Outside markers
      hang in a field reserved on the item's left padding, shared and right-aligned across sibling
      items so `9.` and `10.` meet one text column; inside markers stay inline. `<ol start>`,
      `<ol reversed>`, `<li value>` and `ol/ul type` map onto the counter engine at UA origin, and
      implicit `list-item` operations merge with author counter declarations instead of being
      replaced by them. Headings lost their `#` prefixes and gained UA bold; UA bullets step
      disc→circle→square with nesting depth. Generated fragments carry the originating element's
      `NodeId`, so link rects, hit-testing and search keep working through them.
      The three audit cascade failures are closed, each in the value parser that owned it:
      `display: list-item` computes `Display::ListItem` instead of being bundled with the modes that
      genuinely degrade to block; `content` and `counter-*` treat the CSS-wide keywords as "names
      nothing" rather than "invalid", so a later `initial` clears the earlier side-table value
      instead of letting it stand — while an empty counter list still merges with the implicit
      `list-item` operation, so `counter-increment: initial` does not stop a list numbering; and
      `content: normal` is distinct from `content: none`, deferring to the UA marker on `::marker`
      while still producing no box on `::before`/`::after`. *Proven by* the replaced degradation
      assertion plus `an_authored_display_list_item_is_a_real_list_item_not_a_block`,
      `a_css_wide_content_keyword_clears_an_earlier_winning_declaration`,
      `a_css_wide_counter_keyword_clears_the_reset_without_stopping_the_list` and
      `marker_content_normal_defers_to_the_default_while_none_suppresses_it` — each verified to fail
      against the unfixed source — in addition to the existing twelve cascade tests (independent
      nesting, sibling-scope isolation,
      `counters()` joining, `display: none` suppression, invalid `content` dropping only its own
      declaration, `h1, .note::before` no longer losing the `h1` half, implicit-vs-authored counter
      merging), six layout tests (shared marker field, hanging indent, inside markers, suppressed
      markers, markers and generated content inside table cells, link rects spanning generated
      content) and the `lists`/`generated` fixture goldens.
- [x] **Outer/inner display modes** *(done)* — computed display retains CSS Display's outside,
      inside, box-generation and table-internal categories, including strict legacy and
      multi-keyword grammar. The private flow tree elides `contents` principal boxes after cascade
      inheritance while keeping pseudo content, link ancestry and unusual-element computed-value
      rules. Inline flow-root/table/grid boxes are atomic, use shrink-to-fit normal-flow content,
      horizontal margins and a last-content-line baseline; inline flex uses Taffy shrink-to-fit and
      its first flex-line baseline. Block flow-root/grid keep normal flow while block flex uses
      Taffy; real grid layout remains owned by M6. Anonymous table wrappers group
      consecutive internal roles after `contents` elision, have no fabricated DOM owner, fill
      missing row/table parents, and preserve text under resource-limit degradation. *Proof:* strict
      cascade grammar and computed-value tests; formatting-tree tests for box elision, inheritance,
      link geometry, atomic baseline/margins, the grid normal-flow fallback and misparented roles; one public
      render golden spanning the modes.
- [x] **Length units and the cell metric** *(done)* — `parse_length_token` previously ignored the unit and the axis, so
      `1px`, `1em`, `1rem`, `1pt` and `1vw` are all one cell: `padding: 20px` eats a quarter of an
      80-column viewport and `width: 960px` built a 960-cell box. `parse_media_length` had the
      matching flaw on the query side (`(min-width: 640px)` can never match, `40em` is
      `MediaQuery::Never`), so both must change together or responsive sites flip to a layout
      nobody chose. The implementation adds a cell metric (~8px × ~16px), the absolute/relative unit table anchored to
      a 16px root font size, axis-aware rounding, and MQ4 range syntax. Intrinsic sizing must change
      in both engines: block `min_content_width` and table `cell_metrics.minimum` currently return
      the widest grapheme rather than the widest unbreakable segment. Rewrites every fixture and
      golden that currently writes `px` meaning cells. *Proof:* unit-conversion tests per unit and
      axis; block and table min-content cases use whole unbreakable words; a responsive fixture
      picks the same breakpoint a browser would; existing goldens re-baselined deliberately, not
      silently.
      The implementation contract is a nominal 8×16 CSS-pixel cell and 16px root font. Absolute
      units are `px`/`in`/`cm`/`mm`/`Q`/`pt`/`pc`; font-relative units are `em`/`rem` at 16px and
      `ex`/`ch` at 8px until typography supplies real metrics; viewport-relative units are
      `vw`/`vh`/`vmin`/`vmax`. Layout values round to the nearest axis-sized cell with positive
      halves upward and clamp after conversion; media queries compare unrounded CSS pixels. MQ4
      feature-first, value-first and chained ranges join the legacy colon/min/max forms. Borders
      remain binary, and the VGA feature pins this shared default to its actual 8×16 glyph grid.
- [x] **Presentational HTML** *(done)* — map `align`, `bgcolor`, `width`, `cellspacing`,
      `cellpadding`, `border`, `rules`, `frame`, `valign`/`vertical-align`, `<center>` and
      `<font color>` into the normal author origin at zero specificity, before all author rules;
      attribute-dependent UA defaults remain at UA origin. Add inherited `text-align`
      (`start`/left/right/center, with justify rendered left), table-cell `vertical-align`, reusable
      auto margins, legacy descendant alignment, WHATWG integer/dimension/color parsing, and exact
      direct-table association for derived cell hints. `table[align=center]` uses auto margins;
      left/right table floats stay in M6. Preserve compact HTML-table spacing and binary terminal
      borders. *Proof:* focused parser/cascade/layout cases plus a style-aware old-school fixture.
- [x] **VGA-native bitmap typography** *(done)* — compute inherited `font-size` from the
      supported length/percentage grammar, absolute and relative keywords, CSS-wide keywords and UA
      heading sizes. VGA rasterizes the existing CP437/Unifont faces at integer 1×/2×/3×/4× cell
      scales; terminal and dump keep their historical one-cell output. Thresholds are 32px, 24px,
      18.72px and 16px; positive smaller text stays 1× and dim, while zero has no glyph or advance.
      Scaled layout owns full-cell rectangles, bottom-aligns mixed runs, degrades a common line scale
      until definite widths fit, wraps at 1×, clips whole graphemes and shares the same rules in
      normal and table flow. VGA restores old overlays from shadow cells, performs the ratatui draw,
      rasterizes scaled runs in depth order, repaints the cursor last, and clips against content,
      scroll, window and menu occlusion. *Proof:* computed-value/cascade cases; block/table sizing,
      clipping, hit and link laws; paint reservation/alpha/contrast cases; headless VGA pixel goldens
      for scale, decoration, occlusion, stale-overlay restoration and cursor ordering; pure CLI
      frontend-selection tests. No new font, anti-aliasing or fractional rasterization dependency.
- **Acceptance:** the fixture set above green; manual smoke on a table-heavy page (Wikipedia infobox)
  is readable without horizontal guessing.

### M1-E — Overflow and positioning (done — human VGA/terminal smoke pending)

- [x] Cascade `overflow`, `overflow-x`, `overflow-y` and inherited `visibility`, including
      pseudo-elements, anonymous boxes, CSS-wide keywords, atomic invalid-value handling and the
      cross-axis computed-value fixup.
- [x] Map computed overflow to Taffy with zero-width scrollbars and clip every layout producer on
      all four edges without moving negatively positioned content. Hidden boxes retain geometry;
      hidden paint and hit regions do not, while explicitly visible descendants reappear.
- [x] Cascade and lay out `position`, `inset` and the four inset longhands. Blockify absolute/fixed
      boxes, resolve their containing blocks through a source-order-preserving flow-tree pass, keep
      fixed subtrees out of document height, and preserve the flow index invariant.
- [x] Constrain definite-width auto-layout tables to their specified width when min-content and
      percentage constraints would otherwise expand them; shrink columns proportionally with a
      one-cell floor and clip cell output on grapheme boundaries.
- [x] Add focused cascade/layout contracts plus positioning and Wikipedia-navbox regressions. Run all
      local gates, inspect every snapshot change, then repeat the Wikipedia terminal dump and leave
      the VGA/terminal interaction smoke to the human list.
- **Deliberate limits:** opacity, floats/clear, `z-index`, `clip: rect()`, relative positioning of
  non-replaced inline boxes, positioned descendants inside the independent table-cell formatter,
  true sticky behavior and a fixed-position repaint layer remain unimplemented.

### M2 — Tabs & keyboard navigation (in progress)

Started 2026-08-25 from the robustness end rather than the keyboard end, because the failure paths
are what the browser did worst: a server's 404 was discarded, a binary body was painted as garbage,
and a deep page could abort the process. Render robustness and the non-2xx body are done; content-type
sniffing and rendered error pages remain open. The indextree new-node append prerequisite is done;
the promoted M6 image delivery is the next feature, and the remaining M2 robustness and keyboard
items resume when images close.


- [x] **Linear-time construction of new DOM children** *(done 2026-08-26)* — pinned indextree
      4.9.0 and replaced `Document::append`'s create-detached + `checked_append` sequence with
      `append_value`, whose construction contract exactly matches a value that has never existed in
      the arena. All APIs that attach, prepend, insert or move an existing `NodeId` retain
      `Document`'s validation and indextree's checked mutation paths. *Proof:* an isolated ignored
      child constructs a 100,000-node chain while the existing permitted ten-second parent watchdog
      contains the old quadratic implementation; direct cases preserve root order, parent/child
      relationships and move-cycle rejection. The complete feature gate matrix is green.
- [ ] Keymap unification (extends the M0 keymap tests, same file): `Ctrl+L` (+ existing `a`) focuses
      the address bar so `/` is freed; `/` becomes in-page search; `Tab` in the address bar moves
      focus to content; new `FocusTabs` action (`F6`).
- [ ] `?` help overlay rendered from the keymap definition (single source of truth), snapshot-tested.
- [ ] Keyboard link navigation over the M1-B link list: Tab/Shift+Tab + Enter, focused link
      highlighted, re-paint only on target change (one invalidation path, shared with M3 hover),
      per-tab isolation; plus **link marks/hints** (lynx-style numbering) as the discoverable form.
- [ ] TabManager completion: Ctrl+T/W/N/P cycle + close-last→fresh (exist since M0); page titles from
      `document.title` with host/URL fallback; in-flight loads show the URL.
- [ ] Anchor links: `#fragment` → scroll-to-box + status line; no URL rewrite.
- [ ] `target="_blank"` links → new tab; per-tab history dedup verified by tests.
- [x] Reload (key `R`): generation++, fresh engine+document, scroll top (per-load pivot). (done — M1.5)
- [ ] In-page search: `/` opens a prompt (reuses EditBuffer), `n`/`N` next/prev with
      scroll-into-view, match highlight distinct from link focus, `x/y` counter, `Esc`/`Enter` closes.
- [ ] **Rendered error pages** — DNS/fetch/non-2xx/unknown-scheme failures paint a readable in-content
      error screen. *Half landed 2026-08-25:* non-2xx responses now **keep the body**
      (`http_status_as_error(false)` plus `FetchResponse::status`), so a server's own 404 page renders
      when it is HTML and non-empty, with the status in the context bar; an error status with an
      empty or unreadable body reports the status instead. `accepts_stylesheet_response` rejects any
      non-2xx up front, so a 404 body can never be parsed as CSS. Still open: the failure screens are
      the old three-line stub, and the unparseable-URL and unknown-scheme paths
      (`navigation.rs`) still paint nothing at all — both need the real themed page.
- [x] **Reader-facing load status** *(done — user smoke confirmed)* — a successfully rendered document reports that
      it loaded, not html5ever's recoverable parse-error count, CSS parser telemetry, generation
      numbers or accepted-resource internals. Preserve those diagnostics in their owning pipeline
      results and tests. The status bar still surfaces actionable HTTP, fetch, stylesheet and layout
      failures, including nesting truncation and a disabled external-CSS path.
- [x] **Declarative refresh navigation** *(done — user smoke confirmed)* — recognize the first valid WHATWG
      `meta[http-equiv=refresh]` directive in document order, including markup inside `<noscript>`
      when scripting is disabled. The pipeline parses the bounded content microsyntax and resolves
      its URL through `url`; `app` performs zero-delay navigation through the per-load pivot and
      `--dump` follows the same bounded route. The trampoline's history entry is replaced and
      automatic chains stop at eight. Delayed refreshes remain inert until M2 has a user-visible
      timer/cancel interaction. *Proof:* the live
      DuckDuckGo `cpu` → Wikipedia 200-HTML trampoline, relative/base and malformed grammar cases,
      replace-history behavior, background-tab isolation and a bounded self-refresh chain.
- [ ] **Content-type honesty** — a missing or unparseable `Content-Type` currently defaults to HTML,
      so a binary body is parsed and painted as garbage. Sniff (WHATWG minimum: leading `<`,
      BOM/NUL heuristics) and otherwise refuse with the unsupported-type page.
- [ ] **Back/forward without refetching** — keep a small per-tab document cache keyed by history
      entry so Back/Forward restore instead of re-issuing a network request; the per-load pivot still
      applies to fresh navigations.
- [x] **Render robustness** *(done)* — block nesting is capped at `MAX_BLOCK_DEPTH = 256`, past which
      the flow tree stops and paints `[nesting too deep to render]` where it cut off. The cap is
      measured, not guessed: on the 1 MB stack Windows gives the main thread, an uncapped debug build
      overflows between **460 and 480** levels (~2.2 KB per level), so 256 keeps about half the budget
      for the frames layout runs beneath, and sits far above any real page. The eight Taffy `expect()`
      calls became a degraded tree; `layout` cannot write the status bar, so `LayoutLimits` rides the
      `BoxTree` and then the `DisplayList` — the route `parse_errors`/`css_warnings` already take, and
      the one the resize repaint can still read. `FetchPool::try_recv` separates `Disconnected` from
      `Empty` and every waiting tab is told; `submit` returns `Queued`/`Duplicate`/`Closed` and a
      refused job no longer strands a tab on "loading"; all four quit paths — terminal loop, VGA tick,
      the window's own close button and the VGA failure path — call `Navigate::shutdown`, which
      detaches instead of joining. *Proven by*
      `nesting_past_the_cap_is_truncated_instead_of_overflowing_the_stack` (driven on a deliberately
      1 MB stack, and observed to abort with `STATUS_STACK_OVERFLOW` when the cap is raised),
      `nesting_within_the_cap_renders_whole`, `a_pool_with_no_senders_left_reports_disconnected_not_empty`,
      `a_submitted_job_reports_whether_the_pool_took_it`,
      `detaching_a_parked_worker_returns_instead_of_joining_it`,
      `a_lost_fetch_pool_tells_every_waiting_tab_instead_of_leaving_it_on_loading` and
      `a_pool_that_refuses_the_job_does_not_leave_the_tab_loading_forever`.
- [x] **Designed start page** *(done)* — `about:blank` is a viewport-aware ANSI-style scene with an
      exact 78×18 default canvas. Unicode half blocks provide two independently colored vertical
      pixels per terminal cell for the TextSurfer logo, surfer, sun, beach and palm; repeated
      `TEXTSURFER` letters with layered blue backgrounds form the breaking wave and its board-side
      tail. The scene scales and centers with the content viewport, adding detail on larger tabs,
      without image protocols or invented sub-cell detail. It deliberately contains no instructional
      copy; the M2 help overlay owns the keymap reference. *Proof:*
      `start_page_is_the_exact_colored_default_terminal_canvas`,
      `start_page_renders_inside_the_default_content_frame`,
      `start_page_scales_to_fill_a_larger_content_viewport` and
      `resize_refits_the_start_page_to_the_content_viewport`. (done)
- [ ] **Basic forms** *(in progress — controls render and match selectors; nothing is operable
      yet)*. Target: text/search/hidden/submit, textarea, select, checkbox, radio; GET and
      `application/x-www-form-urlencoded` POST via `url::form_urlencoded`; unsupported
      methods/encodings render a controlled error. Static initial state and user-toggled state
      share one form model.
      **Landed:** `core::form` is that one model — a `FormState` of *user overrides only*, with
      every unset control resolved from its content attributes, so an empty state is exactly the
      authored page and `--dump` needs no seeding. Controls generate their rendering from
      `layout::replaced`: brackets when nothing else delimits them, the field's extent carried by
      reverse video rather than a filler glyph, `placeholder` shown dimmed and never submitted.
      Box-level replaced elements paint from their *content* rect and take their intrinsic size
      when CSS gives none, so a `width:100%` field fills its box and a block `<input>` keeps its
      `size=`. `:checked` matches only checkbox/radio/option and `:disabled` reaches through a
      disabled `<fieldset>`/`<optgroup>` — the attribute-only matcher used to let `<div checked>`
      match `:checked` and hide from a dump fixture. `opacity: 0` computes to `visibility: hidden`.
      **Remaining:** `FormState` mutation and the keyboard/pointer editing model; `:checked`
      reading live state rather than attributes; `FetchRequest` method/body and the entry-list
      serialisation. *Proven by* the `forms` cascade module, the `replaced` layout module and the
      `form_controls` render golden.
- **Acceptance:** scripted-drive checklist of every keybinding incl. the rebinds; per-tab state
  isolation tests; chrome items (titles, error pages, start page, search, help overlay)
  snapshot-tested; the robustness and cache items proven by the tests named above; manual DuckDuckGo
  Lite submission works.

### M3 — Mouse (in progress)

Sequenced ahead of M2 at the user's request (2026-08-24): mouse navigation before keyboard/tab work.
VGA-first — the window frontend is the default, so winit is the primary adapter and the crossterm
mapping exists to keep the frozen terminal fallback behaviourally aligned, as `from_window_key` and
`from_terminal_key` already are.

**Slice 1 — navigation (done — partial user smoke: VGA wheel, links, toolbar, menu confirmed
2026-08-24; hover preview and hand cursor, tab chips and the `+` box, address caret, middle-click and
`target="_blank"`, side buttons, and the whole terminal frontend not yet exercised by a human)**

- [x] One content origin shared by paint and hit-testing *(done)*. `chrome::content_rect` insets the content
      band on all four sides, but the `Content` widget draws side rails only, so the VGA scaled-text
      overlay — its single caller — paints every 2x/3x/4x glyph one row below the background cell
      reserved for it and clips scaled text on the first and last visible rows. Replaced by
      `ChromeGeometry::content_view`, which paint and hit-testing both use. Confirmed M1-D regression,
      fixed here per the register's rule that confirmed regressions are acceptance items.
- [x] Pointer plumbing *(done)*: `MouseKind` carries its button so wheel and move cannot claim one,
      `MouseButton` gains the `Back`/`Forward` side buttons winit reports, and both frontends map
      pointer events into `core::event` — winit primary (`MouseInput` carries no position, so the last
      `CursorMoved` cell is tracked), crossterm aligned, with mouse capture enabled and released
      around the terminal loop plus a panic hook, since `ratatui::restore` does not clear capture.
- [x] Zone dispatch *(done)* over `ChromeGeometry::target_at`: menu titles toggle dropdowns and popup rows
      dispatch, tab chips activate and the `+` box opens a tab, toolbar buttons run their existing
      actions including the dimmed states, an address click focuses and places the caret without
      destroying a partial edit, content clicks focus. Everything routes through the existing
      `Action` set; the mouse is not a second command set. Wheel scrolls only in content.
- [x] Link activation *(done)* on release against the tracked press node (WHATWG/Ladybird `handle_mouseup`
      contract), middle-click and `target="_blank"` to a new tab, `<base href>`-aware resolution of
      the raw `href`, and same-document fragments deferred to M2's anchor item instead of refetching.
- [x] Hover *(done)*: status-bar URL preview, repaint only on target change, re-derived after scroll,
      navigation, tab switch and resize; `CursorIcon::Pointer` over links in the window frontend.
- **Hit-test resolution contract:** hover and link activation use one row-indexed,
  topmost-in-paint-order resolver against the live document; DOM-ordered link storage remains the
  keyboard-navigation contract.

**Slice 2 — live dynamic state (in progress — interactive launch regression under diagnosis)**

- [ ] Replace callback-driven redraws with one cross-frontend frame transaction: coalesced domain
      input, one final viewport/dynamic-state commit, semantic chrome/content damage, and one
      presentation opportunity. The event budget is a fairness bound, not a frame-rate limiter.
      The injected-clock scheduler gives both adapters an immediate idle frame and a 16.667 ms
      sustained cadence, carries discrete backlog losslessly, and has no wake while idle. *(reopened:
      the synthetic cadence contract is green, but the latest native smoke still feels loaded and
      laggy)*
- [ ] Retain the presented Ratatui buffer and page scene. Pure scrolling moves the content-row
      region and paints only exposed/damaged rows. Continuous hover state and resize previews defer
      their conservative full render/reflow fallback until 50 ms quiet, so wheel, pointer and resize
      streams cannot repeatedly enter cascade or layout. *(reopened for fluency; two correctness
      defects that made VGA scrolling render as a frozen bulk with drifting fragments are now fixed
      — see the updates log 2026-08-25 — and guarded by real-`Surface` pixel-equivalence and
      page-down tests. The remaining item is fluency, not correctness.)*
- [ ] Bound VGA raster and presentation work with batched cell invalidation, retained scaled-text
      overlays, pixel damage, buffer-age-correct partial copies, and softbuffer resize only when the
      physical size changes. Damage is retained as at most 32 merged regions with a 50% full-damage
      threshold; unsolicited redraws reuse the retained surface without composition or copying.
      *(reopened: the real window still has unresolved movement/resize/input latency. Note the
      softbuffer Win32 backend is a single retained DIB — `age()` is always 1 — so the
      buffer-age/`damage_history` repair is effectively dead there; a future cleanup can drop it.)*

- [x] `:hover`, `:active` and pointer focus are live inputs to the dynamic-state evaluation added in
      M1-B, dependency-gated without changing load progress or status messages.
- [x] Keep `:focus`, `:focus-visible` and `:focus-within` distinct. The parser previously mapped all
      three to the exact-focus state; ancestor propagation and the keyboard focus-indicator policy
      now have separate selector contracts and a pointer/keyboard source ready for M2.
- [x] Theme extension: a distinct hover accent with snapshot goldens. Selected and search-match
      accents stay in M2, where keyboard link navigation and in-page search provide their consumers.
- [x] Honour the inherited CSS `cursor` property, including every predefined cursor winit 0.30.13
      can represent and `none` through native cursor visibility. Custom cursor images use their
      mandatory predefined fallback but are not loaded in this slice.
- **Acceptance:** scripted zone/state, loop-budget and stateful render tests are green, including
      the layout-changing hover fixed-point regression; native and terminal launch confirmation plus
      the manual mouse walkthrough remain pending.

**Slice 3 — chrome affordances (done — human smoke pending)**

- [x] **Tab close box** *(done)*. Every whole chip carries a Turbo Vision `[■]` before its right
      corner — CP437 0xFE, so the VGA face draws it from the DOS font rather than the Unifont
      fallback tier. `layout_tabs` owns the columns, so drawing and hit-testing widen together; the
      `+` hint and the clipped `…»` stub carry none. `TabManager::close_active` becomes
      `close(index, fresh)` with a lazily-built `FreshTab`, closing any tab rather than only the
      active one and never dragging the selection with a background tab that closes ahead of it.
      Like `Tab(index)`, the close box calls the session directly: no `Action` can name a tab index.
      *Proof:* `every_whole_chip_carries_a_close_box_and_nothing_else_does`,
      `the_close_box_resolves_to_its_own_slot`, `a_clipped_chip_has_no_close_box_to_press`,
      `closing_a_background_tab_leaves_the_same_tab_in_front`,
      `a_close_box_closes_its_own_tab_and_leaves_the_front_one_in_front`,
      `pressing_a_chip_beside_its_close_box_still_selects_the_tab`.
- [x] **Page scrollbar** *(done)*. The content frame's right rail becomes `▲` cap, `▒` track, `█`
      thumb, `▼` cap — always drawn, and a document that fits gets a full-track thumb. It takes over
      the rail rather than claiming a column, so `content_cols` is unchanged and nothing reflows.
      `ChromeLayout::scrollbar` is the one derivation both the painter and the pointer read, and the
      VGA scaled-text overlay already clips to the band interior, so a 2x–4x glyph cannot bleed into
      it. Caps and trough dispatch the existing `Scroll*` actions; only the thumb drag is new, since
      no action can name an absolute position, and it round-trips exactly because placement rounds
      down while its inverse rounds up. A drag owns the pointer — it keeps tracking off the bar,
      hovers nothing it crosses, and ends when the pointer leaves the window, because nothing
      captures the pointer and a button released outside is never reported.
      *Proof:* `dragging_to_a_row_and_reading_it_back_lands_on_the_same_row`,
      `an_unpainted_document_does_not_divide_by_zero`, `a_bar_too_short_for_caps_is_all_track`,
      `the_scrollbar_owns_the_right_hand_column_of_the_content_band`,
      `the_scrollbar_caps_step_a_row_and_the_trough_pages`,
      `dragging_the_thumb_walks_the_document_to_both_ends`,
      `a_thumb_drag_owns_the_pointer_and_lights_up_nothing_it_crosses`,
      `leaving_the_window_ends_a_thumb_drag`.
- [x] **Retained composition repaints the bar** *(done)*. A retained scroll moves a full-width
      region, thumb included, so every frame that touched the content owes the column a repaint.
      *Proof:* `a_retained_scroll_redraws_the_thumb_it_dragged_along`, plus the existing
      `retained_scroll_matches_a_fresh_composition` and the real-pixel
      `incremental_scroll_matches_a_fresh_full_compose_on_the_real_surface`, all three of which fail
      without it.
- [x] **Closing a tab renders its replacement** *(done — confirmed regression on the path)*.
      `close_tab` never called `activate_current`, so the tab that came to the front kept showing a
      stale page until an unrelated event poked it, while `NextTab`/`PrevTab`/`select_tab` all
      refreshed. *Proof:* `closing_the_front_tab_renders_the_one_that_takes_its_place`.
- **Acceptance:** the tests above are green; the human walkthrough of both affordances in the VGA
  window and the terminal remains pending.

### M4 — JS seam (open)

- [ ] `JsEngine` + `js` feature wiring in the composition root; runtime `--js=off` wins over feature.
- [ ] `MutateOp` funnel + invalidation-once rule; host subset: document, location, console→status
      buffer, alert→dialog line; no dispatch except `onclick` handlers. Harden the DOM boundary at
      this first non-html5ever caller: repeated template-content creation must not orphan the prior
      fragment or overwrite its mapping, and mutation failures must not reach the existing panic
      paths.
- [ ] Noop contract suite covers the inert set; app behavior byte-identical compiled-off vs on-but-off.
- **Acceptance:** `--js=off` and no-js builds pass identical integration suites.

### M5 — Boa (open)

- [ ] Decision gate: Boa 0.21.1 vs Deno Core on an async responsiveness fixture.
- [ ] `BoaEngine` behind the trait; engine instance per loaded document; job pump per tick (≤256),
      injected clock; fetch/timer promises resolved from the job queue; TLA out of scope.
- [ ] Full-suite contract run (script feature); noscript fixture; globals-isolation across documents.
- [ ] test262 subset runner: pinned checkout under `testdata/test262`, harness files, YAML
      frontmatter, curated slices, xfail manifest by feature, regressions forbidden. Upstream
      reference: Boa 0.21.1 ≈ 94.12%; our slice must stay within a documented delta.
- **Acceptance:** fixture page (inline script + onclick + document.title + console echo) green; no
  crashes on example.com with JS on; gates green with `--features js`.

### M6 — Stretch (in progress)

- [x] **Taffy 0.14 baseline upgrade** *(done)* — pin 0.14.0 with the existing
      block-only features, adapt the 0.14 leaf-measure callback without changing render output,
      and require unchanged goldens plus the complete feature gate matrix before flexbox begins.
- [x] **Flexbox** *(done)* — Taffy's flexbox engine owns block, inline and nested flex geometry.
      The cascade supports the flex direction/wrap/flow, grow/shrink/basis/shorthand, order,
      justify/align, gap/place-content, height and min/max sizing families with atomic invalid-value
      handling and computed flex-item blockification through `display: contents`. Constraint-keyed
      deferred atoms share table and nested inline-flex formatting across intrinsic measurement and
      fragment emission; text baselines reach Taffy's baseline alignment. Generated content and
      tables participate as flex items, while stable order-modified layout leaves DOM order intact.
      Paint builds a row index over a total interaction order shared by hover and link activation.
      Grid and float fallbacks are unchanged. *Proof:* complete supported direction/wrap,
      shorthand, alignment and CSS-wide cascade matrices; block/column, every direction,
      main/cross-axis distribution, grow/shrink/basis/constraints, auto margins, wrapping/gaps,
      inline-flex shrink-to-fit, stable order, generated-item, table-item, deep nesting, baseline,
      degenerate geometry and overlapping-link layout and paint cases; all 12 direction/wrap pairs;
      a wrapped-flex viewport property law; an inspected public render golden, public
      DOM-link/visual-order case and dynamic-restyle reflow case; complete local feature gate matrix
      green.
- [x] **CSS custom properties and `var()`** *(done)* — stable Level 1 custom names and
      arbitrary token values, case-sensitive inherited per-element environments, dependency-cycle
      invalidation, token-boundary-safe fallback substitution and all supported style, typography,
      counter and generated-content consumers. Computed custom values and substituted declarations
      are capped at 2 MiB with bounded component nesting. `ComputedStyle` and `StyleTree` remain
      unchanged; selector bucketing's median may not regress by more than 15%. CSSOM, `@property`,
      animations, `env()`, dynamic Level 2 names/units and `revert-layer` remain out of scope.
- [x] **CSS math, render metrics and signed margins** *(done)* — parse and type-check bounded Values 4
      `calc()`, `min()`, `max()` and `clamp()` trees after custom-property substitution, preserve
      unresolved percentages until the owning layout axis is known, and resolve physical units
      through a frontend-injected render context. VGA injects its 8×16 bitmap metric with scaled
      text; terminal and dump inject the nominal 8×16 cell metric. Enable Taffy's `calc` and
      `strict_provenance` features behind a layout-owned adapter rather than its zero-resolving
      built-in tree. Signed margins must reach block, flex, inline and the supported table subset;
      overlapping paint and hit testing share one source/depth order. Unsupported dimensions,
      division by zero, non-finite results, incompatible types and exhausted limits invalidate the
      declaration atomically. Landed: render-context plumbing; typed mixed length/percentage math
      for size/min/max, margins, padding, insets, gaps, `flex-basis` and `font-size`; used-value
      `min()`/`max()`/`clamp()` with `none` bounds; destination-axis shorthand lowering; signed
      margins; nonnegative used-value ranges; and shared Taffy/manual-path calculation resolution.
- [x] **Grid** *(done)* — enable Taffy's grid engine after the flex formatting and
      paint-order seams settle.
      *Promoted to the top of the remaining M6 rendering work 2026-08-26 on live evidence:* Wikipedia's
      Vector 2022 skin lays its whole page skeleton out with grid — `grid-area` for
      header/titlebar/toolbar/content/column/footer, and `grid-template-areas: 'headerStart headerEnd'`
      for the header itself. `taffy_style` emits only `Flex` or `Block`, so every one of those falls
      back to normal flow and the skeleton stacks vertically instead of being placed. It is the
      single largest remaining gap between our header area and Firefox's.
      The delivery target is the CSS Grid Level 1 longhands and shorthands accepted by Taffy,
      including legacy `grid-gap` aliases, block/inline/nested Grid containers, transactional
      parsing, and bounded storage. `subgrid`, masonry, RTL/writing modes, Grid absolute positioning,
      aspect ratio, and z-index remain explicit limits. Math-valued tracks use the shared CSS math
      store, except inside auto-repeat where Taffy's fixed-component contract cannot represent them.
      Landed: Taffy-backed block, inline and nested Grid containers; Level 1 template, placement,
      implicit-track, flow, gap and alignment properties and shorthands; named lines and areas;
      shared layout-time CSS math; order-modified auto-placement; anonymous text, generated content,
      tables, flex items and `display: contents`; rollback-safe bounded interning; and topmost
      paint/hit/link behavior. *Proof:* cascade grammar and atomicity matrices, 96 focused Grid tests,
      two property laws, the Vector 2022 skeleton case, and a public rendering golden.
- [ ] **Images** *(in progress — automated work green; human VGA and terminal image smoke pending)* — one delivery across the
      shared pipeline, default VGA frontend and terminal compatibility frontend. The item is not
      complete until all four subitems and both human frontends pass. Static HTML `<img src>` is the
      first boundary: SVG, animation, `srcset`/`picture`, CSS images, `object-fit`, lazy loading,
      `data:` URLs and cross-page caching remain deferred.
      - [x] **Shared loading and decoding** *(done)*. Pin `image` 0.25.10 with default features disabled and
        only PNG, JPEG, WebP and GIF enabled; GIF and animated WebP expose their first frame only.
        Pin `ratatui-image` 11.0.6 separately as a terminal adapter with default features disabled
        and `crossterm` enabled, avoiding Rayon, Chafa, `pkg-config` and unrelated decoders.
        `PageLoad` owns typed stylesheet/image subresource state without changing `net::Fetch`:
        normalized image URLs resolve against the document base, obey the existing subresource
        scheme policy, fetch once per page, require a 2xx response and never extend the stylesheet
        blocking window. An `ImageDecoder` contract returns immutable RGBA assets with stable IDs,
        intrinsic dimensions and revisions; signature and enabled decoder support, not an extension
        or untrusted MIME label, decide the format. Deliveries in one app tick coalesce into one
        render. Empty/invalid URLs, fetch/decode failure, unsupported formats, refusal and pending
        work retain M1-B's nonempty/empty/missing-alt behavior and remain non-fatal.

        Before worker code lands, amend the standing thread invariant here and in `AGENTS.md` from
        fetch-only workers to one main owner for DOM/mutable state plus bounded workers over
        immutable bytes/pixels. The composition root injects one decode worker; the terminal adapter
        owns one bounded protocol-preparation worker while VGA samples decoded pixels directly into
        its owned framebuffer without allocating resized variants. Queues coalesce by work key, hold
        at most 128 distinct jobs, cancel queued decode work on navigation, tag results with
        tab/generation/asset revision/target geometry/render context, drop stale completions and
        detach on quit. Strict pre-allocation
        checks cap a page at 128 unique image URLs, 64 MiB fetched image bytes and 128 MiB decoded
        RGBA; one image is capped at 8192 pixels per axis, 8,388,608 pixels and 32 MiB RGBA output.
        `image::Limits.max_alloc` is set to 64 MiB only as defense in depth because that limit is
        non-strict. Crossing an image budget refuses only that resource and reports one aggregate
        page warning; unlike external CSS, already decoded images are not discarded atomically.
      - [x] **Replaced layout and shared paint** *(done)*. Layout consumes image state and intrinsic metadata,
        not pixel buffers. Pending or failed images use the existing fallback geometry; decoded
        assets use intrinsic size/aspect ratio with the existing CSS and legacy width, height and
        min/max constraints. `min-*`/`max-*` follow CSS 2.1 §10.4's constraint table, so they resize
        the picture along both axes at once instead of stretching it, measure the element's own
        padding and border when `box-sizing: border-box` says to, and quantise a natural size the
        way an authored length does. Successful decode
        reflows only when the used geometry changes. Image boxes participate in inline, block,
        table, flex and grid layout, overflow clipping, anchors and hit testing. `BoxTree` carries
        ordered image placements and `DisplayList` carries one unified paint-ordered overlay stream
        for scaled text and images plus a separate ID/revision-addressed immutable asset store, so
        buffers are neither duplicated per occurrence nor compared byte by byte. The cascade's
        ordinary-inline size reset must exclude replaced elements; otherwise inline image CSS and
        presentational dimensions are erased before layout.
      - [x] **Native VGA output** *(done)*. The VGA adapter nearest-samples only the visible target
        pixels from immutable decoded RGBA and alpha-blends them through the existing framebuffer
        overlay path; unlike terminal protocols, this requires no resize allocation or encoding. Image and scaled-
        text overlays share CSS paint order and obey content clipping, vertical scroll, partial
        viewport edges, menu occlusion, retained-surface restoration and damage tracking; the cursor
        still paints last. Navigation, resize, theme/backdrop changes and failed replacements cannot
        leave stale pixels.
      - [x] **Terminal output** *(done)*. Detect terminal capability once after terminal setup and build fixed-
        size `ratatui_image::sliced::SlicedProtocol` variants off the UI thread so vertical scrolling
        never resizes or encodes during presentation. Use Sixel/Kitty/iTerm2 only for an unobscured
        rectangular placement; fall back per placement to primitive cell-native halfblocks for
        horizontal clipping, later overlapping paint or popup occlusion, and use halfblocks globally
        if detection fails. While protocol images are present, compose fresh content on scroll rather
        than trusting the retained terminal scroll shortcut. Evict stale variants and force fresh
        content composition on movement, resize, navigation and replacement; terminal teardown clears
        the final screen on quit. `--dump` always retains the
        textual fallback and existing non-image terminal snapshots remain unchanged.
        A protocol carries its whole picture in one cell's escape and marks the rest of the image
        `CellDiffOption::Skip`; because `FrameComposer` replaces ratatui's diff with its own damage
        tracking, it owes that rule too and must never hand a skipped cell to the backend. Confirmed
        regression on this path, fixed here.

      *Proof required:* decoder/resource contracts for every format, first-frame behavior, misleading
      MIME, malformed input, redirects, deduplication, budgets, refusal, cancellation and stale
      generations; fallback transitions, intrinsic/CSS sizing, every supported formatting context,
      clipping, linked-image hits, completion coalescing and overlay order; VGA pixel cases for
      scaling, alpha, scroll, overlap, occlusion, damage and cursor order; injected terminal protocol
      cases, halfblock snapshots, sliced scrolling, complex-clip fallback, stale-region clearing and
      incremental-versus-fresh presentation equivalence. Run the complete local feature matrix for
      delivery. The `terminal-cell-v1` WPT oracle continues to exclude image cases; raster reftests
      now run under the separate `vga-pixel-v1` oracle below, where all three admitted cases are
      `xfail` pending CSS pixel precision for replaced sizing.
      Human Wikipedia image smoke in both VGA and terminal is the final acceptance gate.
- [ ] **Floats** — conformant line-flow-around-float formatting.
- [ ] **Perf gate**: largest corpus page layout+paint < 200 ms debug. Includes memoizing
      `format_inline`, which is currently recomputed on every Taffy measure call, again for intrinsic
      width, and again when emitting fragments; it also decides whether `DisplayList` can stay dense
      by row, since the painter currently allocates one `PaintedRow` for the entire document height
      even when most rows have no cells.
- [ ] Persistence/backup · per-history-entry scroll memory · drag input · console view (F12) ·
      config file · `data:` URL scheme · optional Readability-style reader view.
- Non-goals beyond this list stay non-goals.

## Test infrastructure (standing)

- Contract suites, capability-parameterized, defined in M0, run against every impl (M1-A fakes …
  M5 Boa) — an interface is defined by its tests.
- insta snapshots of ratatui `TestBackend` buffers (widget goldens), corpus goldens (M1-B; twelve
  fixture pages under `tests/fixtures/` as of M1-D generated content), DOM tree dumps (M1-A).
  `buffer_string` compares symbols only; style assertions use the style-aware helper added in M1-B.
- proptest laws — all ten green. Six from M1-B: viewport-width monotonicity, painted-row bounds,
  disjoint leaf glyph cells, laminar per-row box families and engine-backed deepest-hit round trip
  (`layout/engine/tests.rs`), plus scroll clamping as a fixed point under arbitrary key sequences
  (`app/controller/tests/`). Two more came with M1-D tables (`layout/table/tests.rs`): generated
  spans never overlap, and a wider table viewport never increases height. Grid adds bounded
  declaration/layout completion and auto-fill height monotonicity under wider viewports
  (`layout/engine/tests/grid/tracks.rs`). Property tests for `url_fix`/`EditBuffer` continue from M0.
- FakeFetch + fake clock + fake Host; no test touches the network. Product code and corpus workers
  never read the real clock; the sole exception is the static-WPT parent supervisor's fixed
  wall-clock watchdog, which may terminate and reap an isolated child.
- `tests/support/` shared corpus helpers; inline `#[cfg(test)]` fakes where module-local.
- `--dump` (M1-B) is the scriptable end-to-end harness: fixture in, golden text out.
- [x] **Static WPT terminal-cell pilot** *(done)*. Use a
      pinned, vendored WPT-derived slice rather than WPT's browser runner. One isolated child owns a
      complete test/reference graph, every page gets a fresh `PageLoad`, and a fixed ten-second
      parent watchdog contains aborts and hangs. The hermetic worker resolves only manifest-listed
      files below `https://wpt.test/` and runs parse → cascade → layout → paint with injected zero
      time, scripting disabled and production limits.
      - **Pinned pilot:** WPT `797589c8452b14ba448ba77819427ff3d743e37f`; raw-pass
        `css/css-ui/box-sizing-003.html` and `box-sizing-005.html` against their shared
        `reference/box-sizing-001-ref.html`; must-complete `textarea-large-padding-crash.html`,
        `large-border-crash.html` and `negative-flex-margins-crash.html`.
      - **Terminal oracle:** profile `terminal-cell-v1` is a 100×38 content viewport with terminal
        8×16 metrics, Paper White light media appearance and inert dynamic state. Reftests compare
        the actual ratatui content cells, not internal boxes: at least one `match` agrees and every
        `mismatch` differs. VGA builds also render each graph with VGA metrics for crash-only
        coverage. Geometry is never an additional WPT verdict.
      - **Classification:** a typed manifest records `run`/`skip`/`xfail`, capabilities, exact
        reference relations and the transitive resource allowlist. Only an assertion mismatch may
        xfail; unexpected pass, crash, timeout and harness errors fail. The 24 audited
        `box-sizing-*` tests admit `003`/`005`, skip `001`/`026` for `z-index`, `007`–`025` for
        SVG/image intrinsic sizing and `027` for `testharness.js`. The three named crashtests are
        curated safety additions, not directory-wide coverage.
      - **Integrity and acceptance:** a dedicated failure-preserving fetch tool vendors exact
        upstream bytes, license and SHA-256 membership under `testdata/wpt/`; Rust tests never use
        the network. Completion requires audited=27, eligible=5, run=5, pass=5, xfail=0, skip=22,
        no unexpected/crash/timeout/harness result, all local gates, and the manual smoke list.
- [x] **VGA pixel WPT profile** *(done)*. `vga-pixel-v1` is a second typed profile inside the same
      manifest, so the raster cases inherit the existing hash, isolation, watchdog, allow-listed
      resource and match/mismatch contracts rather than getting a parallel runner. It renders test
      and reference at the fixed 100×38 cell viewport through deterministic `PageLoad`/image
      delivery and the real VGA software surface, crops the browser chrome away and compares exact
      RGB with no tolerance; a mismatch writes actual/expected/diff PNGs below `target/wpt-vga/`.
      Terminal-cell verdicts stay separate, and font cases remain excluded independently.
      - **Admitted:** WPT `css/css-sizing/box-sizing-replaced-001..003.xht` with their references
        and 20 raster support files, at the existing pinned revision.
      - **Result:** audited=3, eligible=3, run=3, pass=0, xfail=3, skip=0, no
        unexpected/crash/timeout/harness result. The three are expected failures for the reason
        below, and the profile reports a passing case as `unexpected`, so it is a two-way tripwire.
      - **Honest label:** this is TextSurfer's deterministic VGA profile, not general browser pixel
        equivalence.
- [ ] **CSS pixel precision for replaced sizing** *(open — found by the `vga-pixel-v1` profile)*.
      Lengths are converted to whole cells at computed-value time, so a replaced element's `min-*`
      and `max-*` reach layout already rounded. When a constraint pins one axis, the ratio-preserving
      size of the other is then derived from the rounded value: `max-height: 85px` becomes five
      16px rows, and the width taken from it is 80px where the reference's authored `width: 75px`
      is nine 8px columns. That one-column drift is why the three admitted WPT replaced-sizing
      reftests are `xfail`. Closing it means carrying CSS pixel precision into replaced sizing and
      quantising once, at the end; the WPT cases turning into `unexpected` passes is the proof.
- [x] **Rendering regression atlas** *(done)*. `tests/fixtures/render_atlas.html` is one
      deterministic offline document split into 10 stable named panels — `ua-flow`, `inline-state`,
      `lists`, `tables`, `controls`, `search-flex`, `box-layout`, `flex-grid`, `images`,
      `presentational` — covering the rendering-special element families and the supported block,
      table, flex, grid, form-control and replaced-image constellations. Image requests are answered
      synchronously from fixed RGBA fixtures; no network, worker, clock, OS font or JS is involved.
      Every panel carries semantic and geometry assertions, and a manifest rejects duplicate,
      missing, oversized or untested panels.
      - **Reference inventory:** 33 Insta snapshots (30 per-panel styled-cell, 3 whole-document
        structure) across the 40, 100 and 160-column widths, plus 30 exact VGA PNG references under
        `tests/reference/vga/render_atlas/`.
      - **Update rules:** ordinary runs compare and never rewrite. Terminal references use Insta's
        explicit update mode; VGA references require both the ignored generator and
        `TEXTSURFER_UPDATE_ATLAS=1`. Failures write actual and diff PNGs only below
        `target/render-atlas/`.
      - Standards correctness remains the pinned WPT profiles' job; the atlas locks TextSurfer's
        reviewed cell-quantized result rather than declaring browser pixel equivalence.
- **Standing rule — rendering regressions.** Every rendering defect gains a focused minimal
  regression, and a visually relevant one also gains or amends an atlas panel. Every newly supported
  rendering-special element or layout context extends the atlas manifest before its roadmap item can
  be marked done. Compatible standards behavior is backfilled into the pinned WPT profiles
  incrementally. Actual gate counts, reference inventory, WPT profile results and human-smoke status
  are recorded in the log below.
- [ ] **Incremental WPT terminal-cell backfill** *(open; non-blocking after the pilot)*. Import one
      bounded, fully inventoried tranche at a time from supported box, sizing, values, alignment,
      flex, grid and table suites. Exact existing case statuses may not regress, but the reported raw
      supported-slice rate may fall when new known failures expand the denominator. Script, server,
      cross-origin, fuzzy, font/image, print/manual, variant, native-widget and viewport-sensitive
      cases remain excluded until a capability-specific roadmap decision admits them. WPT results
      are labelled a TextSurfer terminal-cell slice, never browser pixel conformance; specifications
      remain authoritative and Ladybird is a manual comparison implementation for disputed cases.
- [ ] Corpus-harness audit follow-up: `DatCase.error_count` is parsed but never compared with
      `ParseOutcome.parse_errors`, so the published 95.16% is tree-output conformance only; compare
      error counts or explicitly justify the exclusion. The attribute-order test named for UTF-16
      covers BMP names only while `tree_dump` uses Rust scalar-value sorting; add an astral-vs-BMP
      case against the pinned reference serializer.

### External conformance corpus (M1-A / cross-cutting / M5)

- WPT `html/syntax/parsing/resources/*.dat` (pinned commit `ed37f83e`; the html5lib-tests repo is
  archived and points here) is the M1-A landing gate; the tokenizer suite is informational.
- Static WPT rendering uses the same pinned, vendored, hashed and offline model. Its adapter consumes
  upstream HTML/reference metadata directly; it does not translate cases into bespoke Rust tests or
  claim pixel-browser equivalence for cell-quantized output. WPT `testharness.js` is not admitted
  unless a later M5+ capability audit proves that the required script and DOM APIs exist; this plan
  makes no promise to support that harness.
- test262 (pinned commit): executed through `boa_engine` 0.21.1 with harness files and YAML
  frontmatter honored; per-milestone curated slices with an explicit xfail manifest.
- css-syntax + WPT-selectors corpora: inherited by adopting `cssparser` / `selectors`.
- Rules: corpora live in `testdata/`; HTML parsing uses `tools/fetch-corpus.ps1`, while rendering
  uses its dedicated human-run fetch tool. Both pin exact commits and keep Rust tests offline. Xfail
  entries name an exact case and reason; each corpus reports its own clearly labelled progression.

## Non-goals (locked unless a milestone re-opens them)

Text selection/copy, iframes/`<frame>`, bidi/RTL/writing modes, `line-height`/fonts, border-radius,
inline-element borders, cookies, `addEventListener` DOM events (click-only v0), top-level await,
full CSS/DOM, window-title setting, syscall sandboxing, config files pre-M6, drag input pre-M6.

## Roadmap updates log

Log of decisions, pins, and plan changes only — task status lives in the plan markers above.

- 2026-08-27 — **Terminal graphics-protocol images were being overprinted by their own halfblock
  fallback; confirmed from user smoke and fixed.** Reported symptom: the top band of every image
  rendered at full resolution, the rest as halfblocks, with the real picture visible behind the
  halfblocks whenever a pointer move forced a repaint. Cause: a graphics protocol puts the entire
  escape sequence in one cell and marks every other cell of the picture `CellDiffOption::Skip`;
  those cells still hold the halfblock fallback, and `FrameComposer` — which does its own damage
  tracking instead of using ratatui's diff — handed all of them to the backend. Two things followed:
  the fallback text printed over the picture, and because the escape's cell declares
  `ForcedWidth(1)` while a sixel leaves the real cursor below the image, the crossterm backend
  suppressed its `MoveTo` and landed the rest of the run at the wrong place. The composer now drops
  skipped cells, which is the rule ratatui's own diff applies. Proof: a focused regression drives a
  real sixel picker and asserts the reserved cells are composed but withheld while the escape's own
  cell is drawn; it fails on the previous code at the first covered cell. Format, all three strict
  Clippy configurations and the default/JS/VGA/no-default matrices are green at 858 library tests
  (764 without defaults). The terminal image path still needs a human re-smoke.

- 2026-08-27 — **Rendering-regression hardening delivered; replaced sizing corrected and its
  remaining drift pinned as an xfail.** The rendering atlas and the `vga-pixel-v1` WPT profile are
  both in place with the inventories recorded in their items above. Admitting WPT
  `box-sizing-replaced-001..003.xht` immediately caught a real defect the focused tests had missed:
  `min-*`/`max-*` on a decoded image clamped the two axes independently, so a constraint stretched
  the picture instead of scaling it, and `box-sizing: border-box` was ignored for those constraints
  while a natural size rounded up where an authored length rounds to nearest. `image_cells` now
  follows CSS 2.1 §10.4's table, subtracts the element's own chrome when `box-sizing` says to, and
  rounds like `CellMetric::resolve_cells`; the two table rows that scale one axis to satisfy a
  minimum are clamped afterwards, which is what the WPT references expect. What remains is not an
  algorithm bug but lost precision — computed lengths are already whole cells, so a width derived
  from a rounded `max-height` lands one column off the reference — and it is now an open item with
  the three cases held as `xfail` rather than a weakened comparison. Format, all three strict Clippy
  configurations and the default/JS/VGA/no-default matrices are green at 857 library tests (763
  without defaults), 13/12 binary, 6 fetch-pipeline, 14 corpus, 42 golden, 3 atlas and 6 WPT.
  Human VGA and terminal smoke on example.com, DuckDuckGo and Wikipedia images is still pending, so
  **Images** stays open.

- 2026-08-26 — **DuckDuckGo click-through smoke passed.** The user confirmed that searching for
  `cpu` and opening the first Wikipedia result now loads the destination successfully. This closes
  the human acceptance gate for declarative refresh navigation and the reader-facing load status.
- 2026-08-26 — **Declarative refresh navigation delivered; human VGA click-through re-smoke
  pending.** The isolated pipeline scanner follows the WHATWG prefix grammar, scripting-disabled
  `<noscript>` parsing, document/base URL rules, first-accepted directive semantics and a 4 KiB
  input ceiling. The app replaces wrapper history, preserves background-tab ownership and stops
  automatic chains after eight pivots; dump mode follows the same bounded path. The exact live
  DuckDuckGo `cpu` wrapper now renders Wikipedia's Central processing unit article. Format, all
  three strict Clippy configurations and the default/JS/VGA/no-default matrices are green at 829
  library tests (736 without defaults), 13/12 binary, 6 fetch-pipeline, 14 corpus and 42 golden.
- 2026-08-26 — **DuckDuckGo click-through hang reproduced and declarative-refresh repair started.**
  The `cpu` result points to DuckDuckGo's `/l/?uddg=…&rut=…` wrapper. A live fetch proves it returns
  HTTP 200 with an empty script-driven body plus
  `<noscript><meta http-equiv=refresh content='0;URL=https://en.wikipedia.org/...'>`; it is not an
  HTTP redirect, so ureq correctly has nothing to follow. TextSurfer will implement the WHATWG
  declarative-refresh seam rather than special-case DuckDuckGo. Only zero-delay navigation is
  admitted now, with replace-history semantics, per-tab generation pivots and an eight-hop cap.
- 2026-08-26 — **Reader-facing load status delivered; human DuckDuckGo re-smoke pending.** A
  malformed-but-recoverable HTML controller fixture first reproduced `1 parse errors`; successful
  HTML and plain-text loads now report `loaded <url>` without parser, CSS, generation or accepted-
  resource telemetry. HTTP/fetch failures, stylesheet failures, external-CSS shutdown and layout
  degradation remain visible. The focused controller and composed-fetch regressions pass, followed
  by format, all three strict Clippy configurations and the default/JS/VGA/no-default matrices at
  821 library tests (728 without defaults), 13/12 binary, 6 fetch-pipeline, 14 corpus and 42 golden.
- 2026-08-26 — **Reader-facing load-status repair started from DuckDuckGo navigation evidence.** A
  clicked result rendered, but the context bar presented html5ever's single recoverable conformance
  error as `1 parse error`. Parse recovery is not a load failure and ordinary readers cannot act on
  CSS warning counts or generation/resource telemetry. The M2 status path will retain actionable
  HTTP, fetch, stylesheet and layout failures while a successful page reports only that it loaded.
- 2026-08-26 — **Real-site smoke passed; CSS math and the static WPT pilot are done.** The user
  reports the manual smoke looks correct after the table-intersection and Wikipedia search-control
  repairs. This closes the reopened CSS-math acceptance gate and the pilot's final human gate;
  conformant floats are again the next open M6 rendering item. Incremental WPT backfill remains a
  separate non-blocking test-infrastructure task.
- 2026-08-26 — **Static WPT pilot automated implementation green; human smoke pending.** The
  typed/offline corpus at WPT `797589c8452b14ba448ba77819427ff3d743e37f` reports audited=27,
  eligible=5, run=5, pass=5, xfail=0, skip=22 and zero unexpected, crash, timeout or harness
  outcomes. Its isolated child supervisor, terminal-cell oracle, authored-background sentinel,
  hash/license/membership checks and failure-preserving fetch-tool tests are green. The complete
  local matrix is green with 819 default/JS/VGA and 726 no-default library tests, plus all binary,
  integration, corpus, golden and doc-test targets. The item stays in progress and continues to
  block new M6 rendering features until the manual example.com, DuckDuckGo Lite and Wikipedia smoke
  is recorded.
- 2026-08-26 — **Static WPT plan narrowed after feasibility and failure-mode audit.** This
  supersedes the earlier same-day broad rendering-corpus entry: only an isolated five-case pilot
  blocks M6, while bounded backfill continues incrementally. WPT is pinned at
  `797589c8452b14ba448ba77819427ff3d743e37f`; the pilot has two raw-pass reftests, three
  must-complete crashtests and 22 explicit box-sizing skips. Cell output is the sole reftest oracle;
  geometry verdicts and the aggregate nondecreasing pass-rate floor were rejected. A ten-second
  wall-clock watchdog is allowed only in the parent test supervisor. Typed manifest parsing pins
  dev-only `serde` 1.0.229 and `serde_json` 1.0.151; upstream bytes and BSD-3-Clause licensing stay
  pinned, hashed and offline.

- 2026-08-26 — **Static WPT rendering conformance promoted ahead of further M6 features.** The
  existing html5lib WPT corpus proves the pinned/offline/hash-manifest model, but unit contracts and
  project-authored goldens did not protect the combined CSS-math, border-box, replaced-control and
  table paths during the Wikipedia smoke. The next cross-cutting task therefore adds a direct static
  WPT crashtest runner followed by a cell-rendering reftest adapter. It starts with box sizing and
  expands through the supported box, sizing, values, flex, grid, table and widget suites. The plan
  explicitly excludes unsupported dynamic/server/font/image cases from the denominator, forbids
  blanket xfails, preserves transitive upstream resources, uses style-aware visible-cell equality
  for the adapted WPT verdict, retains geometry fingerprints for diagnostics and reviewed exact
  cases, isolates every rendered corpus page behind deterministic deadlines, and retains human
  real-site smoke.
  WPT's browser-oriented Python runner is not added to the Rust gates; no dependency pin changes
  until implementation audits the exact upstream commit and fixture set.
- 2026-08-26 — **Wikipedia search-control sizing regression fixed; human re-smoke pending.** The
  post-crash smoke reached the page, but the Vector header search field geometry was wrong and its
  Search button lost the label. A Wikipedia-shaped border-box/padding/overflow test failed before
  the fix: the CSS-math sizing change had stopped adding definite border and padding to a replaced
  element's intrinsic size under `box-sizing: border-box`, allowing the chrome to consume the entire
  content box. Intrinsic sizing now retains that definite chrome contribution with saturating
  arithmetic; percentage padding remains basis-dependent. Format, all three strict Clippy
  configurations and the default/JS/VGA/no-default test matrix are green at 819 library tests (726
  without defaults), 13/12 binary, 6 fetch-pipeline, 14 corpus and 42 golden tests. CSS math remains
  in progress until another human Wikipedia smoke passes.
- 2026-08-26 — **Wikipedia table-intersection panic fixed; human re-smoke pending.** A focused test
  reproduces the exact horizontal underflow and also covers the vertical-disjoint case.
  `intersect_rect` now constructs its result lazily, so rejected intersections perform no unsigned
  subtraction. An audit of the remaining eager `then_some` calls found no matching arithmetic
  hazard. Format and all three strict Clippy configurations are green. The default, JS and VGA
  matrices pass 818 library, 13 binary, 6 fetch-pipeline, 14 corpus and 42 golden tests; no-default
  passes 725 library, 12 binary, 6 fetch-pipeline, 14 corpus and 42 golden tests. The CSS-math item
  stays in progress until the reported Wikipedia route passes another human smoke.
- 2026-08-26 — **CSS-math milestone close reopened by Wikipedia human smoke.** Surfing
  `https://en.wikipedia.org/wiki/Terminal_emulator` reached a disjoint table-rectangle intersection
  and panicked at `layout/table/geometry.rs` through eager unsigned subtraction inside
  `bool::then_some`. Milestone completion now requires a regression contract for disjoint rectangles,
  lazy intersection construction, the full local gates, and another human Wikipedia smoke.
- 2026-08-26 — **M6 CSS math, render metrics and signed margins complete.** The shared bounded
  expression store now preserves percentage provenance and property range policy across every
  supported length/percentage consumer. Margin and padding shorthands lower each component for its
  destination cell axis while retaining the containing-width basis; inset, gap and flex-basis use
  their owning axes; font-size math resolves against the inherited computed size before descendant
  font-relative lengths. Taffy receives pure-length expressions as lengths and defers basis-bound
  expressions through the same resolver used by leaf and container layout; manual inline/table
  cyclic percentages use a zero basis. No dependency pin changed. Format and strict
  default/all-feature/no-default Clippy are green. The default, JS and VGA matrices pass 817
  library, 13 binary, 6 fetch-pipeline, 14 corpus and 42 render-golden tests; no-default passes 724
  library, 12 binary, 6 fetch-pipeline, 14 corpus and 42 render-golden tests. The new CSS-math
  golden was inspected, no existing snapshot changed, and no `.snap.new` was produced. The standing
  human smoke list remains the milestone-close handoff.
- 2026-08-26 — **M6 typed CSS math completion started with corrected resolution contracts.**
  cssparser 0.37.0 and Taffy 0.14.0 remain the latest published versions, so no dependency changes
  are planned. The remaining margin, padding, inset, gap, flex-basis and font-size forms must retain
  percentage provenance, lower shorthand components for their destination axes, and distinguish the
  output cell axis from the percentage-basis axis. In particular, top and bottom margin/padding
  percentages use containing-block width and therefore scale columns into rows under the injected
  cell metric. Pure-length math must reach Taffy as a length when no percentage token occurred;
  declaration parsing and bounded interning remain atomic. Taffy-owned intrinsic passes retain
  Taffy 0.14's missing-basis behavior for mixed length/percentage calculations; the manual inline
  and table paths resolve their cyclic percentage component against zero.
- 2026-08-26 — **M6 Grid rendering complete.** Taffy-backed Grid now covers the documented Level 1
  parser, cascade, layout, content and paint slice with transactional bounded stores and shared
  math/order properties. Format and strict default/all-feature/no-default Clippy are green. The
  default, JS and VGA matrices pass 811 library, 13 binary, 6 fetch-pipeline, 14 corpus and 41
  render-golden tests; no-default passes 718 library, 12 binary, 6 fetch-pipeline, 14 corpus and 41
  render-golden tests. One inspected Grid snapshot was added, no existing snapshot changed, and no
  `.snap.new` was produced. README capability and next-work summaries now reflect Grid completion.
  The standing human smoke list remains the milestone-close handoff.
- 2026-08-26 — **M6 Grid implementation resumed with a corrected parser contract.** Kept the
  existing Taffy 0.14.0 and cssparser 0.37.0 pins after confirming they are current; Grid adds
  `smallvec` transitively. The slice requires atomic shorthand and variable handling, bounded and
  rollback-safe interning, complete Level 1 placement and template grammar within the documented
  engine limits, layout/content coverage, and a public rendering golden before completion.
- 2026-08-26 — **M2 form controls render; the item stays open for interaction and submission.**
  Started from live evidence rather than the board: `lite.duckduckgo.com` — M2's own acceptance page —
  rendered the single word "DuckDuckGo", because every control fell through to `Display::INLINE`
  with no children and so produced no box, no glyph and no hit region. Hacker News' and Wikipedia's
  search fields were missing for the same reason.
  - **`core::form` stores user overrides only.** Every unset control resolves from its content
    attributes, so an empty `FormState` *is* the authored page. That is what "static initial state
    and user-toggled state share one form model" asks for — one lookup rather than two stores that
    can drift — and it makes `--dump` need no seeding, a reset a `clear()`, and the per-load pivot
    automatic.
  - **Neither engine trait grew a required parameter.** `Cascade::apply` and `LayoutEngine::layout`
    have one production caller each but ~93 and ~34 test call sites; a defaulted
    `layout_with_form_state` keeps every existing site compiling and meaning what it meant — *no user
    input yet*. The three read-only inputs that now travel together became one `LayoutInput`, which
    also settled a Clippy argument-count limit the fourth parameter tripped.
  - **Two confirmed regressions closed** — see the risk register. A block-level replaced element
    painted nothing at all, and generated content on a bordered box painted at the border-box
    origin. The second was mine, introduced earlier in this same slice and caught by viewing
    Wikipedia at the VGA frontend's real 160 columns rather than the 100 the earlier passes used.
  - **Controls were redrawn once after review.** The first attempt filled fields with underscores,
    which made `[hi____]` indistinguishable from a value containing underscores and doubled the
    emphasis against an underline the UA sheet was also setting. Blanks plus reverse video replaced
    both. That rework exposed a real bug: the normal-flow walker was giving the stand-in the
    *parent's* `white-space`, collapsing the field's padding — invisible while the filler was a
    glyph.
  - **`opacity: 0` computes to `visibility: hidden`**, with the two resulting divergences accepted
    and recorded in the locked decisions. Without it, three `opacity: 0` dropdown checkboxes paint
    over Wikipedia's article chrome — that pattern is how the web builds a custom control.
  - No dependency changed. Format, strict default/all-feature/no-default Clippy and the
    default/JS/VGA/no-default test matrix are green at **724 library tests** (**631** without default
    features), 13 binary (12 without), 6 fetch-pipeline, 14 corpus and 39 render-golden tests. Only
    the new `form_controls` golden was added; no pre-existing snapshot moved, and the `example.com`
    and DDG Lite dumps are unchanged. Human VGA and terminal smoke remain pending.
- 2026-08-26 — **M6 basis-dependent comparison math delivered; item remains in progress.**
  `min()`, `max()` and `clamp()` now remain as typed expressions until Taffy supplies the containing-
  block basis, including nested arithmetic, all three `none`-bound forms and the specified rule that
  a conflicting minimum wins. A `StyleTree`-owned, structurally deduplicated expression store keeps
  `ComputedStyle` copyable and caps the tree at 65,536 stored nodes; each parsed value remains capped
  at 32 nested components and 256 primary nodes. Invalid types, division by zero and exhausted limits
  still discard the declaration atomically, including after custom-property substitution. No
  dependency pin changed. Typed math for margins, padding, insets, gaps, flex basis and font size
  remains open. The complete local format, strict default/all-feature/no-default Clippy and
  default/JS/VGA/no-default test matrix is green at 695 library tests (602 without defaults), 13
  binary tests (12 without defaults), 6 fetch-pipeline, 14 corpus and 38 render-golden tests.
- 2026-08-26 — **M6 CSS math foundation delivered; item remains in progress.** `RenderMetrics` and
  `RenderContext` now keep cell geometry and text capability together from VGA/terminal/dump through
  `PageLoad` and `MediaContext`; both current profiles explicitly inject 8×16, so a future frontend
  can change geometry without changing cascade code. Taffy 0.14.0 enables `calc` and
  `strict_provenance`; a layout-owned tree now dispatches block/flex/leaf work and resolves opaque
  calculation handles without unsafe code. The bounded cssparser-backed evaluator type-checks
  arithmetic, resolves supported units through the injected metric, defers mixed
  length/percentage `calc()` on size/min/max properties, and supports range functions when their
  ordering is independent of the eventual percentage basis. Basis-dependent ranges and math for
  margins, padding, insets, gaps, flex basis and font size stay open rather than being approximated.
  Margins are signed through cascade, block/flex layout and inline cursor offsets; a public overlap
  case proves later content paints on top. Taffy remains latest 0.14.0 and cssparser latest 0.37.0.
  The complete local gate matrix is green at 689 library tests (596 without defaults), 13 binary
  tests (12 without defaults), 6 fetch-pipeline, 14 corpus and 38 render-golden tests.
- 2026-08-25 — **M6 CSS math, render metrics and signed margins started from a green baseline.**
  Frontend-owned render metrics replace an implicit universal geometry assumption; VGA injects its
  8×16 bitmap profile and terminal/dump the
  nominal 8×16 cell profile. cssparser remains pinned at latest 0.37.0. `muskitty-css-values` 0.1.0
  was rejected because it cannot supply computed-value typing, percentage-basis evaluation,
  resource caps or the full target grammar. Taffy remains pinned at latest 0.14.0 and will enable
  its `calc` and `strict_provenance` features behind a layout-owned adapter. All eight required
  gates were green before implementation: 684 library tests (591 without defaults), 13 binary (12
  without defaults), 6 fetch-pipeline, 14 corpus and 38 render-golden tests.
- 2026-08-25 — **M6 CSS custom properties started from a green baseline.** Stable Custom
  Properties Level 1 is the target; the experimental Level 2 dynamic-name and variable-unit grammar
  is excluded. cssparser remains pinned at 0.37.0 and owns syntax tokenization while the cascade owns
  inherited environments, dependency cycles and substitution. CSS math is a separate open layout
  item. The pre-change selector-cascade median is **6.487 ms**; a confirmed regression over 15%
  blocks delivery. All eight required gates were green before implementation.
- 2026-08-25 — **M6 CSS custom properties delivered.** cssparser 0.37.0 now validates stable
  Level 1 custom names, arbitrary values and every `var()` grammar while retaining source spelling.
  Case-sensitive `Rc` delta environments compute inheritance, CSS-wide values and fallback-aware
  dependency cycles off the call stack; token-boundary-safe substitution feeds typography,
  style/flex declarations, counters and pseudo content without enlarging `ComputedStyle` or
  `StyleTree`. Invalid computed declarations use the shared applied/invalid/unsupported outcome to
  become `unset`, shorthands reset atomically and `revert` restores the captured UA baseline.
  Values and substitutions are capped at 2 MiB with component nesting capped at 64. The inspected
  `variables.html` golden and literal-equivalence/dynamic-restyle cases cover the public render path.
  All format, strict Clippy and default/JS/VGA/no-default gates are green at **684 library tests**
  (**591** without default features), plus 13 binary, 6 fetch-pipeline, 14 corpus and 38 render tests.
  The selector-cascade median moved from **6.487 ms** to **7.003 ms** (+7.95%), inside the 15% gate.
  Human VGA and terminal interaction smoke remains pending. No dependency pin changed.
- 2026-08-25 — **M1-E overflow and positioning started from a green default test baseline.** The
  live Wikipedia terminal dump confirmed three related failures: zero-height hidden dropdowns paint
  into the article, the clipped skip link prints one glyph per row, and a 100%-wide auto navbox
  expands through its own border. CSS Overflow 3 and Taffy 0.14 require `clip` to remain distinct
  from scrollable overflow; only a specified `visible` axis paired with `hidden | scroll | auto`
  computes to `auto`. `auto` maps to Taffy's non-scrolling clipped behavior, while `scroll` uses a
  zero scrollbar width. Axis-aware element clips, signed emission geometry and inherited visibility
  are therefore one layout slice. Positioning follows with source-order-preserving containing-block
  hoisting; the auto-table correction is the final independent slice. No dependency pin changes.
- 2026-08-25 — **M1-E overflow and positioning delivered.** Cascade now computes overflow axes,
  inherited visibility, all five position modes and signed/percentage/auto insets. Taffy receives
  the matching overflow/position primitives with zero-width scrollbars; a shared axis-aware clip
  trims boxes, fills, strokes and whole graphemes before hit/link geometry is derived. Positioned
  flow hoists absolute descendants to the nearest positioned ancestor without disturbing source
  order, fixed subtrees stay out of document height, and negative text origins clip instead of
  translating. Definite-width auto tables proportionally shrink oversized min-content columns while
  intrinsic probes retain their natural minimum. All format, strict Clippy and default/JS/VGA/
  no-default gates are green at **665 library tests** (**572** without default features), plus 13
  binary, 6 fetch-pipeline, 14 corpus and 35 render-golden tests. The live 100-column Wikipedia
  terminal-emulator dump contains none of the five recorded corruption signatures and retains all
  required article/navbox content. Human VGA and terminal interaction smoke remains pending.
- 2026-08-25 — **M2 started at the robustness end (user).** The slice is ordered
  robustness → non-2xx bodies → content-type sniffing → rendered error pages, ahead of the keyboard
  items, because failure was what the browser handled worst. The first two are delivered.
  - **The block-depth cap is a measured number, not a guess.** Taffy's `compute_block_layout`
    recurses through `compute_child_layout`, so nesting depth is stack depth. On the 1 MB stack
    Windows gives the main thread, an uncapped debug build overflows between **460 and 480** levels
    (~2.2 KB per level), so `MAX_BLOCK_DEPTH = 256` keeps roughly half the budget for the frames
    layout runs beneath. Following the 2026-08-23 launch-overflow lesson, the test drives the real
    path on a deliberately 1 MB stack, and was observed to abort with `STATUS_STACK_OVERFLOW` when
    the cap is raised. Table and inline-flex nesting keep their separate `TableLimits` bound.
  - **`layout` reports limits instead of writing the status bar.** It sits below `app`, so the new
    `LayoutLimits` rides the `BoxTree` and then the `DisplayList` — chosen over `RenderedPage`
    because the resize repaint (`viewport.rs`) paints straight from a stored document and never
    builds one.
  - **ureq pin unchanged at 3.4.0, configuration corrected.** The 2026-08-23 audit recorded only
    that 4xx/5xx-as-error is the *default*; `ConfigBuilder::http_status_as_error(false)`
    (`config.rs:435`) turns it off, and it governs error reporting only — redirect following and
    `get_redirect_history` are unaffected. `FetchResponse` gained `status: u16`; non-HTTP fetchers
    report 200.
  - **A non-2xx body is content for the document and never for a subresource.** Keeping the body
    exposed a real hole: `accepts_stylesheet_response` treats a missing or unparseable type as CSS,
    so a 404 page would have been parsed as a stylesheet and its rules applied. Confirmed by
    disabling the new status check and watching the test fail. Non-2xx policy: the status always
    shows in the context bar, the body renders only when it is HTML and non-empty, and anything else
    reports the status.
  - **Found and not fixed: `Document::insert_element` is quadratic in depth** — recorded in the risk
    register with its measurements. It is a parser-side hang that arrives before the layout cap can
    matter, and indextree 4.8.1 offers no unchecked append, so it needs its own decision.
  - No dependency changed. Format, strict default/all-feature/no-default Clippy and the
    default/JS/VGA/no-default test matrix are green at 647 library, 13 binary, 6 fetch-pipeline, 14
    corpus and 35 render-golden tests; 554 library and 12 binary without default features. No
    snapshot changed and no `.snap.new` was produced.

- 2026-08-25 — **M6 rendering work split into Flexbox, Grid and Floats.** Flexbox starts first on
  Taffy 0.14.0 with only its `flexbox` dependency feature. The slice includes height/min/max sizing,
  constraint-aware table/inline-flex formatting, baseline propagation, total paint order and indexed
  topmost hit/link resolution. It may land the formatter memoization and hit-index portions of the
  later performance gate early; the 200 ms corpus target, dense-row decision and remaining resource
  ceilings stay open.
- 2026-08-25 — **M6 Flexbox delivered.** Taffy remains pinned at 0.14.0 and now enables only its
  additional `flexbox` feature. CSS sizing and flex longhands/shorthands feed block, inline and
  nested flex layout; constraint-keyed deferred atoms preserve table/inline formatting and
  baselines. Generated items take part in stable order-modified layout. Paint interaction uses a
  row index and one topmost order for hover and link activation while keyboard links remain in DOM
  order. Grid, floats and the remaining perf gate stay open. The complete format, strict
  default/all-feature/no-default Clippy and default/JS/VGA/no-default test matrix is green at 604
  library tests (511 without defaults), 13 binary tests (12 without defaults), 4 fetch-pipeline, 14
  corpus and 32 render-golden tests; no snapshot changed.
- 2026-08-25 — **M6 Flexbox regression coverage expanded.** Table-driven cascade cases now cover
  every supported direction/wrap pair, shorthand omission form, alignment keyword and CSS-wide
  property group, plus invalid numeric/gap/sizing winners. Geometry cases cover all directions,
  main/cross-axis distributions, growth ratios, shrink, percentage basis/gap, min/max constraints,
  multiple auto margins, wrap-reverse, stable equal order, generated/table items and deep nested
  flex; a property law checks that widening wrapped flex never increases height. A new inspected
  public golden covers row distribution, growth/order, wrapping/gap, inline-flex, nested column flex
  and a table item; another public case proves visual order and DOM link order together. The matrix
  exposed and fixed `flex` shorthand incorrectly resetting container direction/alignment fields.
  Format, strict default/all-feature/no-default Clippy and default/JS/VGA/no-default tests are green
  at 617 library tests (524 without defaults), 13 binary tests (12 without defaults), 4
  fetch-pipeline, 14 corpus and 34 render-golden tests.
- 2026-08-25 — **M6 Flexbox branch and interaction coverage expanded again.** Primitive-value,
  sizing-axis, gap/place-content and invalid-safety cases close the remaining computed-style parser
  branches. Geometry now exercises all 12 direction/wrap pairs, column gaps, basis units,
  percentage constraints, box sizing, every supported distribution alias, safe/unsafe alignment,
  wrapped inline-flex first-line baselines and empty/zero-sized containers. A public dynamic-state
  regression proves hover-driven row-to-column reflow and restoration. The full local gate matrix is
  green at 626 library tests (533 without defaults), 13 binary tests (12 without defaults), 4
  fetch-pipeline, 14 corpus and 35 render-golden tests.
- 2026-08-25 — **M6 Flexbox tests audited and reorganized.** Cascade flex cases now share one
  computed-style fixture, while layout coverage is separated into axis, sizing and content modules.
  Mixed-responsibility cases were split for precise failures; fixtures resolve the container by DOM
  identity instead of box-vector position; the viewport property now always generates a nonempty
  flex container and a strictly wider comparison. A direct adapter case covers Taffy alignment
  safety and physical-axis fallback mapping. The full local gate matrix is green at 638 library
  tests (545 without defaults), 13 binary tests (12 without defaults), 4 fetch-pipeline, 14 corpus
  and 35 render-golden tests.
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
- 2026-08-20 — M1-A shipped: all gates green; composition root verified end-to-end over loopback.
  Corpus progression: 1829/1922 raw (95.16%) → 100% with xfail.
- 2026-08-20 — M1-A property-law correction: `url_fix` whitespace-only → `""` is the documented
  contract; two M0 laws gained the matching assumption.
- 2026-08-20 — Updates log trimmed to decisions/pins/plan changes only.
- 2026-08-20 — M2/M3 scope widened (user): chrome & usability folded in — TabBar page titles,
  rendered error pages, designed start page, in-page search, theme pass; console view/config/drag
  stay in M6. Plan review against `src/ui/keymap.rs` fixed M2: `/` → search with `Ctrl+L` for the
  address bar, `Tab` in the address bar becomes FocusContent, new `FocusTabs` (`F6`), `R`/`n`/`N`
  free. M0 keymap tests to be extended, not replaced. Theme moved to a single `ui::Theme`.
- 2026-08-20 — New milestone M1.5 (user): chrome redesigned as a DOS-style rich UI; `ui::Theme`
  pulled forward from M3's theme task. Can proceed in parallel with M1-B (separate modules).
- 2026-08-20 — M1.5 shipped: frame + divider chrome, `[‹][›][↻][⌂]` buttons with history-driven
  disabled states, `URL:`-labelled railed field. History: `Tab.history_pos` cursor with forward-stack
  truncation; history records at load time; back/forward/reload/home navigate in place through the
  per-load pivot. Navigation bindings Alt+Left/Right, R, Alt+Home suspended while the address field
  is focused; `layout_width` set from `content_cols()`.
- 2026-08-20 — M1.5 chrome follow-up (user): Norton-Commander palette with explicit black-on-white
  `Theme::selected()`; browser-like startup focus; always-visible interactive menu bar with dropdowns
  composed from ratatui `List` + `Block` + `Clear` (ratatui 0.30 has no menu widget —
  inventory-verified); mouse deferred to M3.
- 2026-08-20 — M1.5 tab strip restored and re-skinned (user): raised NC-style boxes with an open
  bottom under the active box, driven by the shared `layout_tabs` walker via `active_span` so the
  boxes and the divider gap cannot drift; titles capped at 24 cells.
- 2026-08-20 — Windows duplicate-input fix (user): crossterm reports both Press and Release on
  Windows; `from_terminal_key` now returns `Option` and accepts Press + Repeat, dropping Release.
  M3 mouse handling must apply the equivalent release-event gate.
- 2026-08-20 — QBasic-style restyle (user): grey menu bar at row 0 and grey context bar at the last
  row, navy `rgb(0,0,128)` field, dividers under the menu bar and above the bottom bar removed,
  `CHROME_ROWS` 10→6, address cursor y=3, `MouseZone` Menu 0 / Tabs 1–2 / Address 3–4 / Content 5+.
  `Theme` gains `bar_bg`/`bar_text`/`mnemonic`. M1.5 marked complete.
- 2026-08-20 — Full repository audit accepted for implementation. M1-R added before rendering;
  M1-C external styles split from stretch; forms moved to M2. Pins to refresh at use: indextree
  4.8.1, mediatype 0.23.0, crossbeam-channel 0.5.16, clap 4.6.6, unicode-segmentation 1.13.3,
  Taffy 0.13.0, textwrap 0.16.2, tempfile 3.27.0, sha2 0.11.0.
- 2026-08-20 — M1-R complete and M1-B started. First rendering slice pins cssparser 0.37.0,
  selectors 0.40.0, Taffy 0.13.0, textwrap 0.16.2, unicode-segmentation 1.13.3. RustSec audit: zero
  vulnerabilities; one unmaintained transitive (`paste` 1.0.15) reachable only through optional Boa.
- 2026-08-20 — M1-B property-law progress: viewport-width monotonicity and painted-row bounds green.
- 2026-08-20 — M1-B conditional-CSS plan corrected before implementation: type-only `@media`,
  bounded diagnostics, ignored `@import`, explicit degradation contracts; `box-sizing` moved to the
  box-model slice. Registry audit retained cssparser 0.37.0 + selectors 0.40.0 and rejected
  css-mediaquery 0.1.1, LightningCSS 1.0.0-alpha.72, Stylo/Blitz/MusKitty, rdom-tui, litehtml and
  Ladybird as incomplete media evaluators or incompatible whole-engine stacks.
- 2026-08-20 — M1-B conditional CSS and cascade degradation completed.
- 2026-08-21 — M1-B box-model outline corrected before implementation: `box-sizing` needs a fixed
  author width; author height stays intrinsic; six whitespace modes replace the two-state shorthand;
  node-owned fragments keep border drawing in paint; lengths capped at 65,535 with sparse rows.
- 2026-08-21 — M1-B box model completed: nested block hierarchy with private anonymous runs, Taffy
  intrinsic geometry, absolute border/content rectangles, text-node-owned Unicode fragments, six
  inheriting whitespace modes.
- 2026-08-22 — M1.5 tab-strip border follow-up completed.
- 2026-08-22 — Product renamed from TextSurf to TextSurfer across package, UI, docs and snapshots.
- 2026-08-22 — **Full roadmap rewrite after a whole-crate audit + prior-art survey (user).** The
  document was restructured (shipped milestones condensed, acceptance criteria restated as checkable
  proofs, audit findings assigned to owning milestones) and three classes of flaw were recorded:
  - *Corrections:* the "UA stylesheet as CSS text in `css/ua.rs`" decision described a file that never
    existed — `css/cascade.rs::ua_style` is the as-built form; "borders ≥2 cells doubled" is replaced
    by the binary-border model; the architecture diagram's `DisplayList → ui` edge was drawn as
    existing when the painter's hit and link lists are in fact discarded in `app/controller.rs`.
  - *New locked semantics* from the prior-art audit (chawan): contrast-corrected author colours over
    a themed default, `font-weight > 500` = bold, `font-size` ignored, sub-cell inline
    margins/padding ignored, overflow-x displays / overflow-y clips / no scrollbars, `:visited` never
    matches. Incremental/multi-process rendering deferred with reason; sandboxing a non-goal.
  - *New work owners:* M1-B gains the styled paint seam, dynamic pseudo-class parsing (rules
    containing `:link`/`:hover` were being discarded wholesale), the line-break-opportunity fix,
    pager keys, `<img alt>`/`<hr>`, and `--dump`; M1-C gains selector bucketing and real `@media`
    features; **new M1-D — Layout completeness** (tables, generated content/list markers,
    presentational attributes) is sequenced after M1-C and before M2 on the user's decision; M2 gains
    error-page bodies, content-type sniffing, a back/forward document cache and render robustness
    (DOM depth cap, no `expect()` in the render path); M6 pins `ratatui-image` 11.0.6 for images and
    owns the `format_inline` memoization behind the perf gate.
- 2026-08-22 — M1-B closed (user smoke pending). New pin: **cssparser-color 0.5.0** — it depends on
  the cssparser 0.37 already in use and owns every `<color>` value grammar, so no colour parser was
  hand-rolled; CIE-space colours parse but are ignored because the crate does not convert them to
  sRGB. `app::render` is a new module holding the shared `fetch → parse → cascade → layout → paint`
  path so the TUI and `--dump` cannot diverge, and `MediaContext` grew an injected `Palette` and
  `DynamicState` (the UA sheet takes its link colour from the theme instead of a literal). The four
  audit bugs on this seam are fixed with regression tests: dropped dynamic-pseudo-class rules,
  mid-word breaking of non-ASCII text, line-stepping PageUp/PageDown, and the discarded hit/link
  lists; `<img>` now renders `[alt]` and `<hr>` spans the content width. Gates green at 281 lib ·
  4 binary · 3 pipeline · 14 corpus · 10 golden. `--dump` renders example.com, Wikipedia and
  DuckDuckGo Lite over the real network without a panic; DuckDuckGo Lite comes out nearly empty,
  which is exactly the M1-D table gap and evidence for the tables-before-interaction ordering.
- 2026-08-22 — M1-C closed (user smoke pending). Existing pins were verified current and unchanged:
  **cssparser 0.37.0**, **selectors 0.40.0**, **encoding_rs 0.8.35**, **url 2.5.8**; no dependency was
  added. `ResourceId` extends the fetch scheduler to concurrent subresources, and the I/O-free
  `PageLoad` driver is shared by the TUI and `--dump`. Linked and imported sheets decode, resolve,
  deduplicate and cascade in logical document order under bounded render-blocking and memory rules;
  resize-aware media features and rightmost-compound selector buckets are active. Local proof is
  green at 303 library · 5 binary · 4 pipeline · 14 corpus · 10 golden tests; the development-profile
  5,000-rule × 2,000-element selector benchmark median is 26.0575 ms. The three live-site terminal
  smokes remain human-run by standing rule.
- 2026-08-23 — M1-D table layout completed; M1-D remains in progress for generated content/list
  markers, presentational HTML and `text-align`. No dependency was added: **Taffy 0.13.0** remains
  the block-geometry engine and its `item_is_table` boundary hosts the isolated formatter. HTML and
  authored CSS table roles now retain their computed displays; auto/fixed tracks, percentages,
  column hints, `colspan`/`rowspan` (including group-bounded zero), captions, nested block and
  bottom-aligned inline tables, compact UA spacing, separate/collapsed per-edge borders, conflict
  resolution, layered table backgrounds, grapheme-safe fixed-cell clipping and bounded fallback are
  active. Sparse background/stroke primitives replace the former one-bit layout-box border seam.
  Four table goldens cover simple, collapsed/spanned, nested/captioned and fixed-overflow cases;
  local proof is green at 315 library · 5 binary · 4 pipeline · 14 corpus · 14 golden tests. The
  Wikipedia-infobox smoke remains human-run by standing rule.
- 2026-08-23 — The M2 designed start page landed early by user direction. `about:blank` now scales
  and centers a static EGA-palette scene for the content viewport; an isolated compositor draws its
  curling wave from repeated `TEXTSURFER` letters over bright-blue and dark-blue water cells. The
  exact default canvas remains 78×18, larger tabs gain detail, and no image protocol or raster asset
  is used. The earlier keymap-reference requirement moved to the existing M2 help-overlay work
  because the accepted artwork contains no instructional copy.
- 2026-08-23 — **Whole-crate audit (user), read module by module.** Findings recorded with an owning
  milestone rather than fixed on the spot, except the two folded into the generated-content item:
  - *Wrong rendering, now owned by the new M1-D length-units item:* CSS lengths ignore their unit and
    axis; media queries compare CSS pixels against terminal columns and accept only `px`/`ch`;
    `min_content_width` returns the widest **glyph** instead of the widest unbreakable word, so any
    `AvailableSpace::MinContent` measurement reports 1 for ASCII text.
  - *Fixed in this milestone:* subresources had no scheme policy, so a remote page could name
    `file:///…` in a `<link>` and have `FileFetch` read local files for it; and `render_html` — the
    entry point behind every render golden — used its own `<style>` walker that ignored the `media`
    attribute, the `type` check and the `<template>` boundary that `PageLoad` honours, so the
    fixture corpus and the app did not share a style-discovery path.
  - *Owned by M2's chrome and keymap work:* the status bar prints internal telemetry
    ("accepted gen 7 …"); quitting joins fetch workers that may be parked under the 30 s timeout, so
    it can hang, while `shutdown_without_waiting` already exists; character bindings lowercase before
    matching, so `Q`/`R`/`B` fire the unshifted actions and M2's planned `n`/`N` search pair cannot
    work; the `Key::Enter`/`Key::Esc` arms for `Focus::Address` are unreachable.
  - *Documentation corrections made in the same pass:* the locked terminal-CSS entry read "all CSS
    lengths are cell-rounded", describing the unit defect above as if it were an intended rule; the
    standing test-infrastructure section still listed four proptest laws as outstanding when all
    eight have been green since M1-B/M1-D; and the custom-parser audit table had no row for the
    `content`/`counter-*`/`list-style*` value grammar the generated-content work added.
  - *Owned by M2 robustness / a hygiene pass:* eight Taffy `expect()` calls in the render path;
    `Document::append`/`insert_before` and `Handle::node()` panic paths, safe only while html5ever's
    contract holds and first exercised by M4's `MutateOp` funnel; `FetchPool::try_recv` maps
    `Disconnected` to `None`, hiding a dead pool; `Document::detach` leaks `template_contents`
    entries; `impl Node {}` is empty; `Document::children()` allocates a `Vec` per call in four
    hot traversals; and `app::render::embedded_style_sheets` is now reachable only from tests since
    `render_html` was rewired to drive `PageLoad`, so it should become `#[cfg(test)]` or its two
    call sites should move to the `PageLoad` path.
- 2026-08-23 — M1-D generated content, counters and list markers completed. No dependency was added:
  **cssparser 0.37.0** parses `content` values and **selectors 0.40.0** already modelled
  pseudo-elements, so `UnsupportedPseudoElement` became a real `PseudoElement` enum matched under
  `MatchingMode::ForStatelessPseudoElement` behind a new `MatchTarget`. This closes the same bug
  class M1-B fixed for dynamic pseudo-classes: a rule like `h1, .note::before { … }` previously
  failed to parse and was discarded whole. `ComputedStyle` stays `Copy` — generated text lives in
  `StyleTree` side tables keyed by node and pseudo-element. The Wikipedia smoke caught a real defect
  before close: author `counter-increment` on a list item was replacing the implicit `list-item`
  step that `display: list-item` implies, so every reference numbered `0.`; implicit and authored
  counter operations now merge and only a same-named counter overrides. Local proof is green at
  341 library · 5 binary · 4 pipeline · 14 corpus · 16 golden tests, and `--dump` renders the new
  fixtures and Wikipedia over the real network without a panic. The Wikipedia render also confirmed
  the length-unit defect from the audit above — `padding-left: 20px` indents twenty terminal cells —
  which is why that item is sequenced next.
- 2026-08-23 — **Whole-crate audit closed; affected milestones reopened before more M1-D work.**
  Every first-party Rust target, test/support helper, fixture, snapshot, manifest and user-facing
  status claim was inspected; vendored corpus bytes and transitive source were intentionally
  excluded. Baseline and all five local gates were green at `ba76d50`; no source, dependency,
  snapshot or generated-corpus change was made. Local `--dump` fixtures, existing tests/goldens and
  a public-API harness confirmed the findings now written into their owning tasks:
  - M1-B: empty `alt=""` renders `[img]`; transparent foreground falls back to visible theme text
    and partial alpha is discarded. M1-C: a late stylesheet can shrink a 100-row page to zero while
    leaving scroll at 98. M1-D tables: Latin words split mid-word, `nowrap` preserves newlines,
    nested/inline tables lose source order and inline placement, and caption box styles disappear.
  - M1-D generated/display: the green degradation test still maps authored `list-item` to block;
    CSS-wide `content`/counter winners do not clear earlier side-table values; marker `normal`
    suppresses defaults; `inline-block` degrades to block and `display: contents` is unsupported.
  - M2/M3 follow-ups: an arbitrary `<div checked>` matches `:checked`; `:focus-visible` and
    `:focus-within` are conflated with exact focus. Cross-cutting harness gaps: corpus error counts
    are parsed but ignored, and UTF-16 attribute ordering lacks an astral test.
  - Static risks were assigned without overstating them as product reproductions: duplicate pool
    submissions are silently dropped, repeated template-content creation can orphan the old
    fragment, and painter rows are dense across document height. The current ureq 3.4.0 docs still
    confirm default 4xx/5xx-as-error behavior, so M2's existing keep-the-404-body task remains the
    correct owner. CSS/HTML expectations were checked against the latest published W3C/WHATWG
    Display, Tables, Text, Lists, Generated Content, Color, Selectors and `img` requirements.
- 2026-08-23 — **M1-C late-repaint scroll clamping restored.** Every `RenderedPage` application now
  clamps the tab scroll against the current content rows. Controller regressions reproduce the old
  `scroll=98`-against-zero failure for both an active late stylesheet and a stylesheet delivered to
  a background tab before activation. All five local gates are green at 343 library tests; M1-C is
  done again with only the human terminal smoke pending.
- 2026-08-23 — **M1-B audit repair started.** The foreground colour contract now retains byte alpha
  from cssparser-color 0.5.0 through cascade and layout, resolves it against the effective painted
  background, and suppresses transparent ink without changing layout or interaction geometry.
  Background and border alpha keep their existing terminal degradation; the separate image repair
  centralizes nonempty, empty and missing `alt` behavior across normal and table layout.
- 2026-08-23 — **M1-B audit regressions closed.** No dependency changed: cssparser-color 0.5.0
  remains current. RGBA/HSL/HWB foreground alpha survives cascade and inheritance, transparent ink
  becomes same-width blank cells while link/hit geometry remains live, and partial foregrounds
  composite over the deepest background before contrast correction. Normal and table layout now
  share the nonempty/empty/missing image fallback. All five local gates are green at 347 library ·
  5 binary · 4 pipeline · 14 corpus · 17 render-golden tests; M1-B is done again with only the human
  terminal smoke pending.
- 2026-08-23 — **M1-D table audit regressions closed.** No dependency changed: **textwrap 0.16.2**
  and **Taffy 0.13.0** remain current. Normal flow and table cells now use one private inline
  formatter; ordered nested-table atoms preserve block and inline source placement; depth-limited
  metrics are cached and degraded rendering retains descendant text. Fixed layout honors its
  declared width before grapheme-safe clipping. Styled captions own border/padding/background/hit
  geometry, consecutive improper children share anonymous cells without synthetic DOM nodes,
  `colgroup` widths and inherited table properties contribute normally, and clipped nested strokes
  discard only the removed outer edges. The focused nested/fixed goldens were inspected and updated.
  All five local gates are green at 348 library · 5 binary · 4 pipeline · 14 corpus · 24
  render-golden tests; M1-D remains open for the separately owned generated-content/display work,
  length units, presentational HTML, alignment and typography. Human terminal smoke remains pending.
- 2026-08-23 — **Project structure pass: single-responsibility modules (user).** No dependency
  changed and no behavior changed. Module *layering* was already sound (no cycles, no upward
  production imports); the problem was inside the modules, where several files held four to seven
  unrelated jobs. Decisions recorded above: `app` becomes the composition root only, the rendering
  pipeline is promoted to a new top-level `pipeline` module, `main.rs` is the terminal adapter only,
  and "a module is a responsibility, not a file" is now an AGENTS.md rule set rather than an
  aspiration. Path references in this document were re-pointed: `css/cascade.rs::ua_style` →
  `css/ua.rs::ua_style`, the `content`/`counter-*`/`list-style*` audit row → `css/cascade/{content,
  counters}.rs` + `css/values.rs`, `net/encoding.rs` sniffing → `net/encoding/prescan.rs`, and the
  proptest-law owners → `layout/engine/tests.rs`, `layout/table/tests.rs`, `app/controller/tests/`.
  The 2026-08-22 updates-log entry is left as written: it is history, and the `css/ua.rs` file it
  says never existed does exist now. The status-board test line was stale (347/17) and is corrected
  to the 348 library · 5 binary · 4 fetch-pipeline · 14 corpus · 24 render-golden baseline captured
  before the pass. The refactor lands as one commit per module with all five gates green after each;
  the one deliberate test movement is `dump_lines` and its case going from the binary to `pipeline`
  (binary 5 → 4, library +1). `layout/table.rs::layout_model` is the sole non-mechanical extraction
  and gets its own reviewed commit.
- 2026-08-23 — **Project structure pass continuation corrected before code.** The first six slices
  through the pipeline extraction are already landed; the remaining work stays uncommitted at the
  user's direction. The continuation closes residual responsibility leaks in the touched pipeline,
  CSS, tab, and terminal-adapter modules before splitting table layout. Table layout is separated
  mechanically first, then its orchestration is expressed as model, geometry, caption, column,
  cell, and placement phases without changing caption-width feedback, recursion, clipping, paint
  order, or merge identifiers. The remaining engine, DOM, style, painter, encoding, controller,
  and script-test splits preserve their existing module-level public paths and test-module names.
  The verified pre-continuation baseline is 349 library · 4 binary · 4 fetch-pipeline · 14 corpus ·
  24 render-golden tests in both default and `js` configurations; no dependency changes are needed.
- 2026-08-23 — **Project structure pass continuation complete, left uncommitted.** No dependency or
  behavior changed. Pipeline rendering and dumping, stylesheet URL normalization, CSS selector
  parsing/matching, tab state, and terminal-adapter tests now have responsibility-specific modules.
  Table formatting is split into model, sizing, content, border, caption, geometry, and placement
  responsibilities, with the orchestration retaining its explicit measurement-to-placement phase
  order. Layout engine, DOM, computed style, painter, encoding, controller, and script tests now
  follow the directory-module and sibling-test rules; controller tests are further grouped by
  delivery, key/viewport, and navigation responsibility. Public paths, table merge identities,
  include paths, and depth-relative visibility were audited after the moves. All five local gates
  are green at 349 library · 4 binary · 4 fetch-pipeline · 14 corpus · 24 render-golden tests in
  both default and `js` configurations; the selector-bucketing benchmark also builds. No lower
  layer imports `crate::app`, no `.snap.new` was created, and the human terminal smoke remains
  pending.
- 2026-08-23 — **Structure-pass documentation aligned for the local commit.** The README architecture
  now includes the promoted `pipeline` subsystem and the current composition boundaries; the
  standing module rule now covers both sibling test-file forms. Historical update entries remain
  unchanged. The user approved one local commit for the uncommitted
  continuation, superseding its earlier per-module commit outline. No source or dependency changed
  in this documentation follow-up.
- 2026-08-23 — **M1-D generated-content audit regressions closed.** No dependency changed:
  cssparser 0.37.0 and selectors 0.40.0 remain current, and no new helper was needed —
  `css/values.rs::is_css_wide_keyword` already existed and already covered the exact set
  (`initial | inherit | unset | revert | none`) that must clear a side table. Each of the three
  failures turned out to belong to one value parser rather than to the cascade loop, which is why
  the fixes are three small edits: `display: list-item` was bundled into the arm mapping the modes
  that genuinely degrade to block; `parse_content` and `parse_counter_values` reported the CSS-wide
  keywords as *invalid* rather than as "names nothing", and an invalid declaration leaves the
  earlier winner standing — the opposite of what a later declaration should do; and `content:
  normal` was collapsed onto `content: none`, so an author asking for the UA marker got no marker.
  Two deliberate approximations are recorded in the code: `inherit` on `content`/`counter-*`
  resolves as if it were `initial`, since parent counter values are not modelled, and an empty
  counter list still merges with the implicit `list-item` operation, so `counter-increment: initial`
  does not stop a list numbering. `counter-reset: none` was already correct and is untouched.
  Each new test was run against the unfixed source and observed to fail. The M1-D risk-register row
  on pseudo-box background ownership is closed as **disproved**, not deferred: a pseudo box does
  inherit the non-inherited `background`, but generated content is inline-level and the outside
  marker's field is reserved inside the item's own box, so it can only ever repaint what the
  originating element already painted — verified through the public render harness, including a
  pseudo declaring `background: initial`. All five local gates are green in both the default and
  `js` configurations at 353 library · 4 binary · 4 fetch-pipeline · 14 corpus · 25 render-golden
  tests; no fixture or golden moved, and no `.snap.new` was written. M1-D stays open for length
  units (next), outer/inner display modes, presentational HTML and terminal typography; the human
  terminal smoke remains pending.
- 2026-08-23 — **Framebuffer frontend added behind a non-default `vga` feature.** TextSurfer now has
  two frontends over one engine, both sitting on ratatui's `Backend` trait: the terminal build stays
  the default, and `--vga` opens a window rendering with our own CP437 8x16 face. Nothing in
  `css`/`layout`/`paint` moved — `ui::chrome::draw` already took a backend-agnostic `Frame` and `app`
  already never imported a terminal library, so the cost was a `Backend` impl plus an event-mapping
  adapter. Rationale, rejected alternatives (`mousefood`, `ibm437`, `minifb`) and the two-tier font
  licensing are in the Decisions log above. New module `src/vga/` (font · surface · backend · input ·
  window); `src/vga/font/table.rs` is generated by `tools/gen_cp437.py` and pinned by SHA-256. The
  `draft()` chrome fixture moved from `ui::chrome`'s test module into `ui::test_util` so both
  frontends' tests share it; the 8 chrome tests are unchanged. Deps (all optional): winit 0.30.12
  — the latest *stable*, 0.31 being a prerelease — softbuffer 0.4.8, unifont-bitmap 1.0.0. Release
  binary 5.1 MB default → 6.5 MB with `vga`.
- 2026-08-23 — **Launch-time stack overflow found and fixed; test-harness stacks were hiding it.**
  `unifont_bitmap::Unifont` is 139,264 bytes *by value* (a 4,352-entry page table held inline), and
  `Unifont::open` materialises it through several by-value locals that a debug build does not elide.
  Constructing it on the main thread overran the 1 MB stack Windows gives that thread, so the
  frontend died on launch with `STATUS_STACK_OVERFLOW` before drawing anything — while every test
  passed, because the harness runs tests on threads with a 2+ MiB stack and so never exercised the
  real constraint. `vga::font::unifont` now builds the font on a 4 MB worker thread and moves a
  `Box<Unifont>` back, keeping the large frame off the caller's stack entirely. Pinned by
  `the_frontend_starts_within_a_main_thread_stack`, which drives the real launch path on a
  deliberately 1 MB stack; it was confirmed to reproduce the overflow before the fix. **Process
  lesson: a green suite does not establish that the binary starts** — assert launch-path limits
  explicitly rather than inheriting the harness's more generous environment.
- 2026-08-24 — **M1-D length units started after a green six-gate baseline.** The item contract now
  fixes the previously unspecified unit subset, 8×16 cell metric, 16px root-font approximation,
  nearest-cell layout rounding, exact CSS-pixel media comparisons and MQ4 range forms before code
  depends on them. The VGA backend's real 8×16 font constants will be checked against the shared
  default. `cssparser 0.37.0`, Taffy 0.13.0 and textwrap 0.16.2 remain the latest published
  versions; no dependency change is planned. Baseline: 353 library · 4 binary · 4 fetch-pipeline ·
  14 corpus · 25 render-golden tests, plus 412 library tests with `--features vga`.
- 2026-08-24 — **M1-D length units and the cell metric completed.** A shared `CssLength` and
  injected 8×16 `CellMetric` now resolve supported absolute, font-relative and viewport-relative
  units per axis while media dimensions compare unrounded CSS pixels. Legacy media dimensions and
  MQ4 feature-first, value-first and chained ranges share the same conversion path. Block and table
  min-content sizing now use the widest unbreakable segment, cell-intent fixtures use `ch`/`rem`,
  and the 79/80-column responsive fixture pins the 640px boundary. The `vga` feature asserts that
  the shared default matches its real `CELL_W`/`CELL_H` constants. No dependency changed. All six
  local gates are green: 362 library · 4 binary · 4 fetch-pipeline · 14 corpus · 26 render-golden
  tests, plus 422 library tests with `--features vga`; no `.snap.new` file was produced.
- 2026-08-24 — **M1-D outer/inner display modes started after a green six-gate baseline.** The
  computed display contract follows CSS Display Level 3's outside/inside, box-generation and
  table-internal categories. Layout will build a private formatting tree, elide `contents` boxes
  without changing DOM inheritance or link ancestry, and apply CSS 2.2 anonymous-table fixup after
  that elision. Ladybird's staged tree builder is the comparison implementation, not a source port.
  Flex/grid keep their computed inner modes but use normal-flow layout until M6. `cssparser 0.37.0`,
  Taffy 0.13.0 and textwrap 0.16.2 remain current; no dependency change is planned. Baseline: 362
  library · 4 binary · 4 fetch-pipeline · 14 corpus · 26 render-golden tests, plus 422 library tests
  with `--features vga`.
- 2026-08-24 — **M1-D outer/inner display modes completed.** CSS display values now retain separate
  outside/inside, box-generation and table-internal categories through cascade and parse strict
  legacy plus multi-keyword forms. The private flow tree removes `contents` principal boxes without
  losing inheritance, pseudo content or link ancestry, and applies anonymous-table fixup after that
  elision without synthetic DOM owners. Inline flow-root/table/grid boxes are atomic with
  shrink-to-fit normal-flow content, horizontal margins and last-content-line baselines; inline
  flex uses its first flex-line baseline; block
  flow-root/flex/grid deliberately use normal flow until M6. Bounded anonymous wrappers degrade
  without dropping their text. No dependency changed; **cssparser 0.37.0**, **Taffy 0.13.0** and
  **textwrap 0.16.2** remain current. The public display-modes golden was inspected. Final six-gate
  counts: 367 library · 4 binary · 4 fetch-pipeline · 14 corpus · 27 render-golden tests, plus 427
  library tests with `--features vga`. M1-D remains in progress; presentational HTML is next.
- 2026-08-24 — **M1-D Presentational HTML started after a green six-gate baseline.** Corrected the
  item from UA-origin specificity to WHATWG's author-origin zero-specificity hint layer and pinned
  Ladybird `8baf4260d40dd53cd09c21c868d2bd0625a69149` as the comparison implementation, with WHATWG
  authoritative. No dependency is planned: html5ever 0.39.0, cssparser 0.37.0,
  cssparser-color 0.5.0 and Taffy 0.13.0 remain current; the narrow legacy-value adapters are now
  justified in the parser audit. Baseline: 367 library · 4 binary · 4 fetch-pipeline · 14 corpus ·
  27 render-golden tests, plus 427 library tests with `--features vga`.
- 2026-08-24 — **M1-D Presentational HTML completed.** Presentational attributes now enter the
  cascade as zero-specificity author hints, with attribute-dependent UA defaults kept at UA origin.
  The isolated WHATWG adapters cover legacy integers, dimensions and colours; computed styles now
  carry inherited horizontal alignment, table-cell vertical alignment and auto margins through
  normal flow, table layout, paint and link geometry. The reviewed old-school fixture covers legacy
  colours, rules, frame, cell spacing/padding, captions and alignment; existing table goldens were
  deliberately re-baselined for centered captions/header cells and baseline alignment. No
  dependency changed. Final six-gate counts: 372 library · 4 binary · 4 fetch-pipeline · 14 corpus ·
  31 render-golden tests, plus 432 library tests with `--features vga`. M1-D remains in progress;
  terminal typography is next.
- 2026-08-24 — **VGA-first continuation and M1-D typography contract revised before code.** The VGA
  window is now the active default frontend, with the terminal frozen as an explicit compatibility
  fallback and `--dump` remaining headless. The rejected half-block terminal ladder is replaced by
  cell-aligned integer scaling of the existing CP437/Unifont faces over a frontend-neutral computed
  `font-size` model. Ratatui remains the chrome renderer while VGA owns the richer text compositor.
  Existing pins remain current except the manifest floor is aligned to the already-locked
  **winit 0.30.13**; **ratatui 0.30.2**, **softbuffer 0.4.8** and **unifont-bitmap 1.0.0** remain
  unchanged. Baseline gates were green at 372 library · 4 binary · 4 fetch-pipeline · 14 corpus · 31
  render-golden tests, plus 432 library tests with VGA.
- 2026-08-24 — **M1-D VGA-native bitmap typography completed.** `font-size` now computes through a
  frontend-neutral inherited typography model, including supported lengths, percentages, absolute
  and relative keywords, CSS-wide keywords and UA heading sizes. Block and table flow share integer
  1x/2x/3x/4x sizing, per-line fit degradation, Unicode-width advances, full-height link geometry
  and zero/small-text behavior; terminal and dump preserve their historical cell output. Paint
  emits background-only shadow-cell reservations plus depth-ordered scaled runs with the existing
  alpha and contrast rules. VGA restores stale overlays, clips whole glyphs against scroll/window
  and chrome occlusion, rasterizes CP437/Unifont pixels with dim/reverse/strike support, and repaints
  the cursor last. VGA is the default feature and frontend; `--terminal` selects the frozen fallback,
  `--vga` remains compatible, and `--dump` stays headless. The inspected framebuffer snapshot has no
  `.snap.new` remainder. Final gates are green at 449 library · 6 binary · 4 fetch-pipeline · 14
  corpus · 31 render-golden tests in the default and `js` configurations, plus 385 library tests in
  the terminal-only `--no-default-features` configuration. M1-D is done; the human VGA smoke remains
  pending and the older terminal smoke is deferred while that frontend is paused.
- 2026-08-24 — **M3 pulled ahead of M2 (user) and started as slice 1, mouse navigation.** The
  milestone is re-scoped VGA-first: winit is the primary pointer adapter and crossterm keeps the
  frozen terminal aligned, replacing the old "crossterm mouse capture" wording. Two defects found
  while planning are fixed inside the slice: `chrome::content_rect` inset the content band on all
  four sides although the `Content` widget draws side rails only, so the VGA scaled-text overlay
  painted every scaled glyph one row low and clipped the first and last visible rows (confirmed M1-D
  regression, unpinned because the surface tests pass `origin = (0, 0)`); and AGENTS.md's invariant
  "mouse acts only in Tabs/Address/Content zones" contradicted M3's own menu-bar item, so it now
  reads Menu/Tabs/Address/Content with `MouseZone::Outside` inert. AGENTS.md also gains the
  `cargo test --features vga` gate and a `main.rs` description covering pointer mapping. Behaviour
  was compared against Ladybird `8baf4260d40dd53cd09c21c868d2bd0625a69149`
  (`Libraries/LibWeb/Page/EventHandler.cpp`): activation fires on mouseup against the tracked
  mousedown target, hit testing takes the topmost element in paint order — recorded above with its M6
  trigger — and wheel deltas are converted rather than fixed to a step, while side buttons and
  open-in-new-tab sit in the chrome layer. No dependency changed: winit 0.30.13 (0.31 is beta only),
  ratatui 0.30.2, softbuffer 0.4.8, unifont-bitmap 1.0.0, crossterm 0.29.0 via ratatui. Baseline
  gates green at 449 library · 6 binary · 4 fetch-pipeline · 14 corpus · 31 render-golden tests, plus
  385 library tests with `--no-default-features`.
- 2026-08-24 — **M3 slice 1 delivered.** `ChromeGeometry` now owns `content_view` and
  `target_at`, so paint and hit-testing share one screen-to-document mapping and the controller
  never sees a widget rectangle; `chrome::content_rect` delegates to it, which fixes the scaled-text
  off-by-one and is pinned by tests that fail against the old inset. `App::handle_mouse` routes
  through the existing `Action` set: menu titles toggle, popup rows dispatch, chips activate, the
  `+` box opens a tab, toolbar buttons match their keys down to the dimmed message, the address
  field takes a caret without losing a partial edit, the wheel scrolls only content, side buttons
  walk history, and links follow on release over the pressed node with `<base href>` resolution,
  middle-click and `target="_blank"` new tabs, and fragments deferred to M2. Hover previews the URL
  in the status bar, repaints only on a target change, survives scroll and tab switches, and shows
  `CursorIcon::Pointer` in the window. Both frontends map pointer events — winit primary with a
  tracked cursor cell and a pixel-delta wheel accumulator, crossterm with capture enabled around the
  loop and a panic hook, since `ratatui::restore` leaves capture on. Final gates green at 492
  library · 9 binary · 4 fetch-pipeline · 14 corpus · 31 render-golden tests in the default, `js` and
  `vga` configurations, plus 420 library · 8 binary with `--no-default-features`; no `.snap.new`
  remains. Slice 1 is done pending the human mouse smoke; slice 2 (`:hover`/`:focus*` liveness,
  theme states, the CSS `cursor` property) stays open.
- 2026-08-24 — **M3 slice 1 partial human smoke (user).** In the default VGA window, wheel scrolling,
  link clicks, toolbar buttons and the menu bar all behave. Not exercised yet, so not claimed:
  the hover URL preview and hand cursor, tab-chip and `+` clicks, address-field caret placement,
  middle-click and `target="_blank"` new tabs, the Back/Forward side buttons, and the terminal
  frontend (`--terminal`) including that the shell is left clean after quitting and after a panic.
- 2026-08-24 — **M3 slice 2 implementation plan corrected before code.** Hover becomes
  inline-precise through fragment hit regions; `:hover`/`:active` match ancestor chains and the three
  focus pseudo-classes stay distinct behind a pointer/keyboard focus source ready for M2. Stateful
  renders reuse `PageLoad` instead of adding a parallel render request API. Restyles are dependency-
  gated, preserve load progress and messages, and cap pointer re-hit feedback at one additional
  render. Only the consumed hover accent lands in M3; selected/search-match colours remain in M2.
  CSS `cursor` is inherited and covers the full predefined winit 0.30.13 set plus `none`; custom
  images are syntax-checked and skipped in favour of their mandatory supported fallback. No crate
  pin changes: selectors 0.40.0, cssparser 0.37.0 and stable winit 0.30.13 remain current for this
  slice. Baseline gates are green at 492 library · 9 binary · 4 fetch-pipeline · 14 corpus · 31
  render-golden tests, plus 420 library · 8 binary with `--no-default-features`.
- 2026-08-24 — **M3 slice 2 delivered; human smoke pending.** Text fragments now contribute
  depth-ordered `HitRegion::Text` entries above equal-depth boxes, so inline elements receive live
  hover while link activation deliberately remains on its M6-tracked document-order contract.
  Selector state distinguishes hover/active chains, exact focus, focus-within and focus-visible with
  pointer/keyboard sources; parse-time `StateDeps` recursively covers nested selector lists and media
  rules. The review found one further selectors 0.40.0 requirement and pinned it: generated pseudo-
  elements must both accept state pseudo-classes and classify all focus variants as user-action
  states. `PageLoad::set_dynamic_state` stores prepaint state, gates later cascade→layout→paint runs
  on author or UA dependencies, preserves progress/messages, suppresses same-link UA re-hits and
  caps render→re-hit feedback at one additional render. Tabs retain pointer DOM focus independently;
  chrome focus hides it; navigation clears it; primary/middle press tracking keeps CSS active and
  link activation separate and ignores mismatched releases. The Norton theme supplies a light-cyan
  hover link accent. Inherited CSS `cursor` covers every standard cursor represented by stable winit
  0.30.13 plus `none`; cssparser 0.37.0 validates URL/image-set fallback syntax and custom images are
  skipped, not loaded. Native cursor sync runs after input and periodic fetch delivery, including
  hidden→visible restoration, without making cursor-only movement dirty the canvas. No dependency
  changed: selectors 0.40.0, cssparser 0.37.0 and winit 0.30.13 remain pinned. Final gates are green
  at 513 library · 9 binary · 4 fetch-pipeline · 14 corpus · 32 render-golden tests in default,
  `js` and `vga` configurations, plus 440 library · 8 binary with `--no-default-features`; no
  `.snap.new` remains.
- 2026-08-24 — **M3 slice 2 reopened after an interactive launch freeze.** Both the default window
  frontend and the `--terminal` frontend can consume one main-thread core and become unresponsive,
  including on the blank start page, until TextSurfer is force-closed. The earlier conclusion that
  the terminal frontend remained responsive was disproved by the later live reproduction. Diagnosis
  proceeds through injected-time, window-free event, tick and redraw budgets before any further
  native launch. Slice 2 is not done while this regression remains unexplained.
- 2026-08-24 — **VGA freeze diagnostic tests completed; native cause remains open.** Eight
  window-free contracts prove that blank launch, a pending URL, 200 injected 50 ms ticks, duplicate
  physical resize, a headless redraw, 1,000 identical start-page pointer moves and native cursor
  transitions all reach a bounded clean state. A separate author-page defect is confirmed by an
  opt-in failing reproducer: `span:hover { display: none }` alternates the hit target on every
  identical parked-pointer event, so each event performs the capped render/re-hit pair and dirties
  again. It cannot explain the blank-page freeze, which has no DOM or stylesheet. The reproducer is
  ignored in ordinary gates while the defect remains open. No native window was launched. Gates are
  green at 520 passing + 1 ignored library · 9 binary · 4 fetch-pipeline · 14 corpus · 32
  render-golden tests in default, `js` and `vga`, plus 440 passing + 1 ignored library · 8 binary
  with `--no-default-features`; the opt-in hover reproducer fails as intended.
- 2026-08-24 — **Cross-frontend freeze diagnostics expanded.** The existing headless checks do not
  drive either real interactive loop and therefore cannot exclude an event-intake spin. Both loops
  will be exercised through scripted event sources with operation-count budgets for dispatch, tick,
  dynamic restyle and redraw work; duplicate cell motion, sustained event queues, resize/redraw
  feedback and the parked-pointer hover oscillation are required regression cases.
- 2026-08-24 — **Cross-frontend event-spin guards implemented; human confirmation pending.** The
  shared controller now drops duplicate same-cell motion before hit testing or dynamic restyling,
  which turns the formerly ignored layout-changing hover oscillation into a passing 100,000-event
  regression. VGA also suppresses duplicate mapped-cell motion at its adapter boundary. The terminal
  loop is driven through an injected scripted event source in tests, yields for 1 ms after each burst
  of 64 immediately-ready events, and has controlled poll/read/draw error coverage. A scripted burst
  exposed a separate terminal feedback defect: 128 duplicate 80x24 resize reports caused 129 draws;
  unchanged `App::on_resize` calls are now inert and the same script draws only startup and the final
  focus change. `App::step` caps fetch-result delivery at 256 results so an adversarial source cannot
  starve either frontend. All gates are green at 522 library · 13 binary · 4 fetch-pipeline · 14
  corpus · 32 render-golden tests in default, `js` and `vga`, plus 442 library · 12 binary with
  `--no-default-features`; no ignored tests remain. Native launches were deliberately not run during
  automated diagnosis.
- 2026-08-24 — **Retained-frame repair replaces the event-spin workaround.** Live testing still
  reports heavy/frozen scrolling and resizing in both frontends. The architectural cause is the
  callback-to-App-to-layout-to-presentation path: a boolean dirty flag cannot preserve scroll,
  row-paint and full-layout intent, and VGA still rasterizes/copies broad regions per event. The
  accepted repair owns the retained Ratatui frame, batches input into one App transaction, retains
  the PageLoad scene by stage, scrolls backend row regions, and presents age-correct pixel damage.
  Ratatui 0.30.2 and softbuffer 0.4.8 remain current; Ratatui enables `scrolling-regions`. Winit
  remains on stable 0.30.13 rather than the published 0.31 prerelease. The 64-event/1 ms terminal
  sleep is removed. M3 slice 2 remains in progress until native VGA and terminal smoke pass.
- 2026-08-24 — **Retained-frame repair implemented; native confirmation pending.** Both adapters
  now coalesce continuous input into one controller advance and consume semantic frame damage. A
  shared retained Ratatui composer scrolls the content region and diffs the resulting buffer; the
  VGA backend moves existing cell/pixel rows, preserves scaled overlays across pure scrolls, reuses
  resize allocations, tracks bounded pixel damage, unions intervening damage for older softbuffer
  back buffers, and resizes softbuffer only when the physical size changes. Status-only link hover
  no longer invalidates page content, no-op scroll boundaries are inert, and wheel magnitude is
  preserved. Automated gates are green at 530 library · 13 binary · 4 fetch-pipeline · 14 corpus ·
  32 render-golden tests in default, `js` and `vga`, plus 446 library · 12 binary with
  `--no-default-features`; native VGA and terminal smoke remain human-run acceptance work.
- 2026-08-24 — **Retained-frame claims reopened after continued live latency.** User testing found
  sustained input, window movement and resize still choppy in both frontends. Inspection confirmed
  that semantic damage is followed by full Ratatui composition/diff, VGA lifecycle work is driven
  by every `about_to_wait` wake, resize reports synchronously reflow every tab, dynamic state is
  committed inside individual event handlers, and VGA cell/pixel damage still expands into broad
  repeated work. Slice 2 now requires an injected-clock frame scheduler, one controller transaction
  per rendering opportunity, preview/settled resize, conservative dynamic-selector invalidation and
  genuinely bounded retained presentation before the three performance claims can be checked again.
- 2026-08-24 — **Frame-cadence and retained-presentation repair implemented; human smoke pending.**
  A shared fake-clock-tested scheduler now caps sustained input at 60 Hz, preserves discrete input
  across a 256-event fairness budget, and leaves both adapters asleep when no input or load is
  pending. Controller input is one transaction; hover cascade and page resize reflow settle once
  after 50 ms quiet. Ratatui now mutates and submits only status, exposed scroll rows or explicit
  content damage. VGA batches cell shadow updates before raster, filters scaled-text scroll work to
  exposed rows, keeps up to 32 pixel regions and eight frames of buffer-age history, and ignores
  unsolicited no-damage redraws. Windows resize increments follow the physical 8x16 cell and debug
  builds use `opt-level = 1`. All format, strict Clippy and default/JS/VGA/no-default gates are green
  at 536 library tests in the default feature set and 452 without default features, plus 13/12 binary,
  4 fetch-pipeline, 14 corpus and 32 render-golden tests. Native VGA and terminal responsiveness
  remain human-run acceptance, so M3 stays in progress.
- 2026-08-24 — **Post-repair human smoke still fails; work preserved for independent review.** The
  user reports unresolved lag after the scheduler, resize-settle and retained-presentation repair,
  so the three performance claims above are reopened even though all synthetic work-count and local
  feature gates pass. The current attempt is isolated on `review/m3-input-latency` for another agent
  to inspect before merge. It includes the 60 Hz shared scheduler, one input transaction per frame,
  50 ms hover/resize settlement, partial Ratatui composition, batched VGA cell raster, bounded pixel
  regions with buffer-age history, no-damage redraw suppression and debug `opt-level = 1`. No live
  trace was captured while the problematic process remained open, so the next investigation must
  measure the running native and terminal processes rather than accepting deterministic tests as a
  proxy for responsiveness. Do not merge or mark M3 done until both frontends pass the manual smoke.
- 2026-08-25 — **VGA scroll correctness repaired (three defects), with the end-to-end tests the
  path was missing.** The retained-presentation review found the "frozen bulk with drifting
  fragments" scroll bug had two causes plus a page-down crash, all in `src/vga/surface.rs` and all
  invisible to the existing `TestBackend`-only composer test: (1) `Surface::scroll_rows` handed
  `mark_damage` a *flat buffer offset* as the y coordinate and a byte length as height, so any band
  below row 0 — i.e. every real content band under the chrome — was rejected by the bounds check and
  reported **no pixel damage**; `present()` then re-copied only the freshly-composed exposed strip
  and left the scrolled bulk stale. Fixed to mark real pixel coordinates. (2) Scaled headings
  (`draw_scaled_text`) were dropped whole when they straddled a viewport edge (`contains` full
  clip + `run.rect.row < scroll` early-drop), so edge headings popped in/out and vanished on any
  full repaint; replaced with per-pixel clipping to the content rect via a signed origin, so partial
  headings render clipped instead of disappearing. (3) `scroll_rows`' overlay-cell shift used
  `then_some((col, row - amount))`, whose eager argument underflows `row - amount` for a heading
  above the fold on a large scroll — Space/page-down crashed the window; fixed with lazy `then`.
  New coverage in `src/vga/tests/scroll.rs`: real-`VgaBackend` incremental-vs-fresh pixel
  equivalence across sequential scrolls, a below-the-top scroll-damage assertion, a page-sized
  composer scroll, and a full `VgaApp` load→focus→Space page-down smoke over a fake fetcher; plus
  rewritten surface clip tests. All format, strict Clippy and full gates green at 543 library tests.
  Softbuffer's Win32 backend was confirmed a single retained DIB (`age()` always 1), so the
  buffer-age/`damage_history` machinery is dead there and flagged for later removal. Native VGA
  smoke of scrolling still owed by the user; the fluency work (item two/three above) stays open.
- 2026-08-25 — **M3 slice 3 added and delivered: the two chrome affordances the pointer was still
  missing (user).** A tab could only be closed from the keyboard or the File menu — and only the
  active one — and nothing on screen said where in a page you were. Both are now pointer targets.
  Chips carry a Turbo Vision `[■]` close box (CP437 0xFE, so the VGA face draws it in the DOS font
  rather than the Unifont fallback tier), three columns wider per chip, which makes the `…»` overflow
  stub appear one tab sooner on a narrow strip — accepted over shortening `MAX_TAB_TITLE`. The
  content frame's right rail becomes the page scrollbar (`▲ ▒ █ ▼`), taking the rail over rather
  than claiming a column, so `content_cols` is unchanged and nothing reflows. Three decisions worth
  keeping: the scrollbar is a *target* inside the content zone, not a zone of its own, matching how
  toolbar buttons live inside the address zone; caps and trough dispatch the existing `Scroll*`
  actions, and only the thumb drag is new, because no action can name an absolute position; and
  thumb placement rounds down while its inverse rounds up, which is the only pairing that
  round-trips a drag (rounding both ways down makes the thumb crawl backwards under the pointer —
  caught by `dragging_to_a_row_and_reading_it_back_lands_on_the_same_row` before it ever ran).
  Two defects found on the path and fixed here: the retained composer scrolls a full-width region,
  so the thumb rode up with the text until every content-damaged frame was made to repaint the
  column (all three of `retained_scroll_matches_a_fresh_composition`,
  `a_retained_scroll_redraws_the_thumb_it_dragged_along` and the real-pixel
  `incremental_scroll_matches_a_fresh_full_compose_on_the_real_surface` fail without it); and
  `close_tab` never called `activate_current`, so the tab that came to the front kept a stale page
  until an unrelated event poked it. `TabManager::close_active` became `close(index, fresh)` with a
  lazily-built `FreshTab`, which also stops a start page being rendered for every close that throws
  it away. All format, strict Clippy and default/JS/VGA/no-default gates green at 575 library tests
  (543 without default features), plus 13 binary, 4 fetch-pipeline, 14 corpus and 32 render-golden.
  **Open for the user's eye:** a document that fits paints the whole track as a solid `█` column —
  visible in the regenerated 80x24 chrome golden. It follows from "the thumb fills the track", but
  if it reads too loud in the window, drawing the bare `▒` track when `max_scroll == 0` is a
  one-branch change. Human smoke of both affordances, in the VGA window and the terminal, is owed.
- 2026-08-25 — Default theme switched to Borland Turbo Vision (user, from the Turbo C++ 3.0 About
  screenshot): blue desktop `(0,0,170)`, yellow text, bright-cyan frames, cyan dim text, bright-green
  links, white hover, grey bar with black text and red mnemonics, and a green selection bar.
  `Theme` gained `selected_text`/`selected_bg` so `selected()` is themed rather than fixed
  black-on-white; `theme::DEFAULT` is the single name production and tests draw from, with `NORTON`
  kept as an alternate.
- 2026-08-25 — **Selectable retro themes follow-up locked (user; in progress).** The View menu will
  expose Turbo Vision, Norton, Amber CRT, Green Phosphor and Paper White as one App-owned,
  session-only choice, with no cycle key, persistence or dump flag. Paper White maps to
  `prefers-color-scheme: light`; the other four map to dark. Active pages repaint immediately and
  recompute stylesheet settlement because a scheme change can make a pending external sheet
  applicable; background pages defer through `render_dirty`. VGA must replace its default colours
  and force a physical-buffer fill so window margins cannot retain the previous background.
- 2026-08-25 — **Selectable retro themes follow-up delivered.** View now selects all five palettes
  by keyboard or mouse and marks the current session choice. The App injects each palette and its
  light/dark appearance into new, active and background `PageLoad`s; scheme changes recompute
  applicable external-sheet settlement without blanking an already painted page. The VGA frontend
  refreshes reset colours and retains a forced full physical-buffer fill until presentation
  succeeds, including the right and bottom margins. README usage and the popup snapshot are current.
  The complete local format, strict default/all-feature/no-default Clippy and
  default/JS/VGA/no-default test matrix is green: 594 library tests with default, JS and VGA
  features; 501 without defaults; 13 binary tests (12 without defaults), 4 fetch-pipeline, 14 corpus
  and 32 render-golden tests. Human smoke remains owed in both VGA and terminal frontends against
  `https://example.com`; no dependency pins changed.
- 2026-08-25 — **Taffy 0.14 baseline upgrade started before M6 flexbox (user).** Upgrade Taffy
  independently from 0.13.0 to latest published 0.14.0 while preserving the block-only feature set
  and current rendering. The only expected source migration is the new `LayoutInput`/`LayoutOutput`
  leaf-measure callback; flexbox, grid, float and parse remain disabled. Existing goldens and the
  complete feature gate matrix must stay unchanged before the prerequisite is done.
- 2026-08-25 — **Taffy 0.14 baseline upgrade delivered.** Taffy is pinned at 0.14.0 with only
  `std`, `taffy_tree`, `block_layout` and `content_size`; the lockfile changed only the Taffy version
  and checksum. The layout engine now routes its existing intrinsic text and table measurement
  through Taffy's 0.14 `compute_leaf_layout` callback, preserving block-only behavior. Format and
  strict default/all-feature/no-default Clippy are green. Default, JS and VGA runs each pass 594
  library, 13 binary, 4 fetch-pipeline, 14 corpus and 32 render-golden tests; no-default passes 501
  library, 12 binary and the same integration/golden sets. No snapshot changed and no `.snap.new`
  file was produced.
- 2026-08-26 — **Next-work and image architecture re-audited; roadmap corrected (user).** The
  user-run `cargo update` resolves latest indextree 4.9.0, whose current documentation identifies
  `append_value` as the constant-time fast path for creating and appending a child; the API itself
  predates 4.9.0. That directly invalidates the risk register's
  older "not fixable at the call site" conclusion: the current `Document::append` is still
  quadratic until migrated, so this focused M2 robustness item is now the immediate prerequisite
  and must pass an isolated 100,000-node watchdog case plus the complete gates before feature work.
  Images are promoted next, ahead of remaining M2 work and floats, as one required delivery across
  both frontends. Latest audited pins are `image` 0.25.10 for bounded PNG/JPEG/WebP/first-GIF-frame
  decoding and `ratatui-image` 11.0.6 only for terminal Sixel/Kitty/iTerm2/sliced/primitive-halfblock
  output, both without default features; the default VGA frontend uses its native framebuffer.
  The shared resource/decode, replaced-layout/paint, VGA and terminal subitems each own contract
  tests and a full gate run. Terminal high-resolution protocols degrade to halfblocks for complex
  clipping or occlusion, `--dump` keeps the existing alt fallback, and immutable CPU image work is
  the only planned expansion of the thread invariant. The terminal-cell WPT profile remains honest:
  image cases stay excluded until a separately reviewed `vga-pixel-v1` oracle exists. This entry
  records a plan only; no item is in progress or done, no dependency pin has landed, and the user's
  unverified lockfile update is preserved for its own later gate run.
- 2026-08-26 — **Linear-time DOM construction landed.** Pinned indextree 4.9.0 and changed only
  `Document::append`'s fresh-child path to `append_value`; operations over existing `NodeId`s retain
  `Document` validation and indextree's checked mutations. The new isolated watchdog regression
  first timed out and killed the old 100,000-node construction after ten seconds, then passed in
  0.26 s after the migration. Direct root/sibling/parent and move-cycle cases remain green. The
  complete local matrix passed: fmt; strict default, all-feature and no-default Clippy; default,
  JS, explicit-VGA and no-default tests. M6 Images is now the explicit next task.
- 2026-08-26 — **Image dependency MSRV raised to 1.90 before implementation.** `image` 0.25.10
  itself supports Rust 1.88, but latest `ratatui-image` 11.0.6 resolves `icy_sixel` 0.5.1 and
  `quantette` 0.6.0, whose published floor is Rust 1.90. TextSurfer now declares Rust 1.90 rather
  than claiming an unsupported 1.88 terminal-image graph; default features remain disabled on both
  image crates.
- 2026-08-27 — **M6 Images implemented; human VGA/terminal smoke pending.** Static `<img src>` now
  resolves and deduplicates same-scheme page subresources without blocking first paint, decodes
  signature-selected PNG/JPEG/WebP and first-frame GIF through a bounded generation-tagged worker,
  rechecks redirected final schemes, and enforces per-image and aggregate fetch/RGBA limits. Pending and failed resources retain the
  established alt fallback; decoded replacements preserve intrinsic aspect ratio and CSS sizing in
  inline, block, table, flex and grid layout, including percentage bases, clipping, links and DOM-
  ordered overlay/hit behavior. VGA nearest-samples visible RGBA directly into the retained
  framebuffer with alpha, scroll, occlusion, damage restoration and cursor-last behavior. Terminal
  output uses one bounded off-thread `SlicedProtocol` preparation worker after one capability query,
  falls back per placement to alpha-composited halfblocks for clipping, occlusion or later overlaps,
  and repaints image content rather than using the retained scroll shortcut. The dependency audit
  remains `image` 0.25.10 and `ratatui-image` 11.0.6 at Rust 1.90; the user's lockfile refresh includes
  ordered-float 5.5.0. The complete format, strict default/all-feature/no-default Clippy and
  default/JS/VGA/no-default test matrix is green: 854 library tests with default, JS and VGA, 760
  without defaults, plus 13/12 binary, 6 fetch-pipeline, 14 corpus, 42 render-golden and the static
  WPT supervisor targets. No snapshot changed. Wikipedia image smoke in both frontends remains the
  final acceptance gate, so the item is done rather than complete.
- 2026-08-27 — **Image acceptance reopened and rendering hardening promoted.** A live 160-column
  Wikipedia dump exposed a zero-row search-field content box: excluding decoded images from the
  control minimum-height safeguard also replaced the border-box-adjusted intrinsic height with the
  raw one-row control height. Existing tests covered flex, border-box padding and max-height
  separately, so their real-site combination remained unguarded. Images return to in progress until
  the exact flex/search constellation has a focused regression and the user's VGA/terminal smoke
  passes. A deterministic named-panel rendering atlas now follows that fix, with semantic geometry,
  styled terminal-cell and exact VGA-pixel layers, then the pinned static-WPT runner gains its
  separately labelled `vga-pixel-v1` profile for supported raster reftests.
