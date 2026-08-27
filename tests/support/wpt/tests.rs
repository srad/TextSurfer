use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use textsurfer::core::style::{CellStyle, RenderMetrics, Rgb};
use textsurfer::paint::{DisplayList, PaintedRow, PaintedSpan};
use textsurfer::ui::PAPER_WHITE;

use super::manifest::{
    CaseKind, CaseOutcome, ExpectedStatus, Manifest, OracleProfile, Relation, corpus_root,
    discover_metadata, manifest_path,
};
use super::render::visually_equal;
use super::supervisor::{supervise, supervise_case};

pub fn assert_manifest_contracts() {
    let manifest = Manifest::load().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(manifest.cases.len(), 30);
    assert_eq!(
        manifest.runnable_cases(OracleProfile::TerminalCellV1).len(),
        5
    );
    assert_eq!(manifest.runnable_cases(OracleProfile::VgaPixelV1).len(), 3);
    assert_eq!(
        manifest
            .cases
            .iter()
            .filter(|case| case.status == ExpectedStatus::Skip)
            .count(),
        22
    );
    assert_eq!(
        manifest
            .cases
            .iter()
            .filter(|case| case.path.starts_with("css/css-ui/box-sizing-")
                && case.path.ends_with(".html"))
            .count(),
        24
    );
    assert_eq!(
        manifest
            .cases
            .iter()
            .filter(
                |case| case.path.starts_with("css/css-sizing/box-sizing-replaced-")
                    && case.path.ends_with(".xht")
            )
            .count(),
        3
    );
    assert_eq!(PAPER_WHITE.palette().text, Rgb::new(16, 16, 16));
    assert_eq!(PAPER_WHITE.palette().background, Rgb::new(232, 228, 216));
    assert_eq!(PAPER_WHITE.palette().link, Rgb::new(0, 71, 171));
    assert_eq!(PAPER_WHITE.palette().link_hover, Rgb::new(139, 26, 26));
    assert_eq!(RenderMetrics::TERMINAL.cell.column_px(), 8);
    assert_eq!(RenderMetrics::TERMINAL.cell.row_px(), 16);
    assert_eq!(RenderMetrics::TERMINAL.cell.root_font_px(), 16);

    let source = fs::read_to_string(manifest_path()).expect("manifest source");
    let value: Value = serde_json::from_str(&source).expect("manifest value");

    let mut unknown = value.clone();
    unknown["unknown"] = json!(true);
    assert_manifest_error(unknown, "unknown field");

    let mut duplicate = value.clone();
    let repeated = duplicate["cases"][0].clone();
    duplicate["cases"].as_array_mut().unwrap().push(repeated);
    assert_manifest_error(duplicate, "duplicate case");

    let mut traversal = value.clone();
    traversal["cases"][0]["path"] = json!("../escape.html");
    assert_manifest_error(traversal, "unsafe corpus path");

    let mut crash_xfail = value.clone();
    let crash = crash_xfail["cases"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|case| case["kind"] == "crashtest")
        .unwrap();
    crash["status"] = json!("xfail");
    crash["reason"] = json!("not allowed");
    assert_manifest_error(crash_xfail, "cannot xfail");

    let mut cycle = value;
    let mut left = cycle["cases"][1].clone();
    let mut right = cycle["cases"][2].clone();
    left["path"] = json!("cycle/a.html");
    left["references"] = json!([{ "relation": "match", "path": "cycle/b.html" }]);
    right["path"] = json!("cycle/b.html");
    right["references"] = json!([{ "relation": "match", "path": "cycle/a.html" }]);
    cycle["cases"] = json!([left, right]);
    assert_manifest_error(cycle, "reference cycle");

    let metadata = discover_metadata(
        "css/example/test.html",
        "<link rel='match' href='../same.html'><link rel='mismatch' href='/different.html'><meta name='fuzzy'><script></script><main class='reftest-wait'></main>",
    )
    .expect("synthetic metadata");
    assert_eq!(
        metadata
            .references
            .iter()
            .map(|reference| (reference.relation, reference.path.as_str()))
            .collect::<Vec<_>>(),
        [
            (Relation::Match, "css/same.html"),
            (Relation::Mismatch, "different.html")
        ]
    );
    assert!(metadata.has_fuzzy);
    assert!(metadata.has_script);
    assert!(metadata.has_reftest_wait);

    assert_visual_oracle_contracts();
    assert_hash_contracts();
}

