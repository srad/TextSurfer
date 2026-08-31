use std::time::Duration;

use serde_json::{Value, json};

use super::compare::{
    Direction, RunOccurrence, Token, directional_order, hidden_leak_findings, lcs_pairs,
    missing_findings, overlap_findings,
};
use super::manifest::{Manifest, Renderer, manifest_path};
use super::render::verify_integrity;
use super::supervisor::{CaseOutcome, supervise, supervise_case};

pub fn assert_manifest_contracts() {
    let manifest = Manifest::load().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(manifest.pages.len(), 6);
    assert_eq!(manifest.viewports.len(), 3);
    assert_eq!(manifest.renderers.len(), 2);
    assert_eq!(
        manifest
            .pages
            .iter()
            .flat_map(|page| &page.expectations)
            .count(),
        36
    );
    let source = std::fs::read_to_string(manifest_path()).expect("manifest source");
    let value: Value = serde_json::from_str(&source).expect("manifest JSON");

    let mut unknown = value.clone();
    unknown["unknown"] = json!(true);
    assert_manifest_error(unknown, "unknown field");

    let mut unsafe_slug = value.clone();
    unsafe_slug["pages"][0]["slug"] = json!("../escape");
    assert_manifest_error(unsafe_slug, "unsafe corpus slug");

    let mut duplicate = value.clone();
    let page = duplicate["pages"][0].clone();
    duplicate["pages"].as_array_mut().unwrap().push(page);
    assert_manifest_error(duplicate, "duplicate page");

    let mut incomplete = value.clone();
    incomplete["pages"][0]["expectations"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_manifest_error(incomplete, "incomplete expectation matrix");

    let mut unowned = value;
    let xfail = json!({
        "relation": "a1-missing",
        "count": 1,
        "sha256": "0".repeat(64),
        "owner": "",
        "reason": ""
    });
    unowned["pages"][0]["expectations"][0]["xfails"] = json!([xfail]);
    assert_manifest_error(unowned, "unowned xfail");

    verify_integrity(&manifest).unwrap_or_else(|error| panic!("{error}"));
}

pub fn assert_comparison_contracts() {
    let sequence = |items: &[&str]| {
        items
            .iter()
            .enumerate()
            .map(|(token, item)| RunOccurrence {
                text: (*item).to_string(),
                token,
            })
            .collect::<Vec<_>>()
    };
    let pairs = lcs_pairs(&sequence(&["A", "B", "A"]), &sequence(&["B", "A", "B"]));
    assert_eq!(pairs.len(), 2);
    assert_eq!(
        pairs,
        lcs_pairs(&sequence(&["A", "B", "A"]), &sequence(&["B", "A", "B"]))
    );

    let token = |text: &str, start: f64, direction, ordinal| Token {
        text: text.to_string(),
        band: 0,
        start,
        end: start + 8.0,
        direction,
        ordinal,
    };
    let ordered = directional_order(vec![
        token("A", 8.0, Direction::Ltr, 0),
        token("B", 0.0, Direction::Ltr, 1),
        token("ג", 16.0, Direction::Rtl, 2),
        token("ד", 24.0, Direction::Rtl, 3),
    ]);
    assert_eq!(
        ordered
            .iter()
            .map(|value| value.text.as_str())
            .collect::<Vec<_>>(),
        ["B", "A", "ד", "ג"]
    );

    let visible = vec![token("same", 0.0, Direction::Ltr, 0)];
    let hidden = vec![token("same secret", 0.0, Direction::Ltr, 1)];
    let ours = vec![token("same secret", 0.0, Direction::Ltr, 0)];
    assert!(missing_findings(&visible, &ours).is_empty());
    assert_eq!(hidden_leak_findings(&visible, &hidden, &ours).len(), 1);

    let browser = vec![
        token("repeat", 0.0, Direction::Ltr, 0),
        token("repeat", 24.0, Direction::Ltr, 1),
    ];
    let ours = vec![
        token("repeat", 0.0, Direction::Ltr, 0),
        Token {
            start: 4.0,
            end: 12.0,
            ..token("repeat", 4.0, Direction::Ltr, 1)
        },
    ];
    assert_eq!(overlap_findings(&browser, &ours).len(), 1);
}

pub fn assert_supervisor_contracts() {
    assert_eq!(
        supervise("pass", None, Duration::from_secs(10)),
        CaseOutcome::Pass
    );
    assert!(matches!(
        supervise("mismatch", None, Duration::from_secs(10)),
        CaseOutcome::AssertionMismatch(_)
    ));
    assert!(matches!(
        supervise("malformed", None, Duration::from_secs(10)),
        CaseOutcome::HarnessError(_)
    ));
    assert!(matches!(
        supervise("empty", None, Duration::from_secs(10)),
        CaseOutcome::HarnessError(_)
    ));
    assert!(matches!(
        supervise("panic", None, Duration::from_secs(10)),
        CaseOutcome::Crash(_)
    ));
    assert!(matches!(
        supervise("hang", None, Duration::from_millis(250)),
        CaseOutcome::Timeout(_)
    ));
}

pub fn assert_corpus_conformance() {
    let manifest = Manifest::load().unwrap_or_else(|error| panic!("{error}"));
    verify_integrity(&manifest).unwrap_or_else(|error| panic!("{error}"));
    let mut failures = Vec::new();
    let mut passed = 0;
    for page in &manifest.pages {
        for viewport in &manifest.viewports {
            for renderer in [Renderer::Terminal, Renderer::Vga] {
                let outcome = supervise_case(&page.slug, &viewport.id, renderer);
                match outcome {
                    CaseOutcome::Pass => passed += 1,
                    failure => failures.push(format!(
                        "{} {} {}: {failure:?}",
                        page.slug,
                        viewport.id,
                        renderer.name()
                    )),
                }
            }
        }
    }
    assert_eq!(passed + failures.len(), 36);
    assert!(
        failures.is_empty(),
        "browser corpus failures:\n{}",
        failures.join("\n")
    );
}

pub fn assert_narrow_rtl_completes() {
    for renderer in [Renderer::Terminal, Renderer::Vga] {
        match supervise_case("wikipedia-arabic-linux", "40x30", renderer) {
            CaseOutcome::Pass | CaseOutcome::AssertionMismatch(_) => {}
            failure => panic!("{}: {failure:?}", renderer.name()),
        }
    }
}

fn assert_manifest_error(value: Value, expected: &str) {
    let source = serde_json::to_string(&value).expect("serialize invalid manifest");
    let error = Manifest::parse(&source).expect_err("manifest should fail");
    assert!(
        error.contains(expected),
        "expected {expected:?} in {error:?}"
    );
}
