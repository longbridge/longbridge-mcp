//! Out-of-process evaluation of `_jq` filters.
//!
//! `_jq` is code written by an AI client, possibly under prompt injection, and
//! jaq has no step, memory or recursion limit. Run in-process, one filter could
//! abort the whole server: `def f: 1 + f; f` overflows the stack (SIGABRT),
//! `[range(1e9)]` exhausts memory, and a timed-out `spawn_blocking` thread
//! cannot be cancelled. So every filter runs in a short-lived child process —
//! this same executable started with [`WORKER_ARG`] — under an address-space
//! and CPU rlimit, with an empty environment, and is killed at the deadline.
//! A crash there only fails that one call.

use std::process::{ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::tools::jq;

/// First command-line argument that turns the executable into a jq worker.
pub(crate) const WORKER_ARG: &str = "__jq-worker";
/// Wall-clock budget for one filter, including waiting for a worker slot.
const FILTER_TIMEOUT: Duration = Duration::from_secs(5);
/// Concurrent workers. Together with [`WORKER_ADDRESS_SPACE`] this caps the
/// memory hostile filters can take at about 1 GiB, well inside the
/// production pod's 2000Mi limit, which the workers share with the server.
const MAX_WORKERS: usize = 4;
/// Address-space limit of a worker. Responses are at most a few MiB, so a
/// legitimate filter stays far below it; a runaway aggregate fails allocation
/// and aborts the worker instead of growing until the pod is OOM-killed.
const WORKER_ADDRESS_SPACE: u64 = 256 * 1024 * 1024;
/// CPU-seconds backstop in case the parent dies before killing the worker.
const WORKER_CPU_SECONDS: u64 = 10;

static WORKERS: Semaphore = Semaphore::const_new(MAX_WORKERS);

/// Request sent to a worker on stdin.
#[derive(Serialize, Deserialize)]
struct Request<C> {
    code: C,
    input: Value,
}

/// Outcome of evaluating a filter, written by the worker as one JSON line.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reply {
    /// All filter outputs, in order.
    Values(Vec<Value>),
    /// jq runtime error (e.g. `cannot index number with "a"`), already
    /// truncated; the caller appends the response shape.
    FilterError(String),
    /// Any other failure (output limit, halt, malformed input).
    Error(String),
}

/// Why a filter produced no [`Reply`].
#[derive(Debug, thiserror::Error)]
pub(crate) enum SandboxError {
    /// The deadline passed while the filter was running.
    #[error(
        "filter timed out after 5s; narrow the _jq expression (avoid unbounded ranges or repeats)"
    )]
    Timeout,
    /// Every worker slot stayed busy until the deadline.
    #[error("too many _jq filters are running on this server; retry shortly")]
    Busy,
    /// The worker died: stack overflow, allocation failure or CPU limit.
    #[error(
        "filter aborted ({0}): it exceeded the sandbox's memory, CPU or recursion limits; \
         narrow the _jq expression (avoid deep recursion and huge aggregates)"
    )]
    Aborted(String),
    /// The worker could not be started or answered with garbage.
    #[error("filter worker failed: {0}")]
    Worker(String),
}

impl From<std::io::Error> for SandboxError {
    fn from(error: std::io::Error) -> Self {
        Self::Worker(error.to_string())
    }
}

/// Whether this process was started as a jq worker.
pub(crate) fn is_worker_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == WORKER_ARG)
}

/// Worker process entry point: read one request from stdin, evaluate it,
/// write the reply to stdout and exit. Runs before any runtime, logger or
/// network setup, so a filter has nothing else to reach.
pub(crate) fn worker_main() -> ! {
    limit_resources();
    let reply = match serde_json::from_reader::<_, Request<String>>(std::io::stdin().lock()) {
        Ok(request) => jq::evaluate(&request.code, request.input),
        Err(error) => Reply::Error(format!("invalid worker request: {error}")),
    };
    let line = serde_json::to_string(&reply).expect("a worker reply must serialize");
    let mut stdout = std::io::stdout().lock();
    let written = std::io::Write::write_all(&mut stdout, format!("{line}\n").as_bytes())
        .and_then(|()| std::io::Write::flush(&mut stdout));
    std::process::exit(if written.is_ok() { 0 } else { 1 })
}

/// Evaluate `code` on `input` in a sandboxed worker process.
pub(crate) async fn run(code: &str, input: Value) -> Result<Reply, SandboxError> {
    run_with_timeout(code, input, FILTER_TIMEOUT).await
}