pub fn assert_corpus_integrity() {
    let manifest = Manifest::load().unwrap_or_else(|error| panic!("{error}"));
    verify_hashes(&corpus_root(), &manifest.vendored_files())
        .unwrap_or_else(|error| panic!("{error}"));
    let license = fs::read_to_string(corpus_root().join("LICENSE.md")).expect("WPT license");
    assert!(license.contains("Redistribution and use in source and binary forms"));
    for file in manifest.vendored_files() {
        if !file.ends_with(".html") && !file.ends_with(".xht") {
            continue;
        }
        let source = fs::read_to_string(corpus_root().join(&file))
            .unwrap_or_else(|error| panic!("{file}: {error}"));
        let metadata = discover_metadata(&file, &source)
            .unwrap_or_else(|error| panic!("metadata {file}: {error}"));
        assert!(!metadata.has_script, "{file} contains script");
        assert!(!metadata.has_reftest_wait, "{file} uses reftest-wait");
        assert!(!metadata.has_fuzzy, "{file} uses fuzzy metadata");
        if let Some(case) = manifest.case(&file) {
            match case.kind {
                CaseKind::Reftest => {
                    let mut expected = case.references.clone();
                    expected.sort();
                    assert_eq!(
                        metadata.references, expected,
                        "reference metadata for {file}"
                    );
                }
                CaseKind::Crashtest => {
                    assert!(
                        metadata.references.is_empty(),
                        "crashtest {file} has references"
                    );
                }
                CaseKind::Testharness => {}
            }
        }
    }
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
    assert_profile_conformance(
        OracleProfile::TerminalCellV1,
        "terminal-cell",
        (27, 5, 5, 0, 22),
    );
}

#[cfg(feature = "vga")]
pub fn assert_vga_pixel_conformance() {
    // Every admitted case is an expected failure, which is the tripwire this profile is for: the
    // pixels are compared exactly, and the day cell-quantised replaced sizing stops drifting from
    // the reference these turn into `unexpected` passes rather than passing silently.
    assert_profile_conformance(OracleProfile::VgaPixelV1, "vga-pixel", (3, 3, 0, 3, 0));
}

fn assert_profile_conformance(
    profile: OracleProfile,
    label: &str,
    expected: (usize, usize, usize, usize, usize),
) {
    let manifest = Manifest::load().unwrap_or_else(|error| panic!("{error}"));
    let mut passed = 0usize;
    let mut xfailed = 0usize;
    let mut unexpected = 0usize;
    let mut crashes = 0usize;
    let mut timeouts = 0usize;
    let mut harness_errors = 0usize;
    let mut failures = Vec::new();
    for case in manifest.runnable_cases(profile) {
        let outcome = supervise_case(&case.path);
        match (&case.status, &outcome) {
            (ExpectedStatus::Run, CaseOutcome::Pass) => passed += 1,
            (ExpectedStatus::Xfail, CaseOutcome::AssertionMismatch(_)) => xfailed += 1,
            (ExpectedStatus::Xfail, CaseOutcome::Pass)
            | (ExpectedStatus::Run, CaseOutcome::AssertionMismatch(_)) => {
                unexpected += 1;
                failures.push(format!("{}: {outcome:?}", case.path));
            }
            (_, CaseOutcome::Crash(_)) => {
                crashes += 1;
                failures.push(format!("{}: {outcome:?}", case.path));
            }
            (_, CaseOutcome::Timeout(_)) => {
                timeouts += 1;
                failures.push(format!("{}: {outcome:?}", case.path));
            }
            (_, CaseOutcome::HarnessError(_)) => {
                harness_errors += 1;
                failures.push(format!("{}: {outcome:?}", case.path));
            }
            (ExpectedStatus::Skip, _) => unreachable!("skips are not runnable"),
        }
    }
    let audited = manifest
        .cases
        .iter()
        .filter(|case| case.profile == profile)
        .count();
    let eligible = manifest.runnable_cases(profile).len();
    let skipped = manifest
        .cases
        .iter()
        .filter(|case| case.profile == profile && case.status == ExpectedStatus::Skip)
        .count();
    eprintln!(
        "WPT {label} slice: audited={audited} eligible={eligible} run={eligible} pass={passed} xfail={xfailed} skip={skipped} unexpected={unexpected} crash={crashes} timeout={timeouts} harness={harness_errors}"
    );
    assert!(
        failures.is_empty(),
        "WPT {label} failures:\n{}",
        failures.join("\n")
    );
    assert_eq!((audited, eligible, passed, xfailed, skipped), expected);
}

