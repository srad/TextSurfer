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

**Current (M0, M1-A, M1-R and M1.5 complete; M1-B in progress)**

- Real HTTP(S) and `file://` loading via a fixed, joined 4-worker fetch pool (ureq, OS-native
  certificate roots) with timeouts, cancellation and a 10 MiB response limit
- Full HTML5 parsing through html5ever into an indextree-backed DOM boundary (`<base href>`, quirks
  mode, foreign content, adoption agency, foster parenting, detached template fragments)
- Standards-grade encoding detection: BOM → HTTP header → `<meta charset>` prescan → UTF-8
- Media-type handling through mediatype: HTML/XHTML parse, plain text stays literal, unsupported
  valid types render a controlled error page
- Scheme routing: `http(s)://`, `file://`, `about:blank` start page, unknown schemes rejected
  with a status error
- Stable-tab-ID and generation-tagged fetch results: background-tab results land in the right tab,
  while superseded responses are discarded
- DOS/QBasic-style menu, raised tab strip with clean gaps and an aligned active-tab divider,
  navigation toolbar, grapheme-safe address editor, unbounded scrolling and centralized
  resize/mouse geometry
- Hierarchical rendering path: cssparser + selectors cascade embedded and inline CSS, including
  inherited whitespace and type-only conditional `@media` rules; Taffy sizes nested and anonymous
  block flow with fixed content-box/border-box widths, textwrap and Unicode-aware fragments reflow
  six whitespace modes, and sparse paint emits clipped terminal-cell lines and borders
- WPT html5lib conformance corpus vendored as test fixtures — 1,922 cases, zero network in tests

**Next on the roadmap** (see `ROADMAP.md`, the single source of truth): complete paint, rendering
goldens and the remaining layout laws (M1-B), external stylesheets (M1-C), links/forms/search (M2),
mouse (M3), and the JavaScript seam/Boa integration (M4–M5).

## Architecture

```
┌──────────┐      ┌────────────────┐      ┌────────┐      ┌───────────┐
│  events  │ ───► │   app          │ ───► │   ui   │ ───► │  ratatui  │
│  (core)  │      │  (I/O-free)    │      │widgets │      │  chrome   │
└──────────┘      └───────┬────────┘      └────────┘      └───────────┘
                          │  deliver_fetch (tab ID + generation)
                  ┌───────▼────────┐
                  │  net           │  4 joined workers
                  │  UreqFetch     │  ureq · crossbeam · mediatype
                  │  FileFetch     │
                  │  FetchPool     │
                  └───────┬────────┘
                  ┌───────▼────────┐
                  │  html          │  html5ever → indextree DOM
                  │  css/layout    │  selectors → Taffy → paint
                  └────────────────┘
```

Single crate, module-per-layer: `core` · `net` · `html` · `css` · `layout` · `paint` · `script` ·
`ui` · `app` + a thin `main`. Concrete wiring happens only in the composition root; the app itself
never touches crossterm, the network, or the clock.

## Getting started

**Requirements:** Rust **1.88+** (edition 2024). Windows, macOS and Linux are supported; HTTPS
uses OS-native certificate roots.

```console
$ cargo build --release
$ cargo run --release            # start page
$ cargo run -- --url https://example.com
$ cargo run -- --user-agent TextSurferDev/1 --js off
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
- **Property tests** — `url_fix` and the grapheme-aware address buffer obey laws under proptest;
  viewport-width monotonicity and painted-row bounds have landed, while the remaining layout laws
  stay explicit M1-B completion gates.

Gates are local-only (no CI) and must be green before anything is marked done:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --features js
```

The suite covers unit, integration, corpus and binary-boundary behavior without network access or
the real clock.

## Roadmap

Status, decisions, acceptance criteria and the updates log live in
**[`ROADMAP.md`](ROADMAP.md)** — read it first if you want to contribute. Milestones: M0
foundations ✅ · M1-A parse pipeline ✅ · M1-R stabilization ✅ · M1.5 chrome ✅ · M1-B
style/layout/paint in progress · M1-C external CSS · M2 tabs/keyboard/forms · M3 mouse · M4 JS
seam · M5 Boa · M6 stretch.

## Built on great libraries

[html5ever](https://github.com/servo/html5ever) · [ratatui](https://github.com/ratatui/ratatui) ·
[ureq](https://github.com/algesten/ureq) · [encoding_rs](https://github.com/hsivonen/encoding_rs) ·
[url](https://github.com/servo/rust-url) · [indextree](https://github.com/saschagrunert/indextree) ·
[cssparser](https://github.com/servo/rust-cssparser) · [selectors](https://github.com/servo/stylo) ·
[Taffy](https://github.com/DioxusLabs/taffy) · [textwrap](https://github.com/mgeisler/textwrap) ·
[thiserror](https://github.com/dtolnay/thiserror) · [boa_engine](https://github.com/boa-dev/boa)
(behind the `js` feature) — plus the [Web Platform Tests](https://github.com/web-platform-tests/wpt)
corpus for conformance.
