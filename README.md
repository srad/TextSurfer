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
with the same standards-grade engines the web is built on, and renders them as styled text. The UI is
**I/O-free by construction** — events arrive as domain types, fetch results are delivered through a
single generation-tagged channel, and time is injected. Every module boundary is a trait, so
implementations are replaceable and testable in isolation.

```
 File  Navigate  View  Help                 TextSurfer
│┌ example.com [■]┐                                  │
│┘                └──────────────────────────────────┤
│[‹] [›] [↻] [⌂]  URL: │https://example.com          │
├────────────────────────────────────────────────────┤
│Example Domain                                      ▲
│                                                    █
│This domain is for use in illustrative examples.    ▒
│                                                    ▼
Ready                              https://example.com
```

## Features

**Networking and documents**

- HTTP(S) and `file://` loading through a fixed 4-worker pool (ureq, OS-native certificate roots)
  with timeouts, cancellation and a 10 MiB response limit. Subresources are same-scheme, so a remote
  page cannot read local files through a `<link>`
- HTTP requests identify the released TextSurfer version and project URL by default; `--user-agent`
  remains an exact override. Opt-in structured file diagnostics separate rate limits, transport
  failures, unsupported formats and corrupt image data; performance diagnostics add a correlated
  Chrome/Perfetto timeline without writing to the terminal
- Full HTML5 parsing via html5ever into an indextree-backed DOM (`<base href>`, quirks mode, foreign
  content, adoption agency, foster parenting, detached template fragments)
- WHATWG encoding detection (BOM → header → `<meta charset>` prescan → UTF-8) and media-type routing:
  declared HTML/XHTML parses, plain text stays literal, unsupported types render a controlled error
  page, and missing, malformed or generic types use bounded HTML/text/binary sniffing
- A server's own 4xx/5xx page renders with the status in the context bar; an error response is never
  accepted as a stylesheet. Zero-delay `meta refresh` (including `<noscript>` fallbacks) replaces its
  wrapper history entry and stops after eight hops

**Rendering**

- Cascade over inline, embedded, linked and recursively imported CSS in document order (cssparser +
  selectors), with selector bucketing and terminal-aware `@media` for scripting, colour scheme and
  CSS-pixel viewport dimensions including MQ4 ranges
- Taffy-backed block, flex, grid and physical float layout — wrapping, gaps, alignment, ordering,
  growth, named grid lines and areas, auto-placement, atomic inline-flex and inline-grid, and text
  shaping around left/right floats — beside an in-house table formatter with anonymous boxes,
  spans, captions and nesting
- CSS lengths resolve through a frontend-injected cell metric; `calc()`/`min()`/`max()`/`clamp()`
  reach sizing, margins, padding, insets, gaps, flex basis, font size and grid tracks
- Custom properties with stable Level 1 `var()` semantics: inherited variables, nested and empty
  fallbacks, cycle invalidation and token-safe substitution
- Generated content and list markers: `::before`/`::after`/`::marker`, `counter()`, `counters()`,
  `attr()`, and outside markers that hang in a shared right-aligned field so `9.` and `10.` meet one
  text column
- Overflow clips paint and hit geometry at nested padding boxes, `visibility` preserves layout while
  suppressing hidden descendants, and relative/absolute/fixed positioning uses signed insets
- Author `color`, `background-color`, `font-weight` and `text-decoration` reach the terminal as
  coloured, bold, underlined and struck spans, with a contrast pass that keeps unreadable author
  colours legible and alpha compositing over the effective cell background
- Static `<img src>` loads bounded PNG, JPEG, WebP and first-frame GIF without delaying first paint.
  VGA paints native RGBA pixels; the terminal selects Sixel/Kitty/iTerm2 once and falls back per
  placement to alpha-composited halfblocks