fn assert_manifest_error(value: Value, expected: &str) {
    let source = serde_json::to_string(&value).expect("invalid manifest JSON");
    let error = Manifest::parse(&source).expect_err("manifest should fail");
    assert!(
        error.contains(expected),
        "expected {expected:?} in {error:?}"
    );
}

fn assert_visual_oracle_contracts() {
    let default = DisplayList::from_lines(&["A".to_string()]);
    let palette = PAPER_WHITE.palette();
    let explicit = DisplayList {
        rows: vec![PaintedRow {
            spans: vec![PaintedSpan {
                col: 0,
                text: "A".to_string(),
                style: CellStyle {
                    fg: Some(palette.text.into()),
                    bg: Some(palette.background),
                    ..Default::default()
                },
            }],
        }],
        ..Default::default()
    };
    assert!(visually_equal(&default, &explicit, 4, 2).expect("visual equality"));
    let different = DisplayList::from_lines(&["B".to_string()]);
    assert!(!visually_equal(&default, &different, 4, 2).expect("visual difference"));
}

fn assert_hash_contracts() {
    let directory = tempdir().expect("hash tempdir");
    fs::write(directory.path().join("a.txt"), b"original").expect("hash fixture");
    let hash = hash_bytes(b"original");
    fs::write(
        directory.path().join("SHA256SUMS"),
        format!("{hash}  a.txt\n"),
    )
    .expect("hash manifest");
    let expected = BTreeSet::from(["a.txt".to_string()]);
    verify_hashes(directory.path(), &expected).expect("valid hash corpus");
    fs::write(directory.path().join("a.txt"), b"changed").expect("hash drift");
    assert!(
        verify_hashes(directory.path(), &expected)
            .unwrap_err()
            .contains("hash drift")
    );
    let missing = BTreeSet::from(["a.txt".to_string(), "b.txt".to_string()]);
    assert!(
        verify_hashes(directory.path(), &missing)
            .unwrap_err()
            .contains("membership")
    );
}

fn verify_hashes(root: &Path, expected: &BTreeSet<String>) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    collect_files(root, root, &mut actual)?;
    actual.remove("SHA256SUMS");
    if &actual != expected {
        return Err(format!(
            "corpus membership differs: expected={expected:?} actual={actual:?}"
        ));
    }
    let hash_path = root.join("SHA256SUMS");
    let source = fs::read_to_string(&hash_path)
        .map_err(|error| format!("hash manifest {}: {error}", hash_path.display()))?;
    let mut hashes = BTreeMap::new();
    for (line_number, line) in source.lines().enumerate() {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| format!("hash manifest line {} is malformed", line_number + 1))?;
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "hash manifest line {} has a bad hash",
                line_number + 1
            ));
        }
        super::manifest::validate_path(path)?;
        if hashes
            .insert(path.to_string(), hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(format!("hash manifest repeats {path}"));
        }
    }
    if hashes.keys().cloned().collect::<BTreeSet<_>>() != *expected {
        return Err("hash manifest membership differs from the corpus".to_string());
    }
    for (path, expected_hash) in hashes {
        let bytes = fs::read(root.join(&path)).map_err(|error| format!("{path}: {error}"))?;
        let actual_hash = hash_bytes(&bytes);
        if actual_hash != expected_hash {
            return Err(format!(
                "hash drift for {path}: {actual_hash} != {expected_hash}"
            ));
        }
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("corpus directory {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("corpus entry: {error}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("corpus metadata {}: {error}", entry.path().display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "corpus symlink is forbidden: {}",
                entry.path().display()
            ));
        }
        if metadata.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if metadata.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| format!("corpus path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative);
        }
    }
    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
