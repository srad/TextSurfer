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
- Form controls (text, password, checkbox, radio, buttons, select, textarea) draw stand-ins sized to
  their CSS box, with `placeholder` dimmed and never submitted. They are not yet operable
- Bounded by construction: block nesting is capped, a layout the engine refuses degrades to a
  message instead of aborting, and `:visited` never matches so page styling cannot observe history

**Chrome and interaction**

- DOS/QBasic menu bar, raised tab strip with Turbo Vision `[■]` close boxes, navigation toolbar,
  grapheme-safe address editor and a page scrollbar whose caps step, trough pages and thumb drags
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

**Requirements:** Rust **1.88+** (edition 2024). Windows, macOS and Linux are supported; HTTPS uses
OS-native certificate roots.

```console
$ cargo build --release
$ cargo run --release                                  # VGA window and start page
$ cargo run --release -- --terminal                    # terminal compatibility frontend
$ cargo run -- --url https://example.com               # VGA window at a URL
$ cargo run -- --user-agent TextSurferDev/1 --js off
$ cargo run -- --dump --cols 60 --rows 24 --url https://example.com   # render to stdout
$ cargo run --release -- --vga-scale 2                 # 2x pixels, for HiDPI
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
| Click the address field | Focuses it and places the caret, keeping a half-typed URL |
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
- **Property tests** — `url_fix` and the grapheme-aware address buffer obey laws under proptest,
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

## Roadmap

Status, decisions in force, acceptance criteria and open plans live in
**[`ROADMAP.md`](ROADMAP.md)** — read it first if you want to contribute. Dated history lives in
`git log`, and the standing rules coding agents follow are in [`AGENTS.md`](AGENTS.md).

Next up: the human image smoke in VGA and terminal, then keyboard link navigation, form editing and
submission, in-page search, and the remaining M6 performance gate.

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
[image](https://github.com/image-rs/image) · [ratatui-image](https://github.com/benjajaja/ratatui-image) ·
[unifont-bitmap](https://github.com/SolraBizna/unifont-bitmap) (behind the `vga` feature) — plus the
[Web Platform Tests](https://github.com/web-platform-tests/wpt) corpus for conformance.

The CP437 face is generated from [pcface](https://github.com/susam/pcface)'s Modern DOS 8x16 bitmaps
(MIT or CC0); the fallback face is [GNU Unifont](https://unifoundry.com/unifont/) (SIL OFL 1.1).
