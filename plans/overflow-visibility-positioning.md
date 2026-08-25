# Element overflow clipping, `visibility`, and CSS positioning (implementation plan)

*Written 2026-08-25 against commit `01354cb`. `ROADMAP.md` stays the source of truth for status and
decisions; this file carries the reviewed plan and the cross-session progress state.*

---

## Progress / handoff

**Delivered:** all local gates green (665 lib · 13 binary · 6 fetch-pipeline · 14 corpus ·
35 golden; 572 lib without default features). The live 100-column Wikipedia dump retains the
article and navbox while all five recorded corruption signatures are absent. Human VGA/terminal
interaction smoke remains pending.

### Initial partial state

- **Slice 1a, types only.** `cargo build` clean; no behaviour change yet.
  - `Overflow` + `OverflowAxes` in `src/core/style/box_model.rs` — includes `parse`, `clips()`, and
    `OverflowAxes::computed()` implementing the spec fixup (a `visible` axis paired with a
    non-`visible` one computes to `auto`).
  - `Visibility` in `src/core/style/display.rs` — `parse`, `is_hidden()` (`collapse` ⇒ hidden).
  - Both re-exported from `src/core/style/mod.rs`.
  - `ComputedStyle` gained `overflow: OverflowAxes` and `visibility: Visibility`.
  - `visibility` wired into inheritance site 4 of 5 (`ComputedStyle::anonymous_inheriting`).
  - Standards review corrected the implementation contract: `clip` is not scrollable for the
    cross-axis fixup, `auto` maps to clipped Taffy behavior, and clipping must be axis-aware with a
    horizontally bounded but vertically unbounded viewport.

### Remaining handoff

Implementation and automated verification are complete. Do not repeat the slices below; they are
retained as the design record. The only remaining acceptance work is the manual VGA and terminal
smoke list.

### Implemented order

1. **Finish 1a cascade wiring.** Four inheritance sites still missing `visibility` — miss one and a
   hidden subtree silently reappears:
   | Site | What it is | Status |
   |---|---|---|
   | `src/css/ua.rs:28` | non-element nodes | **done** |
   | `src/css/ua.rs:46` | non-HTML-namespace elements | **done** |
   | `src/css/ua.rs:118` | main HTML element block | **done** |
   | `src/core/style/mod.rs` `anonymous_inheriting` | anonymous boxes | done |
   | `src/css/cascade/document.rs:254` | `::before`/`::after`/marker | **done** |

   Then in `src/css/cascade/declaration.rs`: `overflow` / `overflow-x` / `overflow-y` /
   `visibility` arms in `apply_declaration`, matching arms in `apply_css_wide` (~line 359), and
   `"visibility"` added to `is_inherited()` (~line 447). Call `.computed()` once in
   `cascade_document` where `style.display = computed_display(…)` already runs.
2. **Slice 1b** — Taffy `overflow` + `scrollbar_width: 0.0` in `taffy_style.rs`.
3. **Slice 1c/1d** — the clipping itself. This is the change that actually fixes the page.
4. Slices 1e–1g, then 2, then 3.

### Standing notes for whoever picks this up

- The harness may ask for file edits via `sed`/heredocs. The user's CLAUDE.md forbids editing source
  with scripts; use anchored `Edit` calls.
- No `git add`/`commit`/branch without the user's explicit go-ahead in the moment.

---

## Context

`cargo run -- --url "https://en.wikipedia.org/wiki/Terminal_emulator" --dump --cols 100` renders the
**article body well** — paragraphs, headings, lists, `[edit]` links, nested tables, the footer and
the navbox all come out readable. Every serious defect is in the top ~30 rows plus one navbox row,
and all of them trace to the same small set of unimplemented CSS.

