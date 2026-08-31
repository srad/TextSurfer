use std::ffi::OsString;
use std::fs;
use std::hint;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::worker_supervisor::{WorkerFailure, supervise as supervise_worker};

use super::compare::compare_case;
use super::manifest::{Manifest, Renderer};
use super::render::render_case;

const CORPUS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum CaseOutcome {
    Pass,
    AssertionMismatch(String),
    HarnessError(String),
    Crash(String),
    Timeout(String),
}

pub fn supervise_case(slug: &str, viewport: &str, renderer: Renderer) -> CaseOutcome {
    let case = format!("{slug}|{viewport}|{}", renderer.name());
    supervise("case", Some(&case), CORPUS_TIMEOUT)
}

pub fn supervise(mode: &str, case: Option<&str>, timeout: Duration) -> CaseOutcome {
    let mut environment = vec![("TEXTSURFER_BROWSER_WORKER_MODE", OsString::from(mode))];
    if let Some(case) = case {
        environment.push(("TEXTSURFER_BROWSER_CASE", OsString::from(case)));
    }
    match supervise_worker(
        "browser_corpus_worker",
        "TEXTSURFER_BROWSER_RESULT",
        &environment,
        timeout,
    ) {
        Ok(success) => {
            if std::env::var_os("TEXTSURFER_CORPUS_TRIAGE").is_some()
                && !success.log.trim().is_empty()
            {
                eprintln!("{}", success.log);
            }
            serde_json::from_slice(&success.result).unwrap_or_else(|error| {
                CaseOutcome::HarnessError(format!("worker result JSON: {error}{}", success.log))
            })
        }
        Err(WorkerFailure::Harness(error)) => CaseOutcome::HarnessError(error),
        Err(WorkerFailure::Crash(error)) => CaseOutcome::Crash(error),
        Err(WorkerFailure::Timeout(error)) => CaseOutcome::Timeout(error),
    }
}

pub fn run_worker(mode: &str, result_path: &Path) {
    match mode {
        "case" => write_result(result_path, &evaluate_selected_case()),
        "pass" => write_result(result_path, &CaseOutcome::Pass),
        "mismatch" => write_result(
            result_path,
            &CaseOutcome::AssertionMismatch("synthetic mismatch".to_string()),
        ),
        "malformed" => fs::write(result_path, b"not JSON").expect("write malformed result"),
        "empty" => {}
        "panic" => panic!("synthetic browser-corpus worker panic"),
        "hang" => loop {
            hint::spin_loop();
        },
        other => write_result(
            result_path,
            &CaseOutcome::HarnessError(format!("unknown worker mode {other}")),
        ),
    }
}

fn evaluate_selected_case() -> CaseOutcome {
    let selected = match std::env::var("TEXTSURFER_BROWSER_CASE") {
        Ok(selected) => selected,
        Err(error) => return CaseOutcome::HarnessError(format!("worker case: {error}")),
    };
    let mut parts = selected.split('|');
    let (Some(slug), Some(viewport_id), Some(renderer), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return CaseOutcome::HarnessError(format!("malformed worker case {selected}"));
    };
    let renderer = match renderer {
        "terminal" => Renderer::Terminal,
        "vga" => Renderer::Vga,
        _ => return CaseOutcome::HarnessError(format!("unknown renderer {renderer}")),
    };
    let manifest = match Manifest::load() {
        Ok(manifest) => manifest,
        Err(error) => return CaseOutcome::HarnessError(error),
    };
    let Some(page) = manifest.page(slug) else {
        return CaseOutcome::HarnessError(format!("unknown page {slug}"));
    };
    let Some(viewport) = manifest.viewport(viewport_id) else {
        return CaseOutcome::HarnessError(format!("unknown viewport {viewport_id}"));
    };
    let Some(expectation) = manifest.expectation(page, renderer, viewport_id) else {
        return CaseOutcome::HarnessError(format!("missing expectation for {selected}"));
    };
    let rendered = match render_case(&manifest, page, viewport, renderer) {
        Ok(rendered) => rendered,
        Err(error) => return CaseOutcome::AssertionMismatch(error),
    };
    match compare_case(&rendered.reference, &rendered.tree, expectation) {
        Ok(summary) => {
            println!(
                "{slug} {viewport_id} {}: browser={} ours={} A1-missing={} A1-leaked={} A2-order={}/{} A3-overlap=0",
                renderer.name(),
                summary.browser_tokens,
                summary.our_tokens,
                summary.missing,
                summary.leaked,
                summary.order_matched,
                summary.order_total
            );
            CaseOutcome::Pass
        }
        Err(error) => CaseOutcome::AssertionMismatch(format!(
            "{slug} {viewport_id} {}:\n{error}",
            renderer.name()
        )),
    }
}

fn write_result(path: &Path, outcome: &CaseOutcome) {
    let bytes = serde_json::to_vec(outcome).expect("serialize worker result");
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).expect("write worker result");
    fs::rename(temporary, path).expect("activate worker result");
}
