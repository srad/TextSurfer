<div align="center">

# TextSurfer

### A text-mode web browser, built in Rust on ratatui

![rustc](https://img.shields.io/badge/rustc-1.97.1-orange?style=flat-square&logo=rust)
![edition](https://img.shields.io/badge/edition-2024-blueviolet?style=flat-square)
![tui](https://img.shields.io/badge/TUI-ratatui-ff9e18?style=flat-square)
![gates](https://img.shields.io/badge/gates-fmt%20%E2%9C%93%20clippy%20%E2%9C%93%20tests%20%E2%9C%93-brightgreen?style=flat-square)
![corpus](https://img.shields.io/badge/html5lib%20corpus-95.2%25%20raw%20%E2%86%92%20100%25%20with%20xfail-important?style=flat-square)
![async](https://img.shields.io/badge/async-none-lightgrey?style=flat-square)

*Parse the web as text. No GPU, no JavaScript-by-default, no distractions.*

</div>

TextSurfer is a terminal browser in the tradition of lynx and w3m: it fetches real pages, parses them
with the same standards-grade engines the web is built on, and renders them as styled text in your
terminal. The UI is **I/O-free by construction** — events arrive as domain types, fetch results are
delivered through a single generation-tagged channel, and time is injected. Every module boundary is
a trait, so implementations are replaceable and testable in isolation.

```
 File  Navigate  View  Help                TextSurfer
│┌ example.com ┐                                      │
├───────────────┘─────────────────────────────────────┤
│[‹] [›] [↻] [⌂]  URL: │https://example.com           │
├─────────────────────────────────────────────────────┤
│Example Domain                                       │
│                                                     │
│This domain is for use in illustrative examples.    │
│                                                     │
Ready                              https://example.com
```

## Features

**Current (M0, M1-R and M1.5 complete; M1-A and M1-B done, awaiting human smoke;
M1-C done, awaiting human smoke; M1-D done, awaiting human VGA smoke; terminal smoke deferred)**

- Real HTTP(S) and `file://` loading via a fixed, joined 4-worker fetch pool (ureq, OS-native
  certificate roots) with timeouts, cancellation and a 10 MiB response limit. Subresources are
  same-scheme: a remote page cannot name `file:///…` in a `<link>` and have the browser read local
  files for it
- Full HTML5 parsing through html5ever into an indextree-backed DOM boundary (`<base href>`, quirks
  mode, foreign content, adoption agency, foster parenting, detached template fragments)
- Standards-grade encoding detection: BOM → HTTP header → `<meta charset>` prescan → UTF-8
- Media-type handling through mediatype: HTML/XHTML parse, plain text stays literal, unsupported
  valid types render a controlled error page
- Scheme routing: `http(s)://`, `file://`, a responsive colored half-block `about:blank` start page,
  unknown schemes rejected
  with a status error
- Stable tab, generation and resource-tagged fetch results: concurrent document stylesheets land in
  the right load, background tabs render lazily, and superseded responses are discarded
- DOS/QBasic-style menu, raised tab strip with clean gaps and an aligned active-tab divider,
  navigation toolbar, grapheme-safe address editor, unbounded scrolling and centralized
  resize/mouse geometry
- Hierarchical rendering path: cssparser + selectors cascade inline, embedded, linked and recursively
  imported CSS in document order, with selector bucketing and terminal-aware `@media` features for
  scripting, color scheme and CSS-pixel viewport dimensions, including MQ4 ranges. CSS absolute,
  font-relative and viewport-relative lengths resolve through a shared 8×16 cell metric; Taffy
  sizes nested and anonymous block flow,
  while an isolated table formatter provides initial anonymous-box, auto/fixed-track, span, caption
  and nested-table support; textwrap and Unicode-aware fragments reflow six whitespace modes, and
  sparse paint emits clipped terminal-cell lines and merged borders. Table cells share the normal
  Unicode/white-space formatter; nested tables retain source order, and captions keep box styling.
  CSS Display outside/inside modes survive cascade: `contents` elides its principal box without
  losing inheritance or links, inline flow-root/table/flex/grid boxes remain atomic, and
  misparented table roles receive ownerless anonymous wrappers after contents elision
- Generated content and list-marker support: `::before`, `::after` and `::marker` match, `content`
  supports strings, `counter()`, `counters()` and `attr()`, and CSS counters run over a depth-scoped
  stack. Ordered lists number, nested lists number independently, and `list-style-type` covers
  decimal, roman, alphabetic and the disc/circle/square bullets that step with nesting depth.
  Outside markers hang in a field shared and right-aligned across sibling items, so `9.` and `10.`
  meet one text column and wrapped lines align under the item text; `<ol start>`, `<ol reversed>`,
  `<li value>` and `ol`/`ul` `type` are honoured. Authored `display: list-item` makes any element a
  real list item, the CSS-wide keywords clear `content` and `counter-*` the way they clear every
  other property, and `content: normal` on `::marker` defers to the UA marker while `content: none`
  suppresses it
- Styled paint seam: author `color`, `background-color`, `font-weight` and `text-decoration` reach
  the terminal as coloured, bold, underlined and struck spans — with a contrast pass that keeps
  unreadable author colours legible over the theme's field — plus link and hit geometry carried
  through the display list for keyboard and mouse targeting. Partial-alpha foregrounds composite
  over the effective cell background; transparent text loses its ink while retaining layout, link
  and hit geometry
- `<img>` renders nonempty alternative text as `[alt]`, deliberately empty or whitespace-only
  alternatives as nothing, and `[img]` when `alt` is absent; `<hr>` spans the content width
- Dynamic selectors: `:link`, `:any-link`, `:hover`, `:focus`, `:active`, `:checked`, `:enabled`
  and `:disabled` parse against injected state; `:visited` never matches, so page styling cannot
  observe history. Host-language-correct form states and distinct `:focus-visible`/`:focus-within`
  behavior remain M2/M3 work
- `--dump` renders a page to stdout and exits through the same stylesheet loader and painter the TUI
  uses; `--cols` and `--rows` provide exact content dimensions for scripting and golden diffs
- **The default VGA frontend** behind the default `vga` feature opens a window and
  renders the same chrome with **our own CP437 8x16 face** instead of the host terminal's font. The
  DOS look is mostly the font, and inside a terminal the font belongs to the user — owning a
  framebuffer is the only way to own the face, the cell metric and the palette together. It also
  doubles the usable columns (1280x800 is 160x50). Glyphs come from CP437 first, so the chrome stays
  authentically DOS, then GNU Unifont for the rest of the BMP — curly quotes, dashes and non-Latin
  scripts fall outside the code page and would otherwise render as replacement boxes. Both faces are
  8x16, with Unifont's wide glyphs spanning exactly two cells. The terminal fallback is selected
  with `--terminal`; `--vga` remains a compatibility alias. A
  `--no-default-features` build keeps the terminal-only dependency profile
- WPT html5lib tree-output conformance corpus vendored as test fixtures — 1,922 cases, zero network
  in tests; error-count comparison is an open harness follow-up

**Next on the roadmap** (see `ROADMAP.md`, the single source of truth): links/forms/search (M2),
then mouse (M3), and the JavaScript seam/Boa integration (M4–M5).

## Architecture

```
 main.rs — frontend adapter: CLI · VGA/terminal selection · event mapping and loops
   ▼
 app — I/O-free composition root · controller · tabs · Navigate adapter · start page
   ├──────────────► ui — ratatui chrome and widgets
   ├──────────────► net — UreqFetch · FileFetch · joined 4-worker FetchPool
   └──────────────► pipeline — PageLoad resource graph · render facade · --dump
                       ▼
                  html → core DOM → css → layout → paint → DisplayList
```

Single crate, module-per-layer: `core` · `net` · `html` · `css` · `layout` · `paint` · `script` ·
`pipeline` · `ui` · `app` + a thin `main`, plus `vga` behind its feature. Concrete wiring happens
only in `app`/`main`; the app never imports crossterm or a concrete fetch implementation, and time
remains injected.

Because `ui` draws into a backend-agnostic ratatui `Frame`, the frontend is swappable: `main` picks
either the crossterm terminal or `vga`'s window, and everything below `ui` is identical in both.

## Getting started

**Requirements:** Rust **1.88+** (edition 2024). Windows, macOS and Linux are supported; HTTPS
uses OS-native certificate roots.

```console
$ cargo build --release
$ cargo run --release            # VGA window and start page
$ cargo run --release -- --terminal   # frozen terminal compatibility frontend
$ cargo run -- --url https://example.com   # VGA window at a URL
$ cargo run -- --user-agent TextSurferDev/1 --js off
$ cargo run -- --dump --cols 60 --rows 24 --url https://example.com   # render to stdout, no TUI
$ cargo run --release -- --vga-scale 2                                # 2x pixels, for HiDPI
```

Type `/` to focus the address bar, enter a URL or search terms, press `Enter`. TextSurfer falls back
to DuckDuckGo's lite search for anything that isn't a URL.

## Key bindings

| Key | Action |
|---|---|
| `/` | Focus address bar (typing `q` never quits while focused) |
| `Enter` | Load address / search |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+N` / `Ctrl+P` | Next / previous tab |
| `Alt+Left` / `Alt+Right` | Back / forward |
| `r` / `Alt+Home` | Reload / start page |
| `F10` or `Alt+F/N/V/H` | Open the menu bar |
| `j` / `k` · arrows | Scroll down / up |
| `Space` / `b` · `PageDown` / `PageUp` | Page down / up |
| `Home` / `End` | Top / bottom of page |
| `q` | Quit |

## Testing & conformance

- **html5lib corpus** — 62 `.dat` files vendored from WPT `html/syntax/parsing/resources/` at a
  pinned commit. 1,922 cases run; **95.2% pass raw, 100% green with the xfail manifest** (each
  entry cites its reason: cases unreachable from the HTML tokenizer, a spec-undefined fragment
  case, and elements that postdate html5ever 0.39). Fixtures are pinned by SHA-256; tests never
  touch the network.
- **Contract suites** — trait implementations pass capability-parameterized suites; fetch and app
  integration tests use fakes and synthetic middleware, with no real network or clock.
- **Property tests** — `url_fix` and the grapheme-aware address buffer obey laws under proptest,
  joined by the six layout laws: viewport-width monotonicity, painted-row bounds, disjoint leaf
  glyph cells, laminar per-row box families, engine-backed deepest-hit round trips, and scroll
  clamping as a fixed point under arbitrary key sequences.
- **Render goldens** — fixture pages cover margins, headings, borders, links, wide
  characters, `pre`, ordered/nested/reversed lists and their marker alignment, `::before`/`::after`
  with counters and `attr()`, simple and collapsed/spanned tables, nested/captioned tables, and
  fixed-layout overflow and outer/inner display modes, with assertions for link geometry, colour
  contrast and `--dump` parity.

Gates are local-only (no CI) and must be green before anything is marked done:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test
cargo test --features js
cargo test --features vga
cargo test --no-default-features
```

The suite covers unit, integration, corpus and binary-boundary behavior without network access or
the real clock. No test opens a window: the framebuffer frontend is exercised headlessly, including
a launch test pinned to a 1 MB stack — the size the main thread actually gets, rather than the
larger one the test harness would otherwise hand it.

## Roadmap

Status, decisions, acceptance criteria and the updates log live in
**[`ROADMAP.md`](ROADMAP.md)** — read it first if you want to contribute. Milestones: M0
foundations ✅ · M1-A parse pipeline ✅ · M1-R stabilization ✅ · M1.5 chrome ✅ · M1-B
style/layout/paint implemented, awaiting the human terminal smoke · M1-C external CSS implemented,
awaiting human smoke · M1-D layout completeness implemented, awaiting the human VGA smoke ·
M2 tabs/keyboard/forms · M3 mouse · M4 JS seam · M5 Boa · M6 stretch.

## Built on great libraries

[html5ever](https://github.com/servo/html5ever) · [ratatui](https://github.com/ratatui/ratatui) ·
[ureq](https://github.com/algesten/ureq) · [encoding_rs](https://github.com/hsivonen/encoding_rs) ·
[url](https://github.com/servo/rust-url) · [indextree](https://github.com/saschagrunert/indextree) ·
[cssparser](https://github.com/servo/rust-cssparser) ·
[cssparser-color](https://github.com/servo/rust-cssparser) ·
[selectors](https://github.com/servo/stylo) ·
[Taffy](https://github.com/DioxusLabs/taffy) · [textwrap](https://github.com/mgeisler/textwrap) ·
[thiserror](https://github.com/dtolnay/thiserror) · [boa_engine](https://github.com/boa-dev/boa)
(behind the `js` feature) · [winit](https://github.com/rust-windowing/winit) ·
[softbuffer](https://github.com/rust-windowing/softbuffer) ·
[unifont-bitmap](https://github.com/SolraBizna/unifont-bitmap) (behind the `vga` feature) — plus the
[Web Platform Tests](https://github.com/web-platform-tests/wpt) corpus for conformance.

The CP437 face is generated from [pcface](https://github.com/susam/pcface)'s Modern DOS 8x16 bitmaps
(MIT or CC0); the fallback face is [GNU Unifont](https://unifoundry.com/unifont/) (SIL OFL 1.1).
