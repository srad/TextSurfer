use std::env;
use std::fs;
use std::hint;
use std::path::Path;
use std::time::Duration;

use super::manifest::{CaseOutcome, Manifest};
use super::render::evaluate_case;
use super::worker_supervisor::{WorkerFailure, supervise as supervise_worker};

const CORPUS_TIMEOUT: Duration = Duration::from_secs(10);

pub fn supervise_case(path: &str) -> CaseOutcome {
    supervise("case", Some(path), CORPUS_TIMEOUT)
}

pub fn supervise(mode: &str, case: Option<&str>, timeout: Duration) -> CaseOutcome {
    let mut environment = vec![("TEXTSURFER_WPT_WORKER_MODE", mode.into())];
    if let Some(case) = case {
        environment.push(("TEXTSURFER_WPT_CASE", case.into()));
    }
    match supervise_worker(
        "wpt_case_worker",
        "TEXTSURFER_WPT_RESULT",
        &environment,
        timeout,
    ) {
        Ok(success) => serde_json::from_slice(&success.result).unwrap_or_else(|error| {
            CaseOutcome::HarnessError(format!("worker result JSON: {error}{}", success.log))
        }),
        Err(WorkerFailure::Harness(error)) => CaseOutcome::HarnessError(error),
        Err(WorkerFailure::Crash(error)) => CaseOutcome::Crash(error),
        Err(WorkerFailure::Timeout(error)) => CaseOutcome::Timeout(error),
    }
}

pub fn run_worker(mode: &str, result_path: &Path) {
    match mode {
        "case" => {
            let outcome = match env::var("TEXTSURFER_WPT_CASE") {
                Ok(path) => match Manifest::load() {
                    Ok(manifest) => match manifest.case(&path) {
                        Some(case) => evaluate_case(&manifest, case),
                        None => CaseOutcome::HarnessError(format!("unknown case {path}")),
                    },
                    Err(error) => CaseOutcome::HarnessError(error),
                },
                Err(error) => CaseOutcome::HarnessError(format!("worker case: {error}")),
            };
            write_result(result_path, &outcome);
        }
        "pass" => write_result(result_path, &CaseOutcome::Pass),
        "mismatch" => write_result(
            result_path,
            &CaseOutcome::AssertionMismatch("synthetic mismatch".to_string()),
        ),
        "empty" => {}
        "panic" => panic!("synthetic worker panic"),
        "hang" => loop {
            hint::spin_loop();
        },
        "dom-construction" => super::dom_construction::run_worker(result_path),
        other => write_result(
            result_path,
            &CaseOutcome::HarnessError(format!("unknown worker mode {other}")),
        ),
    }
}

pub(super) fn write_result(path: &Path, outcome: &CaseOutcome) {
    let bytes = serde_json::to_vec(outcome).expect("serialize worker result");
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).expect("write worker result");
    fs::rename(temporary, path).expect("activate worker result");
}
