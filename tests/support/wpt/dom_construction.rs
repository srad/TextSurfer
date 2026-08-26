use std::path::Path;
use std::time::Duration;

use textsurfer::core::dom::{Document, ElementNs};

use super::manifest::CaseOutcome;
use super::supervisor::{supervise, write_result};

pub fn assert_deep_dom_construction_completes() {
    let outcome = supervise("dom-construction", None, Duration::from_secs(10));
    assert_eq!(outcome, CaseOutcome::Pass, "{outcome:?}");
}

pub fn run_worker(result_path: &Path) {
    let mut document = Document::new();
    let root = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let mut parent = root;
    for _ in 0..100_000 {
        let child = document.insert_element(Some(parent), "div", ElementNs::Html, vec![]);
        if document.parent(child) != Some(parent) {
            write_result(
                result_path,
                &CaseOutcome::AssertionMismatch("new child has the wrong parent".to_string()),
            );
            return;
        }
        parent = child;
    }
    let outcome = if document.root() == Some(root) && document.children(parent).is_empty() {
        CaseOutcome::Pass
    } else {
        CaseOutcome::AssertionMismatch("deep chain endpoints are incorrect".to_string())
    };
    write_result(result_path, &outcome);
}
