# TextSurfer — history log

Dated decisions, dependency pins and plan changes, newest first. Moved verbatim out of
`ROADMAP.md` on 2026-08-27, where it had grown into a work journal; the roadmap now carries
only current status, decisions still in force, and open plans.

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
