<div align="center">

# TextSurf

### A text-mode web browser, built in Rust on ratatui

![rustc](https://img.shields.io/badge/rustc-1.97.1-orange?style=flat-square&logo=rust)
![edition](https://img.shields.io/badge/edition-2024-blueviolet?style=flat-square)
![tui](https://img.shields.io/badge/TUI-ratatui-ff9e18?style=flat-square)
![gates](https://img.shields.io/badge/gates-fmt%20%E2%9C%93%20clippy%20%E2%9C%93%20tests%20%E2%9C%93-brightgreen?style=flat-square)
![corpus](https://img.shields.io/badge/html5lib%20corpus-95.2%25%20raw%20%E2%86%92%20100%25%20with%20xfail-important?style=flat-square)
![async](https://img.shields.io/badge/async-none-lightgrey?style=flat-square)

*Parse the web as text. No GPU, no JavaScript-by-default, no distractions.*

</div>

TextSurf is a terminal browser in the tradition of lynx and w3m: it fetches real pages, parses them
with the same standards-grade engines the web is built on, and renders them as styled text in your
terminal. The UI is **I/O-free by construction** — events arrive as domain types, fetch results are
delivered through a single generation-tagged channel, and time is injected. Every module boundary is
a trait, so implementations are replaceable and testable in isolation.

```
┌──────────────┬──────────────────────────────────────────┐
│ [start]      │  https://example.com                     │
├──────────────┴──────────────────────────────────────────┤
│  TextSurf - a text-mode browser                         │
│                                                         │
│    press / to type a URL or search terms, Enter to go   │
│    Ctrl+T new tab  Ctrl+W close  Ctrl+N/P switch        │
│    j/k or arrows scroll  Home/End jump  q quits         │
│                                                         │
│    loading https://example.com                          │
├─────────────────────────────────────────────────────────┤
│ accepted gen 1 - https://example.com/ (0 parse errors)  │
└─────────────────────────────────────────────────────────┘
```

## Features

**Current (M0 + M1-A complete)**

- Real HTTP(S) and `file://` loading via a detached 4-worker fetch pool (ureq, OS-native
  certificate roots) — the UI never blocks on the network
- Full HTML5 parsing through html5ever into a slotmap-backed DOM (`<base href>`, foreign content,
  adoption agency, foster parenting, templates)
- Standards-grade encoding detection: BOM → HTTP header → `<meta charset>` prescan → UTF-8
- Scheme routing: `http(s)://`, `file://`, `about:blank` start page, unknown schemes rejected
  with a status error
- Generation-tagged fetch results: stale responses from superseded navigations are dropped before
  they ever touch the DOM
- Tabs, address bar, focus-aware keymap, scrolling, status line
- Debug tree view: loaded pages render as the canonical html5lib tree dump
- WPT html5lib conformance corpus vendored as test fixtures — 1,922 cases, zero network in tests

**On the roadmap** (see `ROADMAP.md`, the single source of truth): CSS cascade and layout (M1-B),
link navigation (M2), mouse (M3), JavaScript seam (M4–M5), forms and config (M6).

## Architecture

```
┌──────────┐      ┌────────────────┐      ┌────────┐      ┌───────────┐
│  events  │ ───► │   app          │ ───► │   ui   │ ───► │  ratatui  │
│  (core)  │      │  (I/O-free)    │      │widgets │      │  chrome   │
└──────────┘      └───────┬────────┘      └────────┘      └───────────┘
                          │  deliver_fetch (generation-tagged)
                  ┌───────▼────────┐
                  │  net           │  4 detached workers
                  │  UreqFetch     │  ureq · std::fs · encoding_rs
                  │  FileFetch     │
                  │  FetchPool     │
                  └───────┬────────┘
                  ┌───────▼────────┐
                  │  html          │  html5ever → slotmap DOM
                  │  Html5everParser│ → tree dump (M1-A)
                  └────────────────┘
```

Single crate, module-per-layer: `core` · `net` · `html` · `css` · `layout` · `paint` · `script` ·
`ui` · `app` + a thin `main`. Concrete wiring happens only in the composition root; the app itself
never touches crossterm, the network, or the clock.

## Getting started

**Requirements:** Rust **1.97+** (edition 2024). Windows, macOS and Linux are supported; HTTPS
uses OS-native certificate roots.

```console
$ cargo build --release
$ cargo run --release            # start page
$ cargo run -- --url https://example.com
```

Type `/` to focus the address bar, enter a URL or search terms, press `Enter`. TextSurf falls back
to DuckDuckGo's lite search for anything that isn't a URL.

## Key bindings

| Key | Action |
|---|---|
| `/` | Focus address bar (typing `q` never quits while focused) |
| `Enter` | Load address / search |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+N` / `Ctrl+P` | Next / previous tab |
| `j` / `k` · arrows | Scroll down / up |
| `Home` / `End` | Top / bottom of page |
| `q` | Quit |

## Testing & conformance

- **html5lib corpus** — 62 `.dat` files vendored from WPT `html/syntax/parsing/resources/` at a
  pinned commit. 1,922 cases run; **95.2% pass raw, 100% green with the xfail manifest** (each
  entry cites its reason: cases unreachable from the HTML tokenizer, a spec-undefined fragment
  case, and elements that postdate html5ever 0.39). Fixtures are pinned by SHA-256; tests never
  touch the network.
- **Contract suites** — every trait implementation (fetch, pool, parser, JS engine, keymap) must
  pass capability-parameterized suites; fakes everywhere, no real network or clock in tests.
- **Property tests** — `url_fix` and the address-buffer obey laws under proptest; layout laws land
  with M1-B.

Gates are local-only (no CI) and must be green before anything is marked done:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features js
```

Current totals: 157 unit + 12 corpus + 3 end-to-end pipeline tests.

## Roadmap

Status, decisions, acceptance criteria and the updates log live in
**[`ROADMAP.md`](ROADMAP.md)** — read it first if you want to contribute. Milestones: M0
foundations ✅ · M1-A parse pipeline ✅ · M1-B style/layout/paint · M2 tabs & keyboard · M3 mouse ·
M4 JS seam · M5 Boa · M6 stretch.

## Built on great libraries

[html5ever](https://github.com/servo/html5ever) · [ratatui](https://github.com/ratatui/ratatui) ·
[ureq](https://github.com/algesten/ureq) · [encoding_rs](https://github.com/hsivonen/encoding_rs) ·
[url](https://github.com/servo/rust-url) · [slotmap](https://github.com/orlp/slotmap) ·
[thiserror](https://github.com/dtolnay/thiserror) · [boa_engine](https://github.com/boa-dev/boa)
(behind the `js` feature) — plus the [Web Platform Tests](https://github.com/web-platform-tests/wpt)
corpus for conformance.
