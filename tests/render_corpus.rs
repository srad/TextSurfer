#[path = "support/browser_corpus/mod.rs"]
mod browser_corpus;
#[path = "support/worker_supervisor.rs"]
mod worker_supervisor;

use std::env;
use std::path::Path;

#[test]
fn manifest_contracts() {
    browser_corpus::assert_manifest_contracts();
}

#[test]
fn comparison_contracts() {
    browser_corpus::assert_comparison_contracts();
}

#[test]
fn supervisor_contracts() {
    browser_corpus::assert_supervisor_contracts();
}

#[test]
fn browser_reference_comparison() {
    browser_corpus::assert_corpus_conformance();
}

#[test]
fn narrow_rtl_corpus_completes() {
    browser_corpus::assert_narrow_rtl_completes();
}

#[test]
#[ignore]
fn browser_corpus_worker() {
    let mode = env::var("TEXTSURFER_BROWSER_WORKER_MODE").expect("worker mode");
    let result = env::var("TEXTSURFER_BROWSER_RESULT").expect("worker result path");
    browser_corpus::run_worker(&mode, Path::new(&result));
}