| Symptom in the dump | Cause |
|---|---|
| `TeContributelator`, `Special Talks`, `herençais`, `Pageerlands`, `Wikimediaeo` — sidebar / language / tools menus overprinting the article | `.vector-dropdown-content{position:absolute;top:100%;left:-1px;opacity:0;height:0;visibility:hidden;overflow:hidden auto;z-index:50;…}`. `height:0` *is* honoured; its content then overflows unclipped and paints over the article |
| `J u m p t o c o n t e n t` printed one glyph per row down the left margin | `.mw-jump-link:not(:focus){position:absolute!important;clip:rect(1px,1px,1px,1px);width:1px;height:1px;overflow:hidden}` — the 1px box wraps its text to one column and nothing clips it |
| Navbox row bleeding past the right `│` border | cell content wider than the cell; the clip in `layout/table/content.rs` covers only the fixed-layout path |
| Empty `┌──┐ │ │ └──` stubs beside `Main menu` / `Search` | icon buttons sized by `min-width`/`min-height` with a `mask-image` glyph. Harmless once the dropdowns stop overlapping — not addressed here |

Property census over that stylesheet: **90 `position:`** (23 relative, 20 absolute, 3 fixed,
2 sticky) with 76 `left` / 70 `right` / 53 `bottom` / 47 `top`; **48 `float`**; 28 `clear`;
23 `opacity`; 17 `overflow`; 14 `z-index`; 2 `visibility`.

None of `position`, the insets, `overflow`, `visibility`, `opacity`, `float` or `clear` was parsed
before this work — `src/css/cascade/declaration.rs` had no arm for any of them, and
`src/css/cascade/tests/mod.rs:680` explicitly pins that `position: absolute; inset: 4px; top: 1px`
is *dropped*. Nothing in layout or paint clips a box to its ancestors: `grep -rn "clip" src` finds
only the table-cell helper and the ratatui widget clip.

**ROADMAP.md gap:** M6 lists Grid, Floats, Images and the perf gate, but positioning, overflow
clipping and `visibility` appear **nowhere in the roadmap**. The one "Overflow" decision
(ROADMAP.md:268) is about the viewport axes, not element boxes. AGENTS.md requires the roadmap to be
amended in the same change.

Outcome: the article starts at row 1 with no overprinted chrome — which is also the fix for every
site built on the same absolutely-positioned-menu idiom.

### Reference implementation

Checked against Ladybird (`LadybirdBrowser/ladybird@master`, read over raw.githubusercontent):

- `Libraries/LibWeb/Rust/src/painting/scrollable_overflow.rs` — overflow is measured and clipped
  **relative to the padding box** (`absolute_padding_box_rect`, `overflow_relative_to_padding_box`).
- The same file traverses `contained_boxes_by_containing_block` — **containment follows the
  containing-block chain, not the box-tree parent chain** — and skips `position: fixed` children
  when accumulating an ancestor's overflow.
- `Libraries/LibWeb/Layout/Node.cpp:404` `style_establishes_absolute_positioning_containing_block()`
  — "values other than `static` make the box a positioned box"; the Viewport is the fallback.

That is the justification for the hoist in slice 2: once absolutely positioned boxes are re-parented
onto their containing block, the box-tree parent chain **is** the containing-block chain, so one
ancestor-clip walk becomes correct rather than over-eager.

### Deliberately dropped

**`opacity`.** A terminal cell has no alpha, so only `opacity: 0` would be actionable; and every
real "hide me" rule — including Wikipedia's — pairs it with `visibility`, `display` or `height:0`.
Implementing `overflow` and `visibility` covers the page; `opacity` adds a second, non-inherited
subtree-suppression mechanism for no additional pixels. Record it as a non-goal, not an oversight.

---

## Slice 1 — Element overflow clipping + `visibility`

On its own this removes the vertical `Jump to content` column and every overprinted menu: each of
the properties Wikipedia hides them with converges on the same result.

### 1a. Cascade

`src/css/cascade/declaration.rs`:

- `overflow`, `overflow-x`, `overflow-y`. Accept the one- **and two-value** forms —
  `overflow: hidden auto` is what Wikipedia writes. Keywords `visible | hidden | clip | scroll |
  auto`. Per the locked terminal contract there are no scrollbars, so `clip`/`scroll`/`auto` all
  behave as `hidden`; they stay distinct computed values so serialization and future scrolling are
  not foreclosed. Not inherited.
- **Computed-value fixup:** when one axis is scrollable (`hidden | scroll | auto`), a `visible` on
  the other computes to `auto` (`OverflowAxes::computed`). `clip` remains distinct. This is spec,
  and load-bearing here —
  `pre,.mw-code{overflow-x:hidden}` leaves `overflow-y` unset, and without the fixup the two axes
  would disagree about whether the box is a clipping container.