- Operable forms: text/password inputs and multiline textareas share the address bar's grapheme-safe
  editor with selection, clipboard commands and an opaque themed context menu. Checkbox/radio,
  buttons and single-select controls support reset and bounded GET or URL-encoded POST submission;
  placeholders are dimmed, disappear on input and are never submitted
- Bounded by construction: block nesting is capped, a layout the engine refuses degrades to a
  message instead of aborting, and `:visited` never matches so page styling cannot observe history

**Chrome and interaction**

- DOS/QBasic menu bar, raised tab strip with Turbo Vision `[■]` close boxes, navigation toolbar,
  shared grapheme-safe text-field widget for the address and page forms, and a page scrollbar whose
  caps step, trough pages and thumb drags
- Five session-scoped retro colour schemes in the View menu: Turbo Vision, Norton, Amber CRT, Green
  Phosphor and Paper White (which also exposes a light `prefers-color-scheme` to CSS)
- Mouse navigation in both frontends: links activate on release over the press target, middle-click
  and `target="_blank"` open tabs, the wheel scrolls, side buttons walk history, hovering a link
  previews it in the status bar and shows a hand cursor
- `--dump` renders a page to stdout through the same loader, refresh path and painter the TUI uses;
  `--cols`/`--rows` give exact dimensions for scripting and golden diffs

**Frontends**

The default **VGA frontend** opens a window and renders the chrome with **our own CP437 8×16 face**
instead of the host terminal's font. The DOS look is mostly the font, and inside a terminal the font
belongs to the user — owning a framebuffer is the only way to own the face, the cell metric and the
palette together. It also doubles the usable columns (1280×800 is 160×50). Glyphs come from CP437
first, then GNU Unifont for the rest of the BMP, both 8×16 with Unifont's wide glyphs spanning
exactly two cells.

The terminal fallback is selected with `--terminal`; a `--no-default-features` build keeps the
terminal-only dependency profile.

## Architecture

```
 main.rs — frontend adapter: CLI · VGA/terminal selection · event mapping and loops
   ▼
 app — I/O-free composition root · controller · tabs · Navigate adapter · start page
   ├──────────────► ui — ratatui chrome and widgets
   ├──────────────► net — UreqFetch · FileFetch · 4-worker FetchPool
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

**Requirements:** Rust **1.88+** (edition 2024) and **Python 3 on `PATH`**. Windows, macOS and Linux
are supported; HTTPS uses OS-native certificate roots.

Python is a build-time dependency of the `stylo` CSS engine, whose `build.rs` generates the property
tables from Mako templates. The templating packages are vendored in the crate, so nothing needs
installing beyond the interpreter itself. No Gecko, bindgen, nightly Rust or C++ toolchain is
involved.

```console
$ cargo build --release
$ cargo run --release                                  # VGA window and start page
$ cargo run --release -- --terminal                    # terminal compatibility frontend
$ cargo run -- --url https://example.com               # VGA window at a URL
$ cargo run -- --log-file textsurfer.log --url https://example.com
$ cargo run -- --diagnostics target/diagnostics/run --url https://example.com
$ cargo run -- --user-agent TextSurferDev/1 --js off
$ cargo run -- --dump --cols 60 --rows 24 --url https://example.com   # render to stdout
$ cargo run --release -- --vga-scale 2                 # 2x pixels, for HiDPI
```

Type `/` to focus the address bar, enter a URL or search terms, press `Enter`. TextSurfer falls back
to DuckDuckGo's lite search for anything that isn't a URL.

Logging is disabled unless `--log-file` is supplied. The file is appended across runs and defaults
to `textsurfer=debug`; set `RUST_LOG` to another tracing filter when needed. Diagnostic URLs omit
credentials, query strings and fragments, but still contain hosts and paths, so inspect a log before
sharing it.

`--diagnostics <prefix>` is exclusive with `--log-file` and creates new `<prefix>.log` and
`<prefix>.trace.json` files. It never overwrites an earlier capture. The log uses the fixed
`textsurfer=debug,textsurfer::perf=trace` filter; open the JSON timeline in Perfetto or
`chrome://tracing` to correlate parsing, invalidation, queueing, cascade, layout, paint, presentation
and scrolling. Bodies, cookies and form values are not recorded.

