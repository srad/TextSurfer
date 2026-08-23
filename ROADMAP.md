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
| `net/encoding.rs` — WHATWG sniffing: BOM, header, meta prescan, XML fallback, x-user-defined postprocess | custom | **Keep** — html5ever ships only the meta-`charset` substring extractor and it is `pub(crate)`; encoding_rs is decode/encode-only; no sniffer exists in the ecosystem |
| `core/url.rs` — `url_fix` | not a parser | **No change** — delegates all real parsing to the `url` crate; scheme/host/search heuristics are address-bar UX behavior |
| `css/parser.rs` | library adapter | **Keep** — stylesheet/rule/declaration tokenization delegates to cssparser; selector parsing and matching delegate to selectors |
| `css/parser.rs` — terminal media-query grammar/evaluation | custom library adapter | **Keep narrow adapter** — cssparser owns tokens, blocks, delimiters, and recovery; the adapter evaluates media types plus scripting, color scheme and cell viewport dimensions. css-mediaquery 0.1.1 is an immature raw-string port without MQ5 grammar/recovery, LightningCSS has no runtime-context evaluator, rdom-tui explicitly excludes `@media`, and Stylo/Blitz/MusKitty/litehtml/Ladybird require replacement DOM/style/rendering stacks |
| Table layout (M1-D) | custom, implemented | **Custom is correct** — Taffy 0.13 implements block/flex/grid and exposes `item_is_table`, but has no table algorithm. `super-table` 0.3.0 accepts string matrices rather than a foreign styled box tree; `iris-layout` 0.4.0 has no integrated CSS table formatter. Neither supplies CSS anonymous-table fixup, spans, captions, border conflict resolution, or nested box layout |
| `tests/support/dat.rs` | test-fixture parser | **Custom is correct** — no crate parses the WPT `.dat` fixture format; this stays isolated from production code |

### Prior-art audit (2026-08-22)

Conformance is evidence, not reimplementation — and the same discipline applies to product
behavior. Terminal browsers have already settled several questions we were answering ad hoc.

