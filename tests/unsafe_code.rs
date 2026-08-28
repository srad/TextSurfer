//! Pins the crate's one permitted `unsafe_code` exception.
//!
//! `src/lib.rs` carries `deny(unsafe_code)` rather than `forbid` only because Stylo's `TElement`
//! declares seven `unsafe fn` methods for Gecko's benefit, and `forbid` cannot be overridden
//! anywhere in the crate. `css::stylo::dom` is the sole module allowed to opt out. See the locked
//! decision in ROADMAP.md.

use std::path::{Path, PathBuf};

const PERMITTED: &str = "src/css/stylo/dom/mod.rs";

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("the source directory is readable") {
        let path = entry.expect("the directory entry is readable").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn only_the_stylo_dom_adapter_allows_unsafe_code() {
    let mut sources = Vec::new();
    rust_sources(Path::new("src"), &mut sources);
    assert!(!sources.is_empty(), "the source tree should not be empty");

    let mut opted_out = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).expect("the source file is valid UTF-8");
        if text.contains("allow(unsafe_code)") {
            opted_out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    opted_out.sort();

    assert_eq!(
        opted_out,
        [PERMITTED],
        "exactly one module may opt out of the crate's unsafe_code denial"
    );
}

#[test]
fn the_crate_root_still_denies_unsafe_code() {
    let root = std::fs::read_to_string("src/lib.rs").expect("the crate root is readable");
    assert!(
        root.contains("#![deny(unsafe_code)]"),
        "the crate root must keep denying unsafe code"
    );
    assert!(
        !root.contains("#![allow(unsafe_code)]"),
        "the crate root must never allow unsafe code"
    );
}

/// Every `unsafe` method Stylo makes us implement, and which we may therefore call.
///
/// `TElement` declares these `unsafe fn` for Gecko's benefit — Gecko can race to allocate or leak
/// without exclusive access to the element. Our arena is single-threaded and owned by one style
/// session, so none of them carries a real proof obligation here. Nothing else may be `unsafe`.
const CALLABLE: &[&str] = &[
    "TElement::ensure_data",
    "TElement::clear_data",
    "TElement::set_dirty_descendants",
    "TElement::unset_dirty_descendants",
    "TElement::set_handled_snapshot",
];

/// The adapter's own `unsafe fn` bodies are ordinary safe code under edition 2024, so the only
/// `unsafe` blocks it may contain are calls to the trait methods above — the safe wrappers that let
/// the rest of the crate stay clean. A raw-pointer dereference or a transmute would fail this.
///
/// Test modules are exempt: calling an `unsafe fn` needs a block wherever it happens.
#[test]
fn the_stylo_adapter_uses_unsafe_only_to_call_stylos_own_trait_methods() {
    let mut sources = Vec::new();
    rust_sources(Path::new("src/css/stylo"), &mut sources);
    let mut checked = 0usize;
    for path in sources {
        if path.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the source file is valid UTF-8");
        for (number, line) in text.lines().enumerate() {
            if !line.contains("unsafe {") {
                continue;
            }
            checked += 1;
            assert!(
                CALLABLE.iter().any(|callable| line.contains(callable)),
                "{}:{} uses unsafe for something other than a Stylo trait call: {}",
                path.display(),
                number + 1,
                line.trim()
            );
        }
    }
    assert!(
        checked > 0,
        "the adapter should still be calling Stylo's unsafe trait methods; \
         if that changed, this guard needs revisiting rather than deleting"
    );
}
