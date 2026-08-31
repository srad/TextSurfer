<div align="center">

# TextSurfer

### A Rust text-mode web browser with native VGA and terminal frontends

![Rust 1.91+](https://img.shields.io/badge/Rust-1.91%2B-orange?style=flat-square&logo=rust)
![ratatui 0.30.2](https://img.shields.io/badge/ratatui-0.30.2-ff9e18?style=flat-square)
![968 passing tests](https://img.shields.io/badge/tests%20(VGA%2BJS)-968%20passing-brightgreen?style=flat-square)

</div>

TextSurfer fetches and renders real websites through its own HTML, CSS, layout and paint pipeline
rather than embedding another browser. Its default VGA window owns the font, cell geometry and
palette; a terminal frontend uses the same browser engine as a compatibility option.

The pipeline is split across replaceable, testable boundaries. Standards-focused libraries handle
parsing and core algorithms, while TextSurfer owns browser behavior, text-mode layout, painting and
interaction. JavaScript is optional and disabled at compile time unless the `js` feature is enabled.

<!-- Replace this comment with:
![TextSurfer VGA frontend](docs/screenshot.png)
-->

## Getting started

TextSurfer requires Rust 1.91 or newer and Python 3 on `PATH` for Stylo's build step.

```console
git clone https://github.com/srad/TextSurfer.git
cd TextSurfer
cargo check
cargo test
cargo build --release
```

The default build includes the VGA frontend. The terminal-only build removes its windowing and font
dependencies.

```console
cargo run --release -- --url https://example.com                 # default VGA window
cargo run --release -- --terminal --url https://example.com      # terminal frontend
cargo run --no-default-features -- --url https://example.com     # terminal-only build
cargo run --release -- --vga-scale 2                             # VGA at 2x pixel scale
```

JavaScript is compile-time optional and runtime selectable:

| Command | JavaScript behavior |
|---|---|
| `cargo run --features js -- --js auto --url https://example.com` | Build Boa and enable it automatically |
| `cargo run --features js -- --js on --url https://example.com` | Build Boa and require JavaScript to be on |
| `cargo run --features js -- --js off --url https://example.com` | Keep Boa compiled but disable it for this run |
| `cargo run -- --js off --url https://example.com` | Build and run without Boa |

`--js on` reports an error when the binary was built without the `js` feature. `--js off` always
wins, including in a JavaScript-enabled build.

## Components

| Component | Technology | Responsibility |
|---|---|---|
| UI | [ratatui](https://github.com/ratatui/ratatui) | Backend-independent chrome, widgets and frame composition |
| VGA and terminal frontends | [winit](https://github.com/rust-windowing/winit), [softbuffer](https://github.com/rust-windowing/softbuffer), [crossterm](https://github.com/crossterm-rs/crossterm), [CP437](https://github.com/susam/pcface) and [GNU Unifont](https://unifoundry.com/unifont/) | Native framebuffer and terminal presentation over the same UI |
| Networking | [ureq](https://github.com/algesten/ureq), [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam) | HTTP(S), local files and bounded resource workers |
| HTML and encoding | [html5ever](https://github.com/servo/html5ever), [encoding_rs](https://github.com/hsivonen/encoding_rs) | Standards-based parsing and character decoding |
| DOM | [indextree](https://github.com/saschagrunert/indextree) | Mutable document tree with stable node identities |
| CSS | [Stylo](https://github.com/servo/stylo), [cssparser](https://github.com/servo/rust-cssparser), [selectors](https://github.com/servo/stylo) | Parsing, selector matching, cascade and computed values |
| Layout and text | [Taffy](https://github.com/DioxusLabs/taffy), [textwrap](https://github.com/mgeisler/textwrap), [unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation) and [unicode-width](https://github.com/unicode-rs/unicode-width) | Block, flex, grid and float geometry with TextSurfer inline and table adapters |
| Images | [image](https://github.com/image-rs/image), [ratatui-image](https://github.com/benjajaja/ratatui-image) | Bounded decoding and native or terminal image presentation |
| JavaScript | [Boa](https://github.com/boa-dev/boa) 0.22 | Optional per-document script engine behind replaceable host contracts |
| CLI and diagnostics | [clap](https://github.com/clap-rs/clap), [tracing](https://github.com/tokio-rs/tracing) | Command-line configuration and opt-in structured diagnostics |

## Feature coverage

| Area | Current state | Next target or known limit |
|---|---|---|
| Loading and documents | **Supported:** HTTP(S), `file://`, redirects, encoding detection, HTML/plain-text routing and external styles, scripts and images | Additional browser protocols are outside the current scope |
| HTML and DOM | **Supported:** html5ever parsing, quirks mode, foreign content, templates and a mutable indextree DOM | The complete browser DOM API is not implemented |
| CSS cascade | **Partial:** Stylo parsing and cascade, selectors, media queries, custom properties, generated content, presentational hints and CSS math | Continue property coverage and practical-site parity |
| Layout | **Partial:** block, flex, grid, tables, floats, overflow, visibility and relative, absolute and fixed positioning | Complete stacking and `z-index` interactions, remaining inline geometry and horizontal RTL |
| Typography | **Partial:** Unicode cell widths, VGA bitmap typography, terminal styling and scaled headings | No remote fonts, complete `line-height`, vertical writing or advanced font selection |
| Images | **Partial:** PNG, JPEG, WebP, first-frame GIF, VGA RGBA painting, terminal image protocols and halfblock fallback | Add SVG, `picture`, `srcset`, `sizes`, CSS-pixel precision, `object-fit` and `object-position` |
| Forms | **Partial:** text/password fields, textarea, checkbox/radio, buttons, single-select, reset, GET and URL-encoded POST | Remaining input types, file uploads and complete browser form APIs |
| Interaction | **Partial:** tabs, history, links, mouse input, scrolling, CSS hover/focus/active, themes and shared text editing | Add keyboard link hints, the help overlay and in-page search |
| JavaScript engine | **Partial:** optional Boa 0.22, isolated engines, classic inline/external scripts, bounded Promise jobs, injected timers and resource-graph fetches | No script modules, top-level await or completed test262 target; `async`/`defer` semantics remain incomplete |
| JavaScript web APIs | **Partial:** document/title/location, ID and simple selector lookup, checked DOM mutation, attributes, `classList`, inline `style`, click handlers, console/alert, GET `fetch()` text/JSON and timers | No `addEventListener`, `querySelectorAll`, `innerHTML`, full/scoped selectors, request options, headers, CORS model or complete DOM |
| Frontends | **Supported:** native VGA is the default; terminal is the compatibility and no-default-features frontend | Representative human smoke remains part of milestone acceptance |
| Browser platform | **Not planned:** iframes/frames, cookies, remote fonts, transforms, border radius, multicolumn layout, vertical writing and syscall sandboxing | A future roadmap decision must explicitly reopen these areas |

Detailed status, decisions and acceptance criteria live in [ROADMAP.md](ROADMAP.md).