- `visibility: visible | hidden | collapse`. Inherited; `collapse` behaves as `hidden`, including
  inside tables — row/column collapsing is not implemented.
- Arms in `apply_css_wide`; `"visibility"` added to `is_inherited()`.

**`visibility` is inherited by explicit field copy in five places** — the table in the handoff
section above tracks which are done.

### 1b. Taffy mapping

`src/layout/engine/taffy_style.rs`: set `TaffyStyle::overflow` from the computed value and
`scrollbar_width: 0.0`. Map `visible` to Taffy `Visible`, `clip` to `Clip`, `hidden|auto` to
`Hidden`, and `scroll` to `Scroll`. The distinction preserves Taffy's automatic-minimum semantics
while keeping this frontend scrollbar-free.

### 1c. Clipping — the shared helper

`src/layout/table/geometry.rs:506` already has `intersect_rect`, and `content.rs:620–692` already
has per-edge border suppression on a clipped stroke plus grapheme-safe `clip_text`. Lift these into
a new `src/layout/clip.rs`.

Three corrections to make while lifting — this is an extension, not a straight move:

- `intersect_rect` is `pub(super)` inside `layout::table`. Moving it changes what `pub(super)`
  reaches — AGENTS.md module rule 3. Re-check every user and pick `pub(in crate::layout)`
  deliberately.
- **The existing fragment clip handles only the right and bottom edges** (`content.rs:660`:
  `fragment_col >= clip right || fragment_row >= clip bottom`, then `clip_text` to the right).
  General overflow needs all four — `left:-1px` on the dropdown puts content left of the clip. The
  shared helper must take leading graphemes off the left and advance `col` by their width.
- Clip at **grapheme-cell granularity**: a grapheme whose cells are not wholly inside the clip is
  dropped. A wide CJK glyph or a VGA `scale > 1` fragment (`TextFragment::rect()` multiplies width
  and height by `style.scale`) cannot be half-drawn, so it goes or stays whole — vertically too.

### 1d. Clipping — the four emission sites

The clip rect is the **padding box**, per Ladybird. In the main walk that is `border_rect` deflated
by `layout.border.*` only; the existing code already splits `layout.border` from `layout.padding`
when it computes `content_rect` (`engine/mod.rs:350–360`), so it is in hand there.

There is not one emission path but four, all pushing into `tree.{boxes,fills,strokes,fragments}`:

1. **The main walk** (`engine/mod.rs:~320–440`). Its stack is already
   `vec![(index, parent_col, parent_row)]` with absolute coordinates. Widen the tuple to carry
   `clip: LayoutRect` — the intersection of every ancestor's clipping padding box. A child gets
   `clip ∩ own_padding_box` when this box's `overflow` clips, else `clip` unchanged. Clip each
   `BackgroundFill`, `BorderStroke`, `LayoutBox` and `TextFragment`; drop what becomes empty.

   **A box's own painting uses the inherited clip, not its own.** `overflow` clips a box's
   descendants, never the box itself — so its `BackgroundFill`, `BorderStroke`, `LayoutBox`, its
   `rule` fragment and its **outside list marker** (emitted at `content_rect.col - width`, which
   sits outside its own padding box by construction) all take the clip it inherited. Only its
   children and its own inline content take the intersected clip. Getting this backwards makes
   every `overflow:hidden` list item lose its bullet.
2. **`append_inline`** (`flow.rs:747`). Called from the main walk for `flow[index].inline`, so it
   must be handed the **child** clip — this box's own `overflow` clips its own inline content.
3. **`append_table_output`** (`engine/tables.rs`) and 4. **`append_atomic_layout`**
   (`flow.rs:825`). Both merge a nested tree by offsetting every rect; intersect with the clip
   immediately after `offset_rect`, exactly as `layout/table/content.rs:620` already does for
   tables. Clipping *inside* a nested `AtomicLayout` falls out for free — it is produced by
   `layout_flow`, which runs this same walk.

Also:

- **Never clip at the root box or `<body>`.** CSS propagates the root and body `overflow` to the
  viewport, and our viewport contract is already settled (ROADMAP.md:268 — the y axis extends the
  document, the x axis clips at the viewport edge). A naive root clip would truncate every page that
  writes `body{overflow:hidden}` to its first screenful. This page declares neither, so the rule
  costs nothing here and prevents a catastrophic failure elsewhere.
- **`tree.height` must not grow from clipped content.** All three `tree.height = …max(…)` sites
  (main walk, `append_table_output`, `append_atomic_layout`) take the clipped rect. Otherwise the
  `height:0` dropdown still stretches the document by its content height — and `painter/mod.rs:248`
  allocates `vec![PaintedRow::default(); box_tree.height]`, so this is a real memory win too.
- **Empty clip ⇒ skip the whole subtree.** An early-out, and a large one on a page with four
  collapsed menus.

**Clipping must be exactly right, because `overflow:hidden` lands on ordinary content, not just
chrome.** This stylesheet puts it on every section heading
(`.firstHeading,.mw-heading2,:not(.mw-heading2) h2{overflow:hidden;border-bottom:…}`), on `pre`,
and on hatnotes. Those boxes are `height:auto` so a correct implementation changes nothing about
them — but an off-by-one in the padding-box derivation eats article text. The acceptance run below
checks headings explicitly for this reason.

One safety property worth knowing: `calc()` is not supported, so declarations like
`max-height:calc(100vh - 48px)` are dropped and the box grows to its content instead of being
capped. Unsupported lengths therefore make clipping *less* aggressive, never more — it fails open.

### 1e. `visibility`

- **Box level:** a `visibility: hidden` box emits no fill, stroke, marker, rule or fragment, **and
  no `LayoutBox`** — `painter/mod.rs:255` turns every `LayoutBox` into a `HitRegion`, so keeping it
  would leave an invisible menu hoverable and clickable. Keep its geometry and keep walking its
  children: a `visibility: visible` descendant still paints, and still gets its own `LayoutBox`.
- **Inline level:** inline elements have no `FlowBox` — they are flattened into `InlinePiece`s. Add
  `hidden: bool` to `Piece` and `Glyph` (`src/layout/text_flow.rs:19,29`), set from
  `InlineContext.computed.visibility`, and in `append_inline`'s glyph loop skip the
  `tree.fragments.push` while **still advancing `current_col`** — CSS keeps the width of a hidden
  inline. Cost: roughly a dozen `InlinePiece` literals in `flow.rs` gain a field. Do **not** put
  this on `CellStyle`: nothing hidden is ever painted, so the paint style is the wrong home, and
  widening it would churn the style-aware snapshot helper.

### 1f. What comes for free (do not build twice)

- **Link rects.** `assign_link_rects` (`engine/links.rs:8`) derives `LinkBox::rects` from
  `tree.fragments`. Clip the fragments and the link rects are clipped with them; a fully hidden link
  ends up with no rects and no `hit_nodes`. Verify nothing downstream assumes a link has ≥1 rect —
  `painter/mod.rs:281` already tolerates it via `.max().unwrap_or(0)`. This retires half of the M2
  risk-register line "link/hit rectangles are not clipped at paint time".
- **Hit regions.** Built from `boxes` and `fragments` in the painter; both already clipped.

### 1g. Tests

- Cascade: one- and two-value `overflow`; the axis fixup; invalid-value atomicity (follow the
  existing `invalid_display_grammars_do_not_replace_an_earlier_declaration` pattern);
  `inherit`/`unset` through `apply_css_wide`; inherited `visibility` with a `visible` descendant
  re-appearing; `visibility` surviving each of the five inheritance sites (including into a
  pseudo-element and an anonymous box).
- Layout: `height:0; overflow:hidden` emits **no** fragments and does not raise `BoxTree::height`;
  `width:1px; overflow:hidden` (the skip-link shape); partial clip on each of the four edges,
  including a left clip that advances `col`; a wide-glyph and a `scale:2` fragment straddling a clip
  edge; a clipped stroke losing exactly the crossed edges; a clipped link's `LinkBox::rects`; a
  `visibility:hidden` box emitting no `LayoutBox` while a `visibility:visible` child does; a hidden
  inline keeping its width.
