# M1-C — External stylesheets (implementation plan)

*Written and implemented 2026-08-22, after M1-B closed at commit `f87ca0e`. `ROADMAP.md` stays the
source of truth for status and decisions; this file preserves the reviewed milestone plan.*

## Context

At the start of this work, **M1-C — External stylesheets** was the next open milestone and the item
with the most visible payoff: `--dump` of Wikipedia printed the whole navigation sidebar because
the `display: none` that hides it lived in an external sheet. Embedded `<style>` and inline `style`
were all the cascade had seen.

Four facts from reading the code decide the shape of this work:

| Finding | Evidence |
|---|---|
| **A subresource fetch cannot even be scheduled today.** Jobs are keyed `(tab_id, generation)` throughout the pool — `live`, `canceled`, `finish` — and `submit` returns early when that key is already live. A second request for the same load is silently dropped, and if one did come back, `find_load_mut` would hand it to `deliver_fetch` as if it were the document. | `src/net/pool.rs:60-90,124-160`, `src/app/tabs.rs:68`, `src/app/controller.rs` deliver_fetch |
| **`@import` URLs are thrown away at parse time.** The at-rule prelude is the unit variant `SheetAtRulePrelude::Import`; the parser records an `IgnoredImport` diagnostic and never keeps the URL or its media list. | `src/css/parser.rs:282,362,379` |
| **`base_href` is parsed but consumed by nobody.** `ParseOutcome.base_href` has existed since M1-A; no module reads it. This milestone is the first real URL resolution in the app. | `src/html/parser.rs:18`, `src/html/sink.rs:108` |
| **The cascade matches every element against every active rule.** Fine for one embedded sheet, quadratic once a real site's stylesheet lands. | `src/css/cascade.rs:48-77` |

Outcome: pages styled by their own stylesheets, fetched through the existing per-tab generation
scheduler, cascaded in document order regardless of arrival order, bounded so a hostile page cannot
exhaust memory or the worker pool, and fast enough that a real sheet does not stall the cascade.

## Confirmed decisions (user, 2026-08-22)

- **Render-blocking with a budget** — a navigation paints once its applicable stylesheets have
  settled or a per-load deadline expires, then permits one coalesced late repaint.
- **Selector bucketing lands in this milestone**, since it is exactly this change that makes the
  naive cascade hurt.
- **All four `@media` features** — `scripting`, `prefers-color-scheme`, `width`, `height`. Because
  width/height can change on a terminal resize, resize must *recascade*, not merely re-layout.
- **Five-second blocking window** — the clock starts after HTML parsing and only currently
  applicable sheets block. The deadline paints every ready applicable sheet, followed by at most
  one coalesced late repaint when that transitive graph settles. Nonmatching sheets fetch eagerly.
- **Shared dump dimensions** — `--dump` uses the same load driver as the application. `--cols` and
  `--rows` are exact content dimensions, clamped to at least one; rows default to 24.
- **Aggregate failure is atomic** — exceeding either the 32 MiB retained-raw or 32 MiB
  unique-decoded external CSS ceiling cancels and discards every external sheet for the load while
  embedded CSS remains active. The existing 10 MiB response ceiling fails only that resource.
- **Iterative base discovery now** — use the first HTML `<base href>` in tree order outside template
  contents. Root imports resolve against that effective base, while external imports resolve
  against their response's final URL. URL fragments are removed before caching and cycle checks.
- **Occurrences and fetches are separate** — every non-cyclic occurrence retains its source
  position and counts toward the 64-resource limit, while normalized URLs fetch only once.

## Implementation

### 1. Give fetch jobs a resource identity

- `src/net/fetch.rs`: add `ResourceId(u64)` (0 = the document) to `FetchPayload` and pool jobs;
  keep the public URL-only `FetchRequest` contract unchanged.
