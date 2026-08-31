use std::ffi::OsString;
use std::fs::{self, File};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct WorkerSuccess {
    pub result: Vec<u8>,
    pub log: String,
}

pub enum WorkerFailure {
    Harness(String),
    Crash(String),
    Timeout(String),
}

pub fn supervise(
    test_name: &str,
    result_env: &str,
    environment: &[(&str, OsString)],
    timeout: Duration,
) -> Result<WorkerSuccess, WorkerFailure> {
    let directory =
        tempdir().map_err(|error| WorkerFailure::Harness(format!("worker tempdir: {error}")))?;
    let result_path = directory.path().join("result.json");
    let stdout_path = directory.path().join("stdout.log");
    let stderr_path = directory.path().join("stderr.log");
    let executable = std::env::current_exe()
        .map_err(|error| WorkerFailure::Harness(format!("worker executable: {error}")))?;
    let stdout = File::create(&stdout_path)
        .map_err(|error| WorkerFailure::Harness(format!("worker stdout: {error}")))?;
    let stderr = File::create(&stderr_path)
        .map_err(|error| WorkerFailure::Harness(format!("worker stderr: {error}")))?;
    let mut command = Command::new(executable);
    command
        .arg("--ignored")
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(result_env, &result_path)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (key, value) in environment {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .map_err(|error| WorkerFailure::Harness(format!("spawn worker: {error}")))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let log = worker_log(&stdout_path, &stderr_path);
                if !status.success() {
                    return Err(WorkerFailure::Crash(format!("worker exited {status}{log}")));
                }
                let result = fs::read(&result_path).map_err(|error| {
                    WorkerFailure::Harness(format!("worker produced no result: {error}{log}"))
                })?;
                return Ok(WorkerSuccess { result, log });
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let kill_error = child.kill().err();
                let wait_error = child.wait().err();
                let log = worker_log(&stdout_path, &stderr_path);
                return Err(WorkerFailure::Timeout(format!(
                    "worker exceeded {} ms; kill={kill_error:?}; wait={wait_error:?}{log}",
                    timeout.as_millis()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorkerFailure::Harness(format!("poll worker: {error}")));
            }
        }
    }
}

fn worker_log(stdout: &std::path::Path, stderr: &std::path::Path) -> String {
    let stdout = fs::read_to_string(stdout).unwrap_or_default();
    let stderr = fs::read_to_string(stderr).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    if combined.trim().is_empty() {
        String::new()
    } else {
        format!("\nworker log:\n{}", combined.trim())
    }
}