async fn run_with_timeout(
    code: &str,
    input: Value,
    timeout: Duration,
) -> Result<Reply, SandboxError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let _permit = tokio::time::timeout_at(deadline, WORKERS.acquire())
        .await
        .map_err(|_| SandboxError::Busy)?
        .map_err(|_| SandboxError::Busy)?;
    let request = serde_json::to_vec(&Request { code, input })
        .map_err(|error| SandboxError::Worker(error.to_string()))?;
    // Dropping `exchange` on timeout drops the child, and `kill_on_drop`
    // SIGKILLs it, so a spinning filter does not outlive its deadline.
    tokio::time::timeout_at(deadline, exchange(request))
        .await
        .map_err(|_| SandboxError::Timeout)?
}

async fn exchange(request: Vec<u8>) -> Result<Reply, SandboxError> {
    let mut child = worker_command()?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().expect("worker stdin is piped");
    // A worker that dies early closes its stdin; its exit status says why, so
    // a write error here is not the interesting failure.
    let _ = stdin.write_all(&request).await;
    drop(stdin);
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(SandboxError::Aborted(describe_exit(output.status)));
    }
    // The reply is the last line: in unit tests the worker runs inside the
    // test harness, which prints its own banner first.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| SandboxError::Worker("no reply".into()))?;
    serde_json::from_str(line).map_err(|error| SandboxError::Worker(error.to_string()))
}

fn worker_command() -> Result<Command, SandboxError> {
    static EXE: OnceLock<Result<std::path::PathBuf, String>> = OnceLock::new();
    let exe = EXE
        .get_or_init(|| std::env::current_exe().map_err(|error| error.to_string()))
        .clone()
        .map_err(SandboxError::Worker)?;
    let mut command = Command::new(exe);
    // The server environment holds credentials and upstream configuration;
    // the worker needs none of it.
    command.env_clear();
    #[cfg(not(test))]
    command.arg(WORKER_ARG);
    // The unit-test binary is the libtest harness, not this crate's `main`:
    // run the ignored `worker_entry` test, which calls `worker_main`.
    #[cfg(test)]
    command
        .args([
            "--exact",
            "tools::jq_sandbox::tests::worker_entry",
            "--include-ignored",
            "--nocapture",
            "--quiet",
            "--test-threads=1",
        ])
        .env(tests::WORKER_ENV, "1");
    Ok(command)
}

fn describe_exit(status: ExitStatus) -> String {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return format!("signal {signal}");
    }
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "unknown exit status".into(),
    }
}

/// Best effort: if a limit cannot be set (e.g. `RLIMIT_AS` on macOS), the
/// timeout and the worker isolation still apply.
fn limit_resources() {
    #[cfg(unix)]
    {
        use rlimit::{Resource, setrlimit};
        let _ = setrlimit(Resource::AS, WORKER_ADDRESS_SPACE, WORKER_ADDRESS_SPACE);
        let _ = setrlimit(Resource::CPU, WORKER_CPU_SECONDS, WORKER_CPU_SECONDS + 1);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// Set on the child process so that `worker_entry` acts as the worker.
    pub(crate) const WORKER_ENV: &str = "LONGBRIDGE_MCP_TEST_JQ_WORKER";

    #[test]
    #[ignore = "entry point of the jq worker process spawned by the sandbox tests"]
    fn worker_entry() {
        if std::env::var_os(WORKER_ENV).is_some() {
            worker_main();
        }
    }

    #[tokio::test]
    async fn a_filter_round_trips_through_the_worker() {
        let reply = run(".a, .b", json!({"a": 1, "b": [true]})).await.unwrap();
        assert_eq!(reply, Reply::Values(vec![json!(1), json!([true])]));
    }

    #[tokio::test]
    async fn unbounded_recursion_aborts_the_worker_not_the_server() {
        // In-process this overflowed the stack and SIGABRTed the server.
        let error = run("def f: 1 + f; f", json!(null)).await.unwrap_err();
        assert!(matches!(error, SandboxError::Aborted(_)), "{error}");
    }

    #[tokio::test]
    async fn a_spinning_filter_is_killed_at_the_deadline() {
        let error = run_with_timeout("last(repeat(1))", json!(null), Duration::from_millis(500))
            .await
            .unwrap_err();
        assert!(matches!(error, SandboxError::Timeout), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn memory_exhaustion_aborts_the_worker_not_the_server() {
        let error = run("[range(1e10)] | length", json!(null))
            .await
            .unwrap_err();
        assert!(matches!(error, SandboxError::Aborted(_)), "{error}");
    }
}
