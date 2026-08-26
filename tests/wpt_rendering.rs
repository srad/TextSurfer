#[path = "support/wpt/mod.rs"]
mod wpt;

use std::env;
use std::path::Path;

#[test]
fn manifest_contracts() {
    wpt::assert_manifest_contracts();
}

#[test]
fn corpus_integrity() {
    wpt::assert_corpus_integrity();
}

#[test]
fn supervisor_contracts() {
    wpt::assert_supervisor_contracts();
}

#[test]
fn terminal_cell_conformance() {
    wpt::assert_corpus_conformance();
}

#[test]
#[ignore]
fn wpt_case_worker() {
    let mode = env::var("TEXTSURFER_WPT_WORKER_MODE").expect("worker mode");
    let result = env::var("TEXTSURFER_WPT_RESULT").expect("worker result path");
    wpt::run_worker(&mode, Path::new(&result));
}