- `src/net/pool.rs`: key `live`/`canceled`/`finish` on `(tab_id, generation, resource)`; `cancel`
  keeps its `(tab_id, generation)` signature and cancels **every** resource under that key, which is
  what the per-load pivot already wants.
- `src/app/net.rs`: `Navigate::submit` takes the resource id; `PoolNet` forwards it. The in-crate
  `FakeNet` (`src/app/controller.rs:638`) and `ImmediateNet` (`tests/fetch_pipeline.rs`) follow.
- `src/app/tabs.rs`: `find_load_mut` stays keyed on `(tab_id, generation)`; the controller dispatches
  on the resource id — 0 → document, otherwise → stylesheet slot.
- *Proof:* a stylesheet response for a superseded generation is dropped; two subresources for one
  load both survive (today the second is silently dropped by the dedupe).

### 2. Discover sheets and resolve URLs

- New `src/app/page_load.rs`: walk the document for `<style>` and `<link rel=stylesheet href>` —
  honouring `media`, `disabled` and CSS `type`, and skipping `alternate` — in document order,
  and resolve each against the effective base: `ParseOutcome.base_href` if present, else the
  response's `final_url`, via `Url::join`.
- `src/css/parser.rs`: `SheetAtRulePrelude::Import` becomes `Import { url: String, media: MediaQueryList }`
  and lands as a `CssRule::Import` so the URL survives; keep `InvalidImportForm` for malformed ones,
  and keep the existing rule that `@import` after other rules is invalid.
- Recursive imports are discovered from each fetched sheet the same way, depth-first, preserving
  lexical position.

### 3. Ordered resource slots per tab

- `src/app/tab.rs`: each tab owns an optional `PageLoad`; the driver keeps ordered logical
  occurrences, a normalized-URL fetch cache, per-occurrence media/ancestry/depth state, budget
  counters and the deadline.
- Cascade input is built by walking roots and imports in **document order**, splicing each imported
  sheet before its importing sheet — never in completion order. `app::render::render_document`
  remains the shared parse-independent cascade/layout/paint boundary.
- A resource arriving for a stale generation is dropped by the existing generation check; failures
  mark the slot `Failed` and stay tab-local (status-bar note, never fatal).

### 4. Render-blocking paint with a budget

- On navigation: parse → discover slots → submit them → **hold the paint**, showing the existing
  "loading …" content, until either every currently applicable occurrence is settled or the load
  deadline passes (injected clock, checked in `App::step`, so no test touches the real clock).
- When the gate opens: cascade once over the ordered sheets, layout, paint, and store
  `document`/`styles` as today. Late arrivals after the deadline (deep import chains) recascade and
  repaint at most once after the applicable graph settles, and background tabs render lazily on
  activation. A newly applicable pending sheet after resize joins the final update without
  restoring the blank loading state.
- Ceilings per load: 64 external occurrences, separate 32 MiB retained-raw and unique-decoded
  budgets, 10 MiB per resource (already enforced by `MAX_BODY_BYTES`), and import depth 8. Crossing
  either aggregate byte budget disables all external CSS for the load. Repeated non-cyclic
  occurrences apply repeatedly but share a single fetch; requested and redirected URL identities
  both terminate cycles.

### 5. Selector bucketing

- `src/css/cascade.rs`: build a rule map once per cascade, bucketed by the rightmost simple selector
  (id → class → local name → universal), then match only the candidate buckets for each element.
  Selector introspection comes from the `selectors` crate rather than hand-parsing.
- *Proof:* on the fixture corpus the bucketed cascade produces a `StyleTree` identical to the naive
  path (kept as a test-only reference implementation), and a synthetic 5,000-rule × 2,000-element
  benchmark stays inside the M6 perf budget.

### 6. `@media` features

- `src/css/parser.rs`: extend the media grammar from type-only to `(feature)` and `(feature: value)`
  with `and`, keeping the existing fail-closed `MediaQuery::Never` recovery for anything unsupported.