## Key bindings

| Key | Action |
|---|---|
| `/` | Focus address bar (typing `q` never quits while focused) |
| `Enter` | Load address / search |
| `Ctrl+A` / `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Select all / copy / cut / paste in the focused text field |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+N` / `Ctrl+P` | Next / previous tab |
| `Alt+Left` / `Alt+Right` | Back / forward |
| `r` / `Alt+Home` | Reload / start page |
| `F10` or `Alt+F/N/V/H` | Open the menu bar |
| `F12` | Save the whole rendered page to `screenshots/*.png` |
| `j` / `k` · arrows | Scroll down / up |
| `Space` / `b` · `PageDown` / `PageUp` | Page down / up |
| `Home` / `End` | Top / bottom of page |
| `q` | Quit |

Open View with `Alt+V` (or through `F10`) to select a colour scheme. The bullet marks the current
session choice.

`F12` writes the page itself, not the screen: no chrome, and every painted row rather than the
screenful in view. A notice flashes over the page for three seconds with the file that was saved —
or why none was — and the same line stays on the status bar afterwards. The file is named for the frontend that drew it, as in
`screenshots/20260827-153012-vga.png` or `…-terminal.png`, because the window has scaled headings
and true raster images that a terminal capture cannot show. A very long page stops at a 32 MP budget
and the status line says how many of its rows were captured.

## Mouse

| Action | Result |
|---|---|
| Click a link | Follows it on release, if the press landed on the same link |
| Middle-click a link, or click a `target="_blank"` one | Opens it in a new tab |
| Wheel over the page | Scrolls three rows per notch (trackpad pixels accumulate) |
| Side buttons | Back / forward, from anywhere in the window |
| Click a tab chip / the `+` box | Switches tabs / opens one |
| Click a chip's `[■]` | Closes that tab, background or not, without switching to it |
| Click the scrollbar's `▲` / `▼` | Steps one row |
| Click its track above / below the thumb | Pages up / down |
| Drag the thumb | Scrolls to that position; keeps tracking off the bar, ends if the pointer leaves the window |
| Click `[‹] [›] [↻] [⌂]` | Back, forward, reload, start page |
| Click or drag in the address or a page text field | Places the caret or selects a range |
| Right-click an address or page text field | Opens the opaque themed cut/copy/paste/select-all menu |
| Click a menu title or item | Opens, toggles, dispatches; a click elsewhere closes the menu |
| Hover a link | Previews the URL in the status bar, hand cursor in the window |

The terminal frontend enables mouse capture while it runs, so your terminal's own selection needs
its override key (usually `Shift`) while TextSurfer has the screen.

## Testing & conformance

- **html5lib corpus** — 62 `.dat` files vendored from WPT at a pinned commit. 1,922 cases run;
  **95.2% pass raw, 100% green with the xfail manifest**, each entry citing its reason. Fixtures are
  pinned by SHA-256; tests never touch the network.
- **Contract suites** — trait implementations pass capability-parameterized suites; fetch and app
  integration tests use fakes, with no real network or clock.
- **Property tests** — `url_fix` and the shared grapheme-aware text-field state obey laws under proptest,
  joined by ten layout laws covering viewport monotonicity, painted-row bounds, disjoint glyph
  cells, laminar row families, deepest-hit round trips and scroll clamping.
- **Render goldens** — fixture pages covering margins, headings, borders, links, wide characters,
  `pre`, lists and markers, generated content, tables, display modes, flex, grid, floats and CSS
  math, with assertions for link geometry, colour contrast and `--dump` parity.
