mod support;

use std::collections::HashSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use sha2::{Digest, Sha256};
use support::dat::{DatCase, parse_file};
use support::xfail::Xfail;
use textsurf::core::dom::ElementNs;
use textsurf::html::{ElementContext, Html5everParser, HtmlParser, tree_dump};

const TESTDATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");
const PASS_FLOOR: f64 = 0.90;

#[derive(Default)]
struct FileStats {
    total: usize,
    skipped: usize,
    passed: usize,
    xfailed: usize,
    unexpected: Vec<(usize, String)>,
    bad_xfail: Vec<(usize, String)>,
}

enum CaseFailure {
    Mismatch(String),
    Panic(String),
}

fn run_case(case: &DatCase) -> Result<(), CaseFailure> {
    let context = ElementContext {
        name: case
            .context
            .as_ref()
            .map_or("", |(name, _)| name)
            .to_string(),
        ns: case.context.as_ref().map_or(ElementNs::Html, |(_, ns)| *ns),
        attrs: vec![],
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let parser = Html5everParser::new(false);
        if case.is_fragment {
            parser.parse_fragment(&case.input, &context)
        } else {
            parser.parse_document(&case.input)
        }
    }))
    .map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());
        CaseFailure::Panic(message)
    })?;
    let actual = tree_dump(&outcome.document.borrow(), case.is_fragment);
    if actual == case.expected {
        Ok(())
    } else {
        Err(CaseFailure::Mismatch(first_diff(&case.expected, &actual)))
    }
}

fn first_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let len = expected_lines.len().max(actual_lines.len());
    for i in 0..len {
        let want = expected_lines.get(i).copied().unwrap_or("(absent)");
        let got = actual_lines.get(i).copied().unwrap_or("(absent)");
        if want != got {
            return format!("line {}: expected `{want}`  got `{got}`", i + 1);
        }
    }
    "expected equals actual but diff not found".to_string()
}

fn run_file(file: &str, xfail: &Xfail, used: &mut HashSet<(String, usize)>) -> FileStats {
    let bytes = fs::read(Path::new(TESTDATA).join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    let cases = parse_file(file, &bytes);
    let mut stats = FileStats::default();
    for (i, case) in cases.iter().enumerate() {
        let number = i + 1;
        stats.total += 1;
        if case.script_on {
            stats.skipped += 1;
            continue;
        }
        let manifest_reason = xfail.reason(file, number).map(str::to_string);
        match run_case(case) {
            Ok(()) => {
                if manifest_reason.is_some() {
                    stats
                        .bad_xfail
                        .push((number, "passes but manifest expects a failure".to_string()));
                } else {
                    stats.passed += 1;
                }
            }
            Err(CaseFailure::Mismatch(detail)) => {
                if manifest_reason.is_some() {
                    used.insert((file.to_string(), number));
                    stats.xfailed += 1;
                } else {
                    stats.unexpected.push((number, detail));
                }
            }
            Err(CaseFailure::Panic(detail)) => stats
                .unexpected
                .push((number, format!("parser panicked: {detail}"))),
        }
    }
    stats
}

fn corpus_files() -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(TESTDATA)
        .expect("testdata directory")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            name.ends_with(".dat").then_some(name)
        })
        .collect();
    files.sort();
    files
}

#[test]
fn corpus_conformance() {
    let xfail = Xfail::load();
    let mut used: HashSet<(String, usize)> = HashSet::new();
    let mut grand_total = 0usize;
    let mut grand_skipped = 0usize;
    let mut grand_passed = 0usize;
    let mut grand_xfailed = 0usize;
    let mut all_unexpected: Vec<(String, usize, String)> = Vec::new();
    let mut all_bad: Vec<(String, usize, String)> = Vec::new();

    for file in corpus_files() {
        let stats = run_file(&file, &xfail, &mut used);
        grand_total += stats.total;
        grand_skipped += stats.skipped;
        grand_passed += stats.passed;
        grand_xfailed += stats.xfailed;
        for (number, detail) in &stats.unexpected {
            all_unexpected.push((file.clone(), *number, detail.clone()));
        }
        for (number, detail) in &stats.bad_xfail {
            all_bad.push((file.clone(), *number, detail.clone()));
        }
        println!(
            "{file}: total={} skipped={} passed={} xfailed={} unexpected={} first={:?}",
            stats.total,
            stats.skipped,
            stats.passed,
            stats.xfailed,
            stats.unexpected.len(),
            stats.unexpected.first()
        );
    }

    let run = grand_total - grand_skipped;
    let rate = if run == 0 {
        1.0
    } else {
        grand_passed as f64 / run as f64
    };
    println!(
        "corpus: run={run} passed={grand_passed} xfailed={grand_xfailed} unexpected={} pass-rate={rate:.4}",
        all_unexpected.len()
    );

    let stale: Vec<(&str, usize)> = xfail
        .keys()
        .filter(|(file, number)| !used.contains(&(file.to_string(), *number)))
        .collect();

    assert!(
        all_unexpected.is_empty(),
        "unexpected failures:\n{all_unexpected:?}"
    );
    assert!(
        all_bad.is_empty(),
        "manifest entries that now pass:\n{all_bad:?}"
    );
    assert!(
        stale.is_empty(),
        "manifest entries with no failing case (stale): {stale:?}"
    );
    assert!(
        rate >= PASS_FLOOR,
        "corpus pass rate {rate:.4} below floor {PASS_FLOOR}"
    );
}

#[test]
fn manifest_references_only_vendored_files() {
    let xfail = Xfail::load();
    let vendored: HashSet<String> = corpus_files().into_iter().collect();
    let unknown: Vec<&str> = xfail
        .keys()
        .map(|(file, _)| file)
        .filter(|file| !vendored.contains(*file))
        .collect();
    assert!(
        unknown.is_empty(),
        "manifest references unknown files: {unknown:?}"
    );
}

#[test]
fn vendored_files_exactly_match_the_sha256_manifest() {
    let manifest =
        fs::read_to_string(Path::new(TESTDATA).join("wpt-parsing.sha256")).expect("hash manifest");
    let mut expected = HashSet::new();
    for (line_number, line) in manifest.lines().enumerate() {
        let mut fields = line.split_whitespace();
        let hash = fields.next().expect("manifest hash");
        let file = fields.next().expect("manifest file");
        assert!(
            fields.next().is_none(),
            "extra field on hash manifest line {}",
            line_number + 1
        );
        assert!(
            expected.insert(file.to_string()),
            "duplicate hash entry for {file}"
        );
        let bytes = fs::read(Path::new(TESTDATA).join(file)).expect("vendored corpus file");
        let actual = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        assert_eq!(actual, hash, "hash mismatch for {file}");
    }
    let actual: HashSet<String> = corpus_files().into_iter().collect();
    assert_eq!(
        actual, expected,
        "hash manifest membership differs from corpus files"
    );
}
