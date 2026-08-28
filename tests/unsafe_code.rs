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

/// The adapter's `unsafe fn` items exist only because the trait declares them. Under edition 2024
/// an `unsafe fn` body is ordinary safe code, so the implementation must contain no `unsafe`
/// *block* — that is the difference between satisfying a signature and actually asserting an
/// invariant. Test modules are exempt: calling an `unsafe fn` needs a block wherever it happens.
#[test]
fn the_stylo_dom_adapter_contains_no_unsafe_block() {
    let mut sources = Vec::new();
    rust_sources(Path::new("src/css/stylo"), &mut sources);
    for path in sources {
        if path.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the source file is valid UTF-8");
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            assert!(
                !(trimmed.starts_with("unsafe {") || trimmed.contains("= unsafe {")),
                "{}:{} introduces an unsafe block: {trimmed}",
                path.display(),
                number + 1
            );
        }
    }
}