- **Rendering atlas** — one offline document of 11 named panels rendered at 40, 100 and 160 columns,
  asserted semantically and then locked by 36 styled-cell snapshots and 33 exact VGA PNG references.
  Ordinary runs never rewrite a reference.
- **Static WPT profiles** — a pinned, vendored slice run offline in an isolated child with a parent
  watchdog: `terminal-cell-v1` for cell rendering and crash safety, `vga-pixel-v1` for exact-RGB
  raster reftests. These are TextSurfer slices, not browser pixel conformance.
- **Browser-reference corpus** — real pages captured once from a headless Chromium with scripting
  disabled, storing both the bytes it fetched and the geometry it laid out, then compared offline.
  Because a terminal cell is exactly 8×16 CSS pixels, the browser's viewport is the same canvas we
  lay out in. The comparison is structural, never pixels: text the browser shows must appear, in its
  reading order, and text it keeps apart must never share a cell. Capturing is human-run; tests stay
  offline.

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

No test opens a window: the framebuffer frontend is exercised headlessly, including a launch test
pinned to a 1 MB stack — the size the main thread actually gets, rather than the larger one the test
harness would otherwise hand it.

The release app-render benchmark composes the production `App`, fetch, image-decode and render
queues without opening a frontend window. It reports numeric cold-load and settled-resize timings:

```console
cargo bench --bench app_render -- --url "https://en.wikipedia.org/wiki/Linux" --runs 5
cargo bench --bench app_render -- --fixture --runs 5
```

`--url` accepts any supported URL. `--fixture` replaces network access with the pinned Linux page
and deliberately delayed stylesheets and images so initial-render coalescing stays reproducible.

## Roadmap

Status, decisions in force, acceptance criteria and open plans live in
**[`ROADMAP.md`](ROADMAP.md)** — read it first if you want to contribute. Dated history lives in
`git log`, and the standing rules coding agents follow are in [`AGENTS.md`](AGENTS.md).

Next up: close general flow correctness against the browser-reference corpus, starting with spanning
table cells that truncate instead of wrapping, then stacking and inline-layout correctness alongside
CSS-pixel replaced sizing and static SVG. Run the representative-page image and styling smoke in VGA
and terminal before resuming cold-render optimization; retained-paint smoke, keymap unification,
keyboard link hints, the help overlay and in-page search follow that work.

## Built on great libraries

[html5ever](https://github.com/servo/html5ever) · [ratatui](https://github.com/ratatui/ratatui) ·
[ureq](https://github.com/algesten/ureq) · [encoding_rs](https://github.com/hsivonen/encoding_rs) ·
[url](https://github.com/servo/rust-url) · [indextree](https://github.com/saschagrunert/indextree) ·
[cssparser](https://github.com/servo/rust-cssparser) ·
[cssparser-color](https://github.com/servo/rust-cssparser) ·
[selectors](https://github.com/servo/stylo) ·
[Taffy](https://github.com/DioxusLabs/taffy) · [textwrap](https://github.com/mgeisler/textwrap) ·
[arboard](https://github.com/1Password/arboard) · [thiserror](https://github.com/dtolnay/thiserror) ·
[boa_engine](https://github.com/boa-dev/boa)
(behind the `js` feature) · [winit](https://github.com/rust-windowing/winit) ·
[softbuffer](https://github.com/rust-windowing/softbuffer) ·
[image](https://github.com/image-rs/image) · [ratatui-image](https://github.com/benjajaja/ratatui-image) ·
[unifont-bitmap](https://github.com/SolraBizna/unifont-bitmap) (behind the `vga` feature) — plus the
[Web Platform Tests](https://github.com/web-platform-tests/wpt) corpus for conformance.

The CP437 face is generated from [pcface](https://github.com/susam/pcface)'s Modern DOS 8x16 bitmaps
(MIT or CC0); the fallback face is [GNU Unifont](https://unifoundry.com/unifont/) (SIL OFL 1.1).