- `src/css/cascade.rs`: `MediaContext` — which already carries `palette` and `state` since M1-B —
  gains `scripting: bool`, explicit light/dark color scheme and the content viewport `Size`; evaluation covers
  `scripting`, `prefers-color-scheme`, `width`/`height` including `min-`/`max-` prefixes.
- `src/app/controller.rs`: `on_resize` must **recascade before re-layout** when the content size
  changes, because width/height queries can now change which rules match. This extends the M1-B
  width-only invalidation rule and belongs in the roadmap's Decisions log.

### 7. Tests

- Unit and pipeline fixtures cover out-of-order completion, stale-generation drop, redirect/base
  resolution, import cycles, depth and byte budgets, cascade order, MIME failure isolation,
  encoding selection, deadline coalescing, background activation and resize applicability.
- Synthetic Wikipedia-style navigation is hidden by an external `display: none`, proving the
  payoff end to end through both the application and `--dump` driver.
- Gates after each step: `cargo fmt --check`, both clippy invocations with `-D warnings`,
  `cargo test`, `cargo test --features js`.

## Audit findings from the 2026-08-22 pass (fold into the roadmap when M1-C lands)

Re-audited after M1-B, including a self-review of that milestone's code:

| # | Finding | Evidence | Owner |
|---|---|---|---|
| 1 | **Every `file://` body is parsed as HTML.** `FileFetch` always returns `content_type: None`, and `response_kind(None)` is `Html`, so a local `.png`, `.txt` or `.pdf` is parsed and painted as garbage. Fix: guess the type from the path extension before the M2 sniffing work. | `src/net/file.rs:33`, `src/app/render.rs:26` | M2 content-type item, cheap enough to pull earlier |
| 2 | **`base_href` walked the DOM recursively.** A deeply nested document could exhaust the stack before layout ever ran. | `src/html/sink.rs` | fixed iteratively in M1-C |
| 3 | `is_word_joiner` tests `ch.is_alphabetic()`, but its only caller already required `!ch.is_alphanumeric()` — a dead condition added in M1-B. | `src/layout/engine.rs` break-opportunity helpers | M1-D cleanup |
| 4 | `fill_background` computes `.min(rect.row + rect.height)` twice; the second term is redundant. | `src/paint/painter.rs` | M6 cleanup |
| 5 | `legible_foreground` runs six `powf` calls per painted cell, though adjacent cells almost always share a style. A one-entry memo removes nearly all of it. | `src/paint/painter.rs` `into_row` | M6 perf gate, alongside the `format_inline` memoization |
| 6 | Foreign-namespace elements (`svg`, `math`) do not inherit `white-space`: `ua_style` early-returns a style that carries colour but drops the whitespace mode. Pre-existing, visible only for text inside `<pre><svg>`. | `src/css/cascade.rs` `ua_style` | M1-D, with the other cascade fidelity work |
| 7 | A descendant can cancel an ancestor's `text-decoration`; real browsers propagate decorations irrevocably to in-flow descendants. Our model inherits the bit, so `text-decoration: none` on a child removes the parent's underline. | `src/css/cascade.rs` inheritance + `apply_css_wide` | record as a known deviation; revisit in M1-D |

All findings remain with their owning milestones. M1-C does not pull unrelated layout or paint
cleanup into the stylesheet implementation.

## Roadmap and docs

Implemented on 2026-08-22. `ROADMAP.md` records the completed boxes, locked decisions, unchanged
dependency pins and test counts; README documents linked/imported stylesheets and dump dimensions.

## Verification

1. All five local gates and the selector benchmark are green.
2. Synthetic Wikipedia-style navigation is hidden end to end through both the app and dump driver.
3. Existing embedded-only render goldens remain unchanged.
4. Live Wikipedia, example.com and DuckDuckGo Lite terminal smokes remain user-run.
5. Bucketed vs naive cascade agree on every fixture; the synthetic benchmark median is 26.0575 ms.
