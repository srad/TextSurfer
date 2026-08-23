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

### Custom-parser audit (2026-08-20, re-checked 2026-08-22)

| Parser | Status | Verdict |
|---|---|---|
| `net/encoding/prescan.rs` — WHATWG sniffing: BOM, header, meta prescan, XML fallback, x-user-defined postprocess | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode/encode-only; no sniffer exists in the ecosystem |
| `core/url.rs` — `url_fix` | not a parser | **No change** — delegates all real parsing to the `url` crate; scheme/host/search heuristics are address-bar UX behavior |
| `css/parser.rs` | library adapter | **Keep** — stylesheet/rule/declaration tokenization delegates to cssparser; selector parsing and matching delegate to selectors |
| `css/parser.rs` — terminal media-query grammar/evaluation | custom library adapter | **Keep narrow adapter** — cssparser owns tokens, blocks, delimiters, and recovery; the adapter evaluates media types plus scripting, color scheme and cell viewport dimensions. css-mediaquery 0.1.1 is an immature raw-string port without MQ5 grammar/recovery, LightningCSS has no runtime-context evaluator, rdom-tui explicitly excludes `@media`, and Stylo/Blitz/MusKitty/litehtml/Ladybird require replacement DOM/style/rendering stacks |
| `css/cascade/{content,counters}.rs` + `css/values.rs` — `content`, `counter-*` and `list-style*` value grammar | library adapter | **Keep** — cssparser owns tokenization, functions, blocks and error recovery; the adapter only maps already-tokenized values onto `ComputedStyle` fields and the counter engine. Components the terminal cannot render (`url()`, quotes) are refused so the declaration is dropped whole, per spec, rather than half-rendered |
| Table layout (M1-D) | custom, implemented | **Custom is correct** — Taffy 0.13 implements block/flex/grid and exposes `item_is_table`, but has no table algorithm. `super-table` 0.3.0 accepts string matrices rather than a foreign styled box tree; `iris-layout` 0.4.0 has no integrated CSS table formatter. Neither supplies CSS anonymous-table fixup, spans, captions, border conflict resolution, or nested box layout |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` fixture format; this stays isolated from production code |

### Prior-art audit (2026-08-22)

Conformance is evidence, not reimplementation — and the same discipline applies to product
behavior. Terminal browsers have already settled several questions we were answering ad hoc.

| Source | What it establishes | Adopted here |
|---|---|---|
| [chawan](https://github.com/sourcehut-mirrors/chawan) (`doc/css.md`) | The terminal CSS contract: author colours **contrast-corrected against the terminal background**; `border-*-width` is **binary**; `font-weight > 500` = bold, `font-size` ignored; `text-decoration` underline/line-through; sub-cell inline margins/padding ignored; overflow-x displays, overflow-y clips, no scrollbars; `::before`/`::after` + counters + `list-style-type` for markers; link markers/hints for keyboard navigation | All locked as decisions below; markers landed in M1-D, link hints scheduled in M2. `font-size` ignored is the one item we depart from: the M1-D typography item replaces it with a half-block glyph ladder |
| chawan + [w3m](https://w3m.sourceforge.net/) | A real **table layout** engine (colspan/rowspan) is what separates a usable terminal browser from lynx | M1-D, ahead of flex/grid |
| lynx · w3m · chawan | Every one ships a **non-interactive dump mode** | `--dump` in M1-B; doubles as the golden-fixture harness |
| [Blitz](https://github.com/DioxusLabs/blitz) | Mirrors our decomposition — DOM + style + **Taffy for boxes** + a separate text layer (Parley there, textwrap fragments here) | Confirms the M1-B architecture; no dependency |
| [ratatui-image](https://crates.io/crates/ratatui-image) 11.0.6 | Unifies Sixel/Kitty/iTerm2 with a halfblock fallback and terminal font-size querying; depends on `ratatui ^0.30.1` (we pin 0.30.2) | The adopted crate for M6 images, replacing "Kitty protocol later" |
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
| M1.5 — Chrome redesign | DOS/QBasic rich UI: menu bar, tab strip, toolbar, bordered address field, centralized theme | (complete) |
| M1-B — Style, layout, paint | UA cascade, box model, whitespace, **styled paint seam**, link/hit lists, `--dump`, goldens + laws | (done — user smoke pending) |
| M1-C — External styles | Ordered `<link>`/`@import` loading, selector bucketing, `@media` features | (done — user smoke pending) |
| M1-D — Layout completeness | Table layout, generated content + list markers, length units, presentational attributes, `text-align`, terminal typography | (in progress — tables and generated content done; length units next) |
| M2 — Tabs & keyboard | Link navigation, anchors, titles, error pages, start page, in-page search, forms, robustness | (open) |
| M3 — Mouse | Zones, wheel, clicks, hover, dynamic pseudo-class state, theme states | (open) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off, pure Rust | (open) |
| M5 — Boa | Boa 0.21.1 behind trait; decision gate Boa vs Deno Core; host bindings subset; job pump | (open) |
| M6 — Stretch | Flex/grid + conformant floats, images, persistence, scroll memory, console view, config, perf gate | (open) |

Test counts at the last green run (2026-08-23): **353 lib · 4 binary · 4 fetch-pipeline · 14 corpus ·
25 golden**, and **412 lib** with `--features vga` (+59 for the framebuffer frontend).
Cross-cutting: test infrastructure (in progress: corpus error-count and astral attribute-order gaps;
contract suites, snapshots, proptest and fakes landed) · gates (done: local only, no CI) · coverage
floor (open: optional local, 80% overall / 90% css·layout·paint) ·
external conformance corpus (M1-A tree output at 95.16% raw / 100% with xfail; test262 at M5).

### Whole-crate audit risk register (2026-08-23)

Confirmed regressions are acceptance items in their owning milestones above. The following static
candidates were not promoted to confirmed bugs without an executable product reproduction:

| Owner | Unconfirmed risk or test gap | Required disposition |
|---|---|---|
| M1-B | `text-decoration` accepts known tokens from an otherwise-invalid value; inline edge cells take the parent run style; overwriting one cell of a wide glyph clears ownership/text but can retain the old style | Add focused cascade/paint cases before changing behavior; close as disproved if no reachable layout producer can expose it |
| ~~M1-D~~ | ~~Non-inherited background ownership on pseudo boxes lacks adversarial coverage~~ | **Closed 2026-08-23 as disproved.** A pseudo box does start from the originating element's computed style, `background` included, but it can never paint a cell that element did not already paint: generated content is inline-level and the outside marker's field is reserved inside the item's own box. Even a pseudo declaring `background: initial` — transparent in CSS — renders the item's background, which is what CSS requires. Pinned by `pseudo_boxes_never_own_a_background_their_element_did_not_paint` in the public render harness |
| M2 | The address edit buffer is global across tab switches; cursor placement and toolbar writes lack sub-24-column coverage; link/hit rectangles are not clipped at paint time | Resolve with the per-tab-state, tiny-chrome and link-navigation tests already owned by M2 |
| M4 | Template-content replacement is not exercised by html5ever | Exercise it at the first mutation-capable DOM caller and reject orphaning/overwriting behavior |
| M6 | Extreme injected `Size` values can make the start page allocate `cols × rows × 2`; painter output remains dense by document row | Put explicit resource ceilings and sparse-vs-dense evidence behind the perf gate |
| Test infrastructure | `tree_dump` is recursive on untrusted depth; UI clipping walks scalar values rather than grapheme clusters | Add bounded-depth and emoji/ZWJ cases; these do not currently establish a product crash |

Two candidates were closed during the audit: normal-flow `white-space: nowrap` clips rather than
wraps in the public dump harness, and `PageLoad`'s monotonic resource IDs mean the pool's silent
duplicate policy has no demonstrated current data loss (the API/diagnostic gap remains in M2).

## Gates (local only — CI deliberately refused)

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --features js          # M4+; must pass, boa feature compiles
cargo test --features vga         # framebuffer frontend; default build must stay green too
```