- Proptest law, beside the existing disjoint-glyph and laminar-row laws in
  `src/layout/engine/tests/`: **no emitted fragment, fill, stroke, `LayoutBox` or link rect ever
  falls outside the clip of its ancestors.**
- A new `tests/fixtures/` page shaped like the Vector dropdown plus the skip link, with a golden
  (`tests/render_goldens.rs`, `WIDTH = 40`).

---

## Slice 2 — CSS positioning

Takes the remaining out-of-flow chrome out of normal flow, so the article starts at the top.

**Slice 1 is deliberately shipped first even though it is briefly over-eager.** Until the hoist
lands, a clip walk down the box-tree parent chain will clip absolutely positioned descendants that
CSS lets escape a non-containing-block ancestor. That is invisible today — nothing is positioned
yet, so no box can escape — and becomes correct the moment 2d makes the parent chain equal the
containing-block chain. Do not "fix" it by weakening slice 1's clip; re-verify the clip tests after
2d instead.

### 2a. A signed inset type — required, not optional

`CssMargin::Cells(usize)`, `EdgeSizes` and `CssPercentage(u32)` are all **unsigned**
(`core/style/box_model.rs`). Insets are routinely negative — `.vector-dropdown-content{left:-1px}`,
`.mw-jump-link{margin:-1px}` — so none of the existing types can carry one. Add
`CssInset { Auto, Cells(isize), Percent(CssPercentage) }`. Reject negative percentages rather than
guessing (they are vanishingly rare and `CssPercentage` is unsigned).

### 2b. Cascade

`position: static | relative | absolute | fixed | sticky`, plus `top`/`right`/`bottom`/`left` and
the `inset` shorthand, reusing `parse_lengths`/`assign_one` with the new signed type. Arms in
`apply_css_wide` too; `position` and the insets are **not** inherited.

**Absolute and fixed blockify.** CSS turns `display:inline` into `block` on an absolutely
positioned box. `Display::blockify()` already exists (`core/style/display.rs`) and
`computed_display` (`css/cascade/document.rs:305`) is the existing hook — the flex-item path already
calls it. Without this, an absolutely positioned `<span>` stays an `InlinePiece` and has no box to
position.

### 2c. Taffy mapping

`taffy_style.rs`, using Taffy 0.14 natively (`Position::{Relative,Absolute}`,
`Style::inset: Rect<LengthPercentageAuto>`):