| Source | What it establishes | Adopted here |
|---|---|---|
| [chawan](https://github.com/sourcehut-mirrors/chawan) (`doc/css.md`) | The terminal CSS contract: author colours **contrast-corrected against the terminal background**; `border-*-width` is **binary**; `font-weight > 500` = bold, `font-size` ignored; `text-decoration` underline/line-through; sub-cell inline margins/padding ignored; overflow-x displays, overflow-y clips, no scrollbars; `::before`/`::after` + counters + `list-style-type` for markers; link markers/hints for keyboard navigation | All locked as decisions below; markers and hints scheduled in M1-D/M2 |
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
| M1-D — Layout completeness | Table layout, generated content + list markers, presentational attributes, `text-align` | (in progress — tables done) |
| M2 — Tabs & keyboard | Link navigation, anchors, titles, error pages, start page, in-page search, forms, robustness | (open) |
| M3 — Mouse | Zones, wheel, clicks, hover, dynamic pseudo-class state, theme states | (open) |
| M4 — JS seam | `JsEngine` trait + Noop impl + host layer, `js` feature off, pure Rust | (open) |
| M5 — Boa | Boa 0.21.1 behind trait; decision gate Boa vs Deno Core; host bindings subset; job pump | (open) |
| M6 — Stretch | Flex/grid + conformant floats, images, persistence, scroll memory, console view, config, perf gate | (open) |

Test counts at the last green run (2026-08-23): **319 lib · 5 binary · 4 pipeline · 14 corpus ·
14 golden**.
Cross-cutting: test infrastructure (done: contract suites, snapshots, proptest, fakes) · gates (done:
local only, no CI) · coverage floor (open: optional local, 80% overall / 90% css·layout·paint) ·
external conformance corpus (M1-A done at 95.16% raw / 100% with xfail; test262 at M5).

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
4. Manual smoke list (example.com, lite.duckduckgo.com, wikipedia.org) — run by the human per
   milestone close; never part of automated tests.
5. Live repo rule: no commits without explicit user confirmation.

## Architecture (as-built)

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
 layout::LayoutEngine → BoxTree (absolute coords, unbounded height, styled text fragments)
 paint::Painter → DisplayList (styled spans · NodeId hit-tags · link rects) → ui content widget
```

- Single crate, modules: `core`, `net`, `html`, `css`, `layout`, `paint`, `script`, `ui`, `app` + thin `main`.
- Cross-module boundaries via traits; only `app`/`main` know the concrete implementations (composition root).
- `app` never imports crossterm: events come in as `core` types, results via `deliver_fetch`, time injected.
- DOM never crosses threads. One thread owns everything except I/O (fetch worker threads only).
- `Document` owns DOM pre-insertion validation and hides indextree; template contents are detached
  document fragments, and DOM removal detaches rather than invalidating node handles.
- The `DisplayList → ui content widget` edge lands in M1-B. Before it, the painter's hit and link
  lists were computed and discarded, and the content widget drew one flat colour.
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
- Native text renderer (lynx/w3m/chawan family), not embedded-engine (carbonyl/browsh family) — our
  value is small footprint + terminal-native layout.
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
- **UA stylesheet is hardcoded Rust** in `css/cascade.rs::ua_style`, not CSS text. The earlier
  "ships as CSS text in `css/ua.rs`" entry described a file that never existed; corrected 2026-08-22.
  Revisit only if the UA sheet grows past what a typed function expresses clearly.
- **Colour model**: `ComputedStyle` colours are `Option<Rgb>`; `None` means "the theme decides", so
  the Norton palette stays the default field and author colours override only where declared.
  Author foregrounds are **contrast-corrected** against the effective background (chawan's rule) —
  faithfulness never outranks legibility.
- **Borders are binary**: any non-zero border width paints one box-drawing frame. Replaces the
  earlier "borders ≥2 cells doubled" phrasing, which nothing implemented and which contradicts the
  prior-art model.
- `font-weight > 500` = bold; `font-size` is ignored; `text-decoration` maps to
  underline/line-through; reverse video is available as a style bit.
- Sub-cell margins and padding are ignored on inline boxes; all CSS lengths are cell-rounded and
  capped at 65,535.
- Overflow: the x axis displays (clipped at the viewport edge), the y axis extends the document, and
  there are no scrollbars.
- `:visited` parses and **never matches** — page styling must not observe history.
- Colour **values** are parsed by `cssparser-color` 0.5.0 (same cssparser 0.37 pin), so keywords,
  hex, `rgb()/rgba()`, `hsl()` and `hwb()` all work; CIE spaces (`lab`, `lch`, `oklab`, `oklch`)
  parse but do not convert to sRGB, so such declarations are ignored rather than guessed.
- Out of scope for now, revisit when a page needs them: relative colours, `background-image`,
  `line-height`, fonts, border-radius, inline-element borders.

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

Remaining:

- [x] **Styled paint seam.** `ComputedStyle` gained `color`/`background`/bold/underline/strike/reverse
      (`Option<Rgb>`, `None` = theme); `InlinePiece`, `TextFragment` and `LayoutBox` carry the
      resolved `CellStyle` from the inline ancestor chain; `DisplayList` is styled spans + hit tags +
      link rects; paint order is backgrounds bottom-up → borders → clipped text; `legible_foreground`
      contrast-corrects author foregrounds against the effective background. *Proven by*
      `author_colors_weight_and_decoration_reach_the_computed_style`,
      `backgrounds_paint_under_text_in_depth_order`,
      `unreadable_author_colours_are_corrected_towards_the_theme_text`. (done)
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
- [x] **`<img>` and `<hr>`.** `<img>` renders `[alt]` (or `[img]`); `<hr>` fills the content width.
      *Proven by* `images_render_their_alt_text_and_rules_span_the_content_width`. (done)
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
- **Acceptance:** every box above is checked with its proof green and all five gates pass
  (281 lib · 4 binary · 3 pipeline · 14 corpus · 10 golden tests). Remaining for milestone close:
  the human eyeball smoke in a real terminal — `--dump` already renders example.com, Wikipedia and
  DuckDuckGo Lite over the real network without panicking.

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
- **Acceptance:** out-of-order, stale-generation, redirect/base, import-cycle, budget and
  cascade-order fixtures green; bucketed and naive cascades agree; manual Wikipedia smoke renders
  with external author styles.

### M1-D — Layout completeness (in progress)

Sequenced after M1-C and before M2: a terminal browser is judged on whether real pages are readable,
and tables are what separate w3m from lynx. Flex/grid stay in M6.

- [x] **Table layout** *(done)* — `display: table*` stops degrading to block. Normalize the
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
      *Proof:* unit/contract tests for cascade, fixup, spans, width and border conflicts; property laws
      for occupancy, glyph disjointness, hit boxes and width monotonicity; fixture goldens for simple,
      spanned/collapsed, nested/captioned and fixed-overflow tables.
- [ ] **Generated content and markers** — `::before`/`::after` with `content`, counters
      (`counter-reset`/`counter-increment`) and `list-style-type`, replacing the hardcoded `"• "` /
      `"# "` prefixes. Fixes ordered lists, which currently render every `<ol>` item as a bullet.
      *Proof:* `<ol>` numbers, nested lists number independently, `list-style-type: none` suppresses.
- [ ] **Presentational HTML** — map `align`, `bgcolor`, `width`, `cellspacing`, `cellpadding`,
      `border`, `rules`, `frame`, `valign`/`vertical-align`, `<center>` and `<font color>` into the
      cascade at UA-origin specificity, plus `text-align` (left/right/center/justify→left).
      *Proof:* an old-school fixture page lays out as intended.
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
      path with a degraded box tree plus a status-bar message. *Proof:* a 100k-deep synthetic
      document renders a truncation notice instead of aborting.
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
      methods/encodings render a controlled error. `:checked`/`:enabled`/`:disabled` become live.
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
- [ ] Hit-test resolution contract: targets resolved by NodeId against the live document at dispatch.
- [ ] Theme extension: hover/selected/search-match states with their own accents. Snapshot goldens.
- **Acceptance:** scripted zone tests + goldens with hover states; manual mouse walkthrough.

### M4 — JS seam (open)

- [ ] `JsEngine` + `js` feature wiring in the composition root; runtime `--js=off` wins over feature.
- [ ] `MutateOp` funnel + invalidation-once rule; host subset: document, location, console→status
      buffer, alert→dialog line; no dispatch except `onclick` handlers.
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
      width, and again when emitting fragments.
- [ ] Persistence/backup · per-history-entry scroll memory · drag input · console view (F12) ·
      config file · `data:` URL scheme · optional Readability-style reader view.
- Non-goals beyond this list stay non-goals.

## Test infrastructure (standing)

- Contract suites, capability-parameterized, defined in M0, run against every impl (M1-A fakes …
  M5 Boa) — an interface is defined by its tests.
- insta snapshots of ratatui `TestBackend` buffers (widget goldens), corpus goldens (M1-B),
  DOM tree dumps (M1-A). `buffer_string` compares symbols only; style assertions use the
  style-aware helper added in M1-B.
- proptest layout laws (M1-B): width monotonicity and painted-row bounds are green; laminar boxes,
  engine-backed deepest-hit round trip, disjoint leaf glyph cells and scroll-clamp fixed point
  remain. Property tests for `url_fix`/`EditBuffer` continue from M0.
- FakeFetch + fake clock + fake Host; no test touches the network or the real clock.
- `tests/common/` shared fakes; inline `#[cfg(test)]` fakes where module-local.
- `--dump` (M1-B) is the scriptable end-to-end harness: fixture in, golden text out.

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