First snapshot write: `$env:INSTA_UPDATE = "always"; cargo test`. Coverage (optional):
`cargo llvm-cov --workspace` (needs `rustup component add llvm-tools-preview`; no thresholds enforced).

## Session handoff

1. Read this status board + Roadmap updates log (bottom).
2. Resume the next `(open)`/`(in progress)` milestone.
3. Run the gates before and after; never mark `(done)` with red gates.
4. Manual smoke list (example.com, lite.duckduckgo.com, wikipedia.org) — run by the human per
   milestone close; never part of automated tests.
5. Live repo rule: no commits without explicit user confirmation.

## Architecture (as-built)

```
 main.rs — terminal adapter only: CLI · event loop · crossterm→core::event key mapping
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
- DOM never crosses threads. One thread owns everything except I/O (fetch worker threads only).
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
  panels, bordered input field, raised tab strip, Norton-Commander palette on a navy field) around a
  normal browser layout shell — decided by the user; `ui::Theme` centralizes all colors, the painter
  and every widget draw from it; no color literals outside it.
- Product = UI and terminal frontend rendering; every other layer adopts a mature, latest-version
  crate: html5ever (HTML parse incl. tree-builder), cssparser + selectors (syntax + selector
  matching), boa_engine (ECMAScript), ureq (HTTP), encoding_rs (decode), ratatui (widgets/TUI),
  Taffy (block box geometry), textwrap + unicode-segmentation/width (inline formatting).
  In-house scope: Document arena + TreeSink glue, cascade → style tree, layout → terminal grid,
  paint/DisplayList, chrome/App/event loop, and table layout (M1-D — audited crates do not integrate
  with a styled CSS box tree).
- Single crate with modules (not a workspace) — fastest iteration; workspace split trivial later.
  Re-affirmed unchanged by the 2026-08-23 structure pass.
- **`app` is the composition root only; `pipeline` is the rendering subsystem; `main.rs` is the
  terminal adapter only.** `PageLoad`'s stylesheet resource graph, the cascade→layout→paint facade
  and `--dump` are product pipeline stages, not composition wiring — keeping them in `app` forced
  `main`, the golden tests and unit tests in `css`/`layout` to import into the composition root.
  `pipeline` sits above `paint` and below `ui`/`app`; it takes its palette by injection rather than
  reaching up to `ui::theme`.
- **A module is a responsibility, not a file.** Directory modules with one submodule per job, the
  public surface in `mod.rs`, tests in a sibling `tests.rs` or `tests/` directory. Rules live in
  AGENTS.md "Module structure"; line budgets are a prompt to look for a second responsibility, not
  a defect threshold.
- Native text renderer (lynx/w3m/chawan family), not embedded-engine (carbonyl/browsh family) — our
  value is small footprint + terminal-native layout.
- **Two frontends over one engine, behind ratatui's `Backend` trait** (2026-08-23). `ui::chrome::draw`
  takes a backend-agnostic `Frame` and `app` never imports a terminal library, so a second frontend
  costs a `Backend` impl and an event-mapping adapter — nothing in `css`/`layout`/`paint` moves. The
  terminal frontend stays the default and keeps SSH-shaped distribution; `vga` (non-default feature)
  opens a window and renders with **our own CP437 8x16 face**.
  *Why a window at all:* the DOS look is mostly the font, and inside a terminal emulator the font
  belongs to the user — `ui::Theme` fixes the palette but every glyph renders in whatever face the
  terminal was configured with. Owning a framebuffer is the only way to own the face, the cell metric
  and the palette together. It also doubles the usable columns (1280x800 = 160x50) and makes future
  image support a blit rather than a Sixel/Kitty capability matrix.
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
  border colours stay `Option<Rgb>`/`BorderColor`; `None` means "the theme decides", so the Norton
  palette stays the default field and author colours override only where declared. The painter
  composites partial foreground alpha against the effective cell background, suppresses fully
  transparent glyphs without changing their geometry, then **contrast-corrects** the resolved
  foreground (chawan's rule) — faithfulness never outranks legibility.
- **Borders are binary**: any non-zero border width paints one box-drawing frame. Replaces the
  earlier "borders ≥2 cells doubled" phrasing, which nothing implemented and which contradicts the
  prior-art model.
- `font-weight > 500` = bold; `text-decoration` maps to underline/line-through; reverse video is
  available as a style bit. `font-size` is ignored **pending the M1-D terminal typography item**,
  which replaces this rule with a half-block glyph ladder — treat it as scheduled for revision, not
  settled.
- Sub-cell margins and padding are ignored on inline boxes; CSS lengths are capped at 65,535.
- **Lengths currently ignore their unit** — `parse_length_token` rounds any `<dimension>` straight
  into a cell count, so `1px`, `1em`, `1rem`, `1pt` and `1vw` are all one cell. This was never a
  decision; it is the defect the M1-D length-units item fixes, and the earlier phrasing here ("all
  CSS lengths are cell-rounded") described it as though it were intended. Corrected 2026-08-23.
- Overflow: the x axis displays (clipped at the viewport edge), the y axis extends the document, and
  there are no scrollbars.
- `:visited` parses and **never matches** — page styling must not observe history.
- Colour **values** are parsed by `cssparser-color` 0.5.0 (same cssparser 0.37 pin), so keywords,
  hex, `rgb()/rgba()`, `hsl()` and `hwb()` all work; CIE spaces (`lab`, `lch`, `oklab`, `oklch`)
  parse but do not convert to sRGB, so such declarations are ignored rather than guessed.
- Out of scope for now, revisit when a page needs them: relative colours, `background-image`,
  `line-height`, fonts, border-radius, inline-element borders.
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
- Taffy owns block box calculation; terminal inline formatting uses textwrap fragments + Unicode
  cell/grapheme libraries because Taffy has no inline layout. M1-D adds table layout in-house; M6
  enables Taffy flex/grid; floats wait for line-flow-around-float conformance.
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
`ui::theme` with a zero-literal rule; full-width grey menu bar (row 0) with working `Alt+F/N/V/H`
dropdowns; raised NC-style tab strip with an aligned active divider; toolbar with
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
position→static, percentage heights→auto); Taffy 0.13 block geometry with anonymous boxes, margin
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

### M1-D — Layout completeness (in progress)

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
- [ ] **Outer/inner display modes.** `inline-block` is intentionally degraded to block and
      `display: contents` is not parsed, so both force visible line breaks in simple dump fixtures.
      Give `inline-block` an atomic inline box, remove the principal box for `contents` while
      retaining semantics/inheritance, and cover misparented internal table roles with the required
      anonymous wrappers. Flex/grid remain M6. *Proof:* inline sequence fixtures preserve source
      order and text flow; anonymous table fixup follows the box-tree parent requirements.
- [ ] **Length units and the cell metric** — `parse_length_token` ignores the unit and the axis, so
      `1px`, `1em`, `1rem`, `1pt` and `1vw` are all one cell: `padding: 20px` eats a quarter of an
      80-column viewport and `width: 960px` builds a 960-cell box. `parse_media_length` has the
      matching flaw on the query side (`(min-width: 640px)` can never match, `40em` is
      `MediaQuery::Never`), so both must change together or responsive sites flip to a layout
      nobody chose. Adds a cell metric (~8px × ~16px), the absolute/relative unit table anchored to
      a 16px root font size, axis-aware rounding, and MQ4 range syntax. Intrinsic sizing must change
      in both engines: block `min_content_width` and table `cell_metrics.minimum` currently return
      the widest grapheme rather than the widest unbreakable segment. Rewrites every fixture and
      golden that currently writes `px` meaning cells. *Proof:* unit-conversion tests per unit and
      axis; block and table min-content cases use whole unbreakable words; a responsive fixture
      picks the same breakpoint a browser would; existing goldens re-baselined deliberately, not
      silently.
- [ ] **Presentational HTML** — map `align`, `bgcolor`, `width`, `cellspacing`, `cellpadding`,
      `border`, `rules`, `frame`, `valign`/`vertical-align`, `<center>` and `<font color>` into the
      cascade at UA-origin specificity, plus `text-align` (left/right/center/justify→left).
      *Proof:* an old-school fixture page lays out as intended.
- [ ] **Terminal typography** — heading and `font-size` scale drawn from bitmap glyph fonts into
      half-block cells (two vertical pixels per cell, as `app/startpage.rs` already does for the
      logo): 3×4 at two rows, 4×6 at three, 5×7 at four. Ladder ≥2em → 4 rows, ≥1.5em → 3,
      ≥1.17em → 2, 1em → normal, <1em → dim, fed by UA heading sizes — which is why it follows the
      unit work. Degrade-to-fit when a scaled line will not fit (a four-row heading holds six
      characters at 40 columns), a normal-cell fallback for characters the font does not cover, and
      rules for link rects, hit-testing and search highlight inside scaled runs. Revises the locked
      "`font-size` is ignored" decision. *Proof:* ladder goldens per size, a degrade-to-fit golden,
      hit-test and link-rect round trips through a scaled heading.
- **Acceptance:** the fixture set above green; manual smoke on a table-heavy page (Wikipedia infobox)
  is readable without horizontal guessing.

### M2 — Tabs & keyboard navigation (open)

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
      error screen. Non-2xx responses must **keep the body** (`net/http.rs` currently discards it), so
      a server's own 404 page can be rendered when it is HTML.
- [ ] **Content-type honesty** — a missing or unparseable `Content-Type` currently defaults to HTML,
      so a binary body is parsed and painted as garbage. Sniff (WHATWG minimum: leading `<`,
      BOM/NUL heuristics) and otherwise refuse with the unsupported-type page.
- [ ] **Back/forward without refetching** — keep a small per-tab document cache keyed by history
      entry so Back/Forward restore instead of re-issuing a network request; the per-load pivot still
      applies to fresh navigations.
- [ ] **Render robustness** — cap DOM depth for layout (Taffy block layout recurses; a deeply nested
      hostile page can exhaust the stack) and replace the eight Taffy `expect()` calls in the render
      path with a degraded box tree plus a status-bar message. Treat fetch-pool disconnection as a
      controlled error instead of `None`, make duplicate resource submission observable instead of
      silently dropped, and ensure quit never waits on parked fetchers. *Proof:* a 100k-deep
      synthetic document renders a truncation notice instead of aborting; worker failure, duplicate
      scheduling and quit-under-stall have deterministic tests.
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
- [ ] Basic forms: text/search/hidden/submit, textarea, select, checkbox, radio; GET and
      `application/x-www-form-urlencoded` POST via `url::form_urlencoded`; unsupported
      methods/encodings render a controlled error. `:checked`/`:enabled`/`:disabled` become live and
      host-language-correct: the current attribute-only matcher lets `<div checked>` match
      `:checked` and hide from a dump fixture. Static initial state and user-toggled state share one
      form model.
- **Acceptance:** scripted-drive checklist of every keybinding incl. the rebinds; per-tab state
  isolation tests; chrome items (titles, error pages, start page, search, help overlay)
  snapshot-tested; the robustness and cache items proven by the tests named above; manual DuckDuckGo
  Lite submission works.

### M3 — Mouse (open)

- [ ] crossterm mouse capture with the same release-event gate as keys; zones wired to
      `ChromeGeometry` (Menu/Tabs/Address/Content, wheel in content, clicks set focus, tab-bar clicks
      switch, middle-click/`target=_blank` → new tab).
- [ ] Menu-bar mouse: title clicks open/drive dropdowns, item clicks dispatch, hover tracking.
- [ ] Link hover highlight (re-paint on hover-target change only) + status bar URL preview; `:hover`
      and `:focus` become live inputs to the dynamic-state evaluation added in M1-B.
- [ ] Keep `:focus`, `:focus-visible` and `:focus-within` distinct. The parser currently maps all
      three to the exact-focus state; ancestor propagation and the keyboard focus-indicator policy
      need separate selector tests before focus styling becomes live.
- [ ] Hit-test resolution contract: targets resolved by NodeId against the live document at dispatch.
- [ ] Theme extension: hover/selected/search-match states with their own accents. Snapshot goldens.
- **Acceptance:** scripted zone tests + goldens with hover states; manual mouse walkthrough.

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

### M6 — Stretch (open)

- [ ] Taffy flex/grid enabled; conformant float flow with line-flow-around-float.
- [ ] **Images** via `ratatui-image` 11.0.6 (Sixel/Kitty/iTerm2 + halfblock fallback); `[alt]` from
      M1-B stays the fallback when no protocol is available.
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
- proptest laws — all eight green. Six from M1-B: viewport-width monotonicity, painted-row bounds,
  disjoint leaf glyph cells, laminar per-row box families and engine-backed deepest-hit round trip
  (`layout/engine/tests.rs`), plus scroll clamping as a fixed point under arbitrary key sequences
  (`app/controller/tests/`). Two more came with M1-D tables (`layout/table/tests.rs`): generated
  spans never overlap, and a wider table viewport never increases height. Property tests for
  `url_fix`/`EditBuffer` continue from M0.
- FakeFetch + fake clock + fake Host; no test touches the network or the real clock.
- `tests/support/` shared corpus helpers; inline `#[cfg(test)]` fakes where module-local.
- `--dump` (M1-B) is the scriptable end-to-end harness: fixture in, golden text out.
- [ ] Corpus-harness audit follow-up: `DatCase.error_count` is parsed but never compared with
      `ParseOutcome.parse_errors`, so the published 95.16% is tree-output conformance only; compare
      error counts or explicitly justify the exclusion. The attribute-order test named for UTF-16
      covers BMP names only while `tree_dump` uses Rust scalar-value sorting; add an astral-vs-BMP
      case against the pinned reference serializer.

### External conformance corpus (M1-A / M5)

- WPT `html/syntax/parsing/resources/*.dat` (pinned commit `ed37f83e`; the html5lib-tests repo is
  archived and points here) is the M1-A landing gate; the tokenizer suite is informational.
- test262 (pinned commit): executed through `boa_engine` 0.21.1 with harness files and YAML
  frontmatter honored; per-milestone curated slices with an explicit xfail manifest.
- css-syntax + WPT-selectors corpora: inherited by adopting `cssparser` / `selectors`.
- Rules: corpora live in `testdata/`, fetched once by `tools/fetch-corpus.ps1` (human-run, pinned via
  commit hashes); tests never touch the network; xfail entries name the exact test file + reason;
  pass-rate progression is recorded in the updates log at each milestone.

## Non-goals (locked unless a milestone re-opens them)

Text selection/copy, iframes/`<frame>`, bidi/RTL/writing modes, `line-height`/fonts, border-radius,
inline-element borders, cookies, `addEventListener` DOM events (click-only v0), top-level await,
full CSS/DOM, window-title setting, syscall sandboxing, config files pre-M6, drag input pre-M6.

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
