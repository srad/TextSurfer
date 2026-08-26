use std::env;
use std::fs::{self, File};
use std::hint;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

use super::manifest::{CaseOutcome, Manifest};
use super::render::evaluate_case;

const CORPUS_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub fn supervise_case(path: &str) -> CaseOutcome {
    supervise("case", Some(path), CORPUS_TIMEOUT)
}

pub fn supervise(mode: &str, case: Option<&str>, timeout: Duration) -> CaseOutcome {
    let directory = match tempdir() {
        Ok(directory) => directory,
        Err(error) => return CaseOutcome::HarnessError(format!("worker tempdir: {error}")),
    };
    let result_path = directory.path().join("result.json");
    let stdout_path = directory.path().join("stdout.log");
    let stderr_path = directory.path().join("stderr.log");
    let executable = match env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return CaseOutcome::HarnessError(format!("worker executable: {error}")),
    };
    let stdout = match File::create(&stdout_path) {
        Ok(file) => file,
        Err(error) => return CaseOutcome::HarnessError(format!("worker stdout: {error}")),
    };
    let stderr = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => return CaseOutcome::HarnessError(format!("worker stderr: {error}")),
    };
    let mut command = Command::new(executable);
    command
        .arg("--ignored")
        .arg("--exact")
        .arg("wpt_case_worker")
        .arg("--nocapture")
        .env("TEXTSURFER_WPT_WORKER_MODE", mode)
        .env("TEXTSURFER_WPT_RESULT", &result_path)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(case) = case {
        command.env("TEXTSURFER_WPT_CASE", case);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return CaseOutcome::HarnessError(format!("spawn worker: {error}")),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let log = worker_log(&stdout_path, &stderr_path);
                if !status.success() {
                    return CaseOutcome::Crash(format!("worker exited {status}{log}"));
                }
                let bytes = match fs::read(&result_path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return CaseOutcome::HarnessError(format!(
                            "worker produced no result: {error}{log}"
                        ));
                    }
                };
                return serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                    CaseOutcome::HarnessError(format!("worker result JSON: {error}{log}"))
                });
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let kill_error = child.kill().err();
                let wait_error = child.wait().err();
                let log = worker_log(&stdout_path, &stderr_path);
                return CaseOutcome::Timeout(format!(
                    "worker exceeded {} ms; kill={kill_error:?}; wait={wait_error:?}{log}",
                    timeout.as_millis()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return CaseOutcome::HarnessError(format!("poll worker: {error}"));
            }
        }
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
        other => write_result(
            result_path,
            &CaseOutcome::HarnessError(format!("unknown worker mode {other}")),
        ),
    }
}

fn write_result(path: &Path, outcome: &CaseOutcome) {
    let bytes = serde_json::to_vec(outcome).expect("serialize worker result");
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).expect("write worker result");
    fs::rename(temporary, path).expect("activate worker result");
}

fn worker_log(stdout: &Path, stderr: &Path) -> String {
    let stdout = fs::read_to_string(stdout).unwrap_or_default();
    let stderr = fs::read_to_string(stderr).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    if combined.trim().is_empty() {
        String::new()
    } else {
        format!("\nworker log:\n{}", combined.trim())
    }
}