- `static` → `Position::Relative` with `inset: auto` (Taffy's default; CSS `static` ignores insets).
- `relative` → `Position::Relative` + insets. Taffy applies these as an end-of-layout correction,
  which is exactly CSS relative positioning.
- `absolute` / `fixed` → `Position::Absolute` + insets.
- `sticky` → degrade to `relative`.
- `fixed` → degrade to absolute-against-the-root. A text browser scrolls the document; a
  viewport-fixed bar would smear down the page.

Verified in the pinned crate: block layout filters absolute children out of flow
(`compute/block.rs:914`) and lays them out separately (`:1637`), flexbox does the same (`:664`,
`:2652`), and both resolve them against the parent's **padding box**
(`absolute_position_area = final_outer_size - resolved_border`, `block.rs:745`).

### 2d. The containing-block hoist (`src/layout/engine/flow.rs`)

Taffy resolves an absolute child against its **parent node**; CSS resolves it against the nearest
positioned ancestor. Close the gap with a post-pass over the finished flow tree — `FlowTree` is a
flat `Vec<FlowBox>` with `children: Vec<usize>`, so this re-parents rather than rebuilds.

- Walk from the root **top-down**, carrying the flow index of the nearest positioned ancestor (the
  root box is the initial containing block, standing in for Ladybird's Viewport). A box is
  positioned when its computed `position` is anything but `static`.
- `absolute` → move its index from its parent's `children` to that ancestor's. `fixed` → move to the
  root box.
- Top-down order means a hoisted box is itself already positioned when its own absolutely
  positioned descendants are visited, so nested absolutes resolve against it correctly.

**The trap: the hoist target may be a Taffy leaf.** `try_layout_flow` (`engine/mod.rs:~210`) builds
`new_leaf_with_context` whenever `!flow[index].inline.is_empty()`, and a Taffy leaf cannot have
children — so appending to a positioned ancestor that holds inline content would silently drop the
box. Before appending, if the target's `inline` is non-empty, move those pieces into a fresh
anonymous `FlowBox` (`ComputedStyle::anonymous_inheriting(target.style, Display::BLOCK)`) inserted
as the target's first child. This is precisely what `flush_inline` already does when a block child
interrupts inline content; reuse it rather than writing a second version.

Other invariants:

- Shift `depth` uniformly across the moved subtree to sit under the new parent. `depth` drives the
  paint sort and the slice-1 clip stack, and keeping it consistent with the new parent chain is what
  makes the two slices compose.
- **The hoist preserves the flow-tree index invariant that `try_layout_flow` depends on.** That loop
  builds Taffy nodes in reverse index order (`for index in (0..flow.len()).rev()`) and reads
  `taffy_nodes[*child]`, which is only sound because `build_flow_tree_from` always pushes a parent
  before its children, so every child's index exceeds its parent's. Hoisting moves a box only
  *toward* the root — to a strictly lower index than its former parent — so the invariant holds. A
  hoist that ever moved a box to a higher index would produce `None` children and silently degrade
  the whole layout to `engine_failed`. Assert the invariant in the hoist tests.
- **`MAX_BLOCK_DEPTH` survives**: hoisting only ever moves a box closer to the root, so the flow
  tree's maximum depth cannot increase and the stack-overflow guarantee (`flow.rs:27`) is preserved.
- Run the hoist **before `prepare_flow`** (`engine/mod.rs:445`), whose flex `order` sort must not
  see absolute children — they are not flex items.
- Sub-layouts built by `build_flow_subtree` (inline-block/atomic content) have their own root; there
  `fixed` hoists to the sub-root. Note the divergence.
- **Stacking stays document order by `depth`** — `z-index` is not implemented, so an absolute box
  paints above earlier siblings and below later ones.
- `tree.height`: absolutely positioned boxes **do** contribute to their containing block's overflow
  (Ladybird, same file), so keep the `.max()`. `fixed` boxes do **not** — Ladybird skips them
  explicitly; skip them too.

### 2e. Negative origins must clip, not translate

`layout_rect` (`taffy_style.rs:248`) computes `right = (col.max(0.0) + width).round()`, so a box at
`col = -12, width = 20` becomes `col: 0, width: 20` — it **translates** content that should be
clipped at the origin. Today nothing reaches it with a negative column (margins are `usize`); insets
make it reachable, and the common `position:absolute; left:-9999px` offscreen idiom would then dump
hidden content at column 0, on top of the page. That is worse than the status quo.

- Fix `layout_rect` to clip at the origin: derive `right` from the **unclamped** `col + width`, then
  `width = right.saturating_sub(col.max(0))`.
- Taffy's flex alignment can already produce negative locations when an item overflows its container,
  so this may move existing flex goldens. **Pin the current behaviour with a test first**
  (`engine/tests/flex/sizing.rs` already has
  `a_fixed_item_overflows_a_zero_sized_flex_container_without_corrupting_geometry` next door), then
  change it and justify each moved golden in the commit body.
- Wikipedia's own worst case is `left:-12px` (≈ −1.5 cells), so this page does not depend on the
  fix — it is here so positioning does not ship a new way to corrupt a page.

### 2f. Tests

- Cascade: every `position` keyword; `inset` shorthand and the four longhands with `auto`, negative
  lengths and percentages; blockification of an absolutely positioned inline; invalid-value
  atomicity. Note `src/css/cascade/tests/mod.rs:680` currently pins the *opposite* behaviour and
  must be rewritten, not deleted.
- Layout: relative offset on a block and on an inline-block; an absolute child inside a relative
  parent removed from flow (siblings close the gap) and placed at `top:100%`; **an absolute box
  nested two `static` levels below its positioned ancestor lands on that ancestor's padding box**
  (the case the hoist exists for); an absolute box with no positioned ancestor resolving against the
  root; nested absolutes where the outer is the inner's containing block; **an absolute box hoisted
  into a positioned ancestor that holds inline content** (the leaf trap — assert the box is emitted
  at all); `fixed` and `sticky` on their degraded paths; a box at a negative origin clipped rather
  than shifted.
- Hoist invariants: box count and every `owner` preserved; no box becomes its own ancestor; child
  index always exceeds parent index; `depth` monotone down each chain; maximum depth never
  increases; the DOM untouched (the same obligation the flex `order` work already carries).
- A golden for the slice-1 dropdown fixture, now out of flow.

### 2g. Known limitations to record, not fix

`position: relative` on a non-replaced **inline** element is ignored — inlines have no box to offset.
`z-index` is unimplemented. `clip: rect(…)` is unimplemented (`overflow` covers the one use here).

---

## Slice 3 — auto-layout table cell clipping

`layout/table/content.rs:634` clips only the fixed-layout path, which is why the navbox row bleeds
past its `│`. Route the auto-layout path through the same `src/layout/clip.rs` helper. Drop
`TableFragment::clip_right` if the shared helper subsumes it (it is local to `layout::table` and
only read by an assembly step and a test invariant at `table/tests.rs:84`). One golden.

## Not in this plan

- **Floats / `clear`** (48 + 28 uses). Already an M6 roadmap item. `float` is ignored today, so a
  floated `.thumb` or `.mw-editsection` becomes a block, which reads acceptably in the dump. Much
  more work than the three slices combined — Taffy has no float engine.
- **`opacity`** — see "Deliberately dropped" above.
- **Images**, **Grid**, **perf gate** — unchanged M6 items.

## ROADMAP.md changes (required, same commit)

- Add a milestone entry for **CSS positioning + element overflow/visibility** to the status board and
  task list; it is currently absent from the roadmap entirely.
- Amend the locked decision at ROADMAP.md:268 so "Overflow" covers element boxes, not only the
  viewport axes.
- Decisions log: `overflow: clip|scroll|auto` ≡ `hidden` (no scrollbars); clipping is to the padding
  box; the axis fixup; `visibility` honoured at box and inline level and removing hit regions;
  `opacity` a non-goal with its reason; `fixed` ≡ absolute-against-root and excluded from ancestor
  overflow; `sticky` ≡ `relative`; `z-index` unimplemented so stacking is document order by `depth`;
  the absolute containing block resolved by hoisting in the flow tree because Taffy's native rule is
  the parent node; `layout_rect` clipping at the origin instead of translating.
- Append a dated entry to the updates log; narrow the M2 risk-register line about unclipped link/hit
  rectangles.

## Verification

1. Gates, before and after each slice (AGENTS.md):
   ```powershell
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo clippy --all-targets --all-features -- -D warnings
   cargo clippy --no-default-features --all-targets -- -D warnings
   cargo test; cargo test --features js; cargo test --features vga; cargo test --no-default-features
   ```
   New snapshots via `$env:INSTA_UPDATE = "always"; cargo test`, then a clean `cargo test`; never
   accept a `.snap.new` unread. Conserve the `#[test]` count per file when splitting.
2. Two changes can move **existing** goldens: the `layout_rect` origin fix (2e) and the Taffy
   `overflow` mapping (1b). Diff every changed `.snap` by hand and justify it in the commit body —
   an unexplained golden move is the failure mode this gate exists to catch.
3. End-to-end on the reference page:
   ```
   cargo run -- --url "https://en.wikipedia.org/wiki/Terminal_emulator" --dump --cols 100
   ```
   Acceptance: no `Jump to content` glyph column; no overprinted sidebar/language/tools text
   (`TeContributelator`, `Special Talks`, `Pageerlands`, `Wikimediaeo` all gone); `From Wikipedia,
   the free encyclopedia` within the first few rows; the navbox `Terminal emulators (list)` rows
   inside their right border. **And nothing lost:** every section heading (`Background`,
   `Character-oriented terminals`, `Emulators`, `Implementation details`, `CLI tools`, `Local echo`)
   still present and readable — they all carry `overflow:hidden` — along with the `pre`-formatted
   `/dev/*` passages and the footer. Diff the whole dump against the current output, not just the
   first screenful.
4. Manual smoke (human-run, per AGENTS.md): `https://example.com`,
   `https://lite.duckduckgo.com/lite/`, `https://wikipedia.org`, plus the reference article in both
   the terminal and VGA frontends.
