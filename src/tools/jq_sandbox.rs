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
//!
//! Worker slots are limited globally ([`MAX_WORKERS`]) and per caller
//! ([`MAX_WORKERS_PER_CALLER`]). A caller is an account only when the tool's
//! upstream call accepted its token in this very request; everything else —
//! forged or opaque tokens on tools that never send the token upstream (`now`,
//! `fx_rate`, `get_me`, ...), and token-less calls — shares the single
//! [`UNVERIFIED_CALLER`] bucket. Forged tokens therefore hold at most one
//! worker (one CPU core) no matter how many there are, and every verified
//! account keeps a slot. No state outlives a request.

use std::collections::HashMap;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, SemaphorePermit};
use tokio::time::Instant;

use crate::tools::jq;

/// First command-line argument that turns the executable into a jq worker.
pub(crate) const WORKER_ARG: &str = "__jq-worker";
/// Wall-clock budget for running one filter. Real filters take milliseconds;
/// a short budget limits how long a spinning one holds a core.
const FILTER_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a call waits for a free worker slot before giving up with
/// [`SandboxError::Busy`].
const SLOT_TIMEOUT: Duration = Duration::from_secs(3);
/// Concurrent workers. Each can spin one core until [`FILTER_TIMEOUT`], and
/// workers share the server's cgroup quota (2 cores in production), so this
/// is sized against CPU: the unverified bucket gets at most one of them,
/// leaving the server at least a core under any forged-token flood. Memory
/// worst case is 2 x [`WORKER_ADDRESS_SPACE`] = 512 MiB of the 2000Mi limit.
///
/// Unit tests run concurrently and some hold slots on purpose, so they get
/// more global slots; the per-caller limits are what they exercise.
const MAX_WORKERS: usize = if cfg!(test) { 64 } else { 2 };
/// Worker slots one caller — one verified account, or the whole unverified
/// bucket — may hold at once.
const MAX_WORKERS_PER_CALLER: usize = 1;
/// Address-space limit of a worker. Responses are at most a few MiB, so a
/// legitimate filter stays far below it; a runaway aggregate fails allocation
/// and aborts the worker instead of growing until the pod is OOM-killed.
const WORKER_ADDRESS_SPACE: u64 = 256 * 1024 * 1024;
/// CPU-seconds backstop in case the parent dies before killing the worker.
const WORKER_CPU_SECONDS: u64 = 3;

static WORKERS: Semaphore = Semaphore::const_new(MAX_WORKERS);
/// Per-caller slot semaphores, keyed by [`caller_key`]. An entry is removed
/// when its last slot is released and nobody is waiting on it.
static CALLERS: LazyLock<Mutex<HashMap<String, Arc<Semaphore>>>> = LazyLock::new(Default::default);

/// Bucket shared by every call whose account is not proven: no token, or a
/// token that no upstream call accepted during the request.
pub(crate) const UNVERIFIED_CALLER: &str = "unverified";

/// Caller identity for per-caller limits: the account (`member_id` in the
/// token's `sub` claim), so one user's many OAuth tokens share one limit;
/// the token itself for an opaque token. Hashed either way, so neither the
/// token nor the member id is kept.
///
/// The claim is read unverified (see `auth::jwt`), so the key alone proves
/// nothing: `jq::call` uses it only when the tool's upstream call succeeded
/// with this token, and [`UNVERIFIED_CALLER`] otherwise.
pub(crate) fn caller_key(token: &str) -> String {
    let member = crate::auth::jwt::subject(token).and_then(|sub| match &sub["member_id"] {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    });
    let identity = match member {
        Some(id) => format!("member:{id}"),
        None => format!("token:{token}"),
    };
    Sha256::digest(identity.as_bytes())[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A reserved worker slot. Held from reservation until the filter finishes.
pub(crate) struct Slot {
    _global: SemaphorePermit<'static>,
    _caller: CallerPermit,
}

struct CallerPermit {
    key: String,
    permit: Option<OwnedSemaphorePermit>,
}

impl Drop for CallerPermit {
    fn drop(&mut self) {
        drop(self.permit.take());
        let mut callers = CALLERS.lock().unwrap_or_else(PoisonError::into_inner);
        // Waiters clone the Arc under this lock, so a count of 1 means only
        // the map holds it: idle, and safe to forget.
        if callers
            .get(&self.key)
            .is_some_and(|semaphore| Arc::strong_count(semaphore) == 1)
        {
            callers.remove(&self.key);
        }
    }
}

/// Reserve a worker slot for `caller`, waiting at most [`SLOT_TIMEOUT`].
pub(crate) async fn acquire(caller: &str) -> Result<Slot, SandboxError> {
    acquire_until(caller, Instant::now() + SLOT_TIMEOUT).await
}

async fn acquire_until(caller: &str, deadline: Instant) -> Result<Slot, SandboxError> {
    let semaphore = CALLERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(caller.to_owned())
        .or_insert_with(|| Arc::new(Semaphore::new(MAX_WORKERS_PER_CALLER)))
        .clone();
    // Created before waiting so the map entry is cleaned up on timeout too;
    // `acquire_owned` consumes the clone, so the guard sees the true count.
    let mut caller_permit = CallerPermit {
        key: caller.to_owned(),
        permit: None,
    };
    caller_permit.permit = Some(
        tokio::time::timeout_at(deadline, semaphore.acquire_owned())
            .await
            .map_err(|_| SandboxError::Busy)?
            .map_err(|_| SandboxError::Busy)?,
    );
    let global = tokio::time::timeout_at(deadline, WORKERS.acquire())
        .await
        .map_err(|_| SandboxError::Busy)?
        .map_err(|_| SandboxError::Busy)?;
    Ok(Slot {
        _global: global,
        _caller: caller_permit,
    })
}

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
        "filter timed out after {}s; narrow the _jq expression (avoid unbounded ranges or repeats)",
        FILTER_TIMEOUT.as_secs()
    )]
    Timeout,
    /// No worker slot freed up in time (for this caller or globally).
    #[error("too many _jq filters are running; retry shortly")]
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

/// Evaluate `code` on `input` in a sandboxed worker process, in a slot
/// reserved with [`acquire`]; the slot is released when this returns.
pub(crate) async fn run(slot: Slot, code: &str, input: Value) -> Result<Reply, SandboxError> {
    run_with_timeout(slot, code, input, FILTER_TIMEOUT).await
}

async fn run_with_timeout(
    _slot: Slot,
    code: &str,
    input: Value,
    timeout: Duration,
) -> Result<Reply, SandboxError> {
    let deadline = Instant::now() + timeout;
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

    async fn run_as_test(code: &str, input: Value) -> Result<Reply, SandboxError> {
        run(acquire("test").await.unwrap(), code, input).await
    }

    #[tokio::test]
    async fn a_filter_round_trips_through_the_worker() {
        let reply = run_as_test(".a, .b", json!({"a": 1, "b": [true]}))
            .await
            .unwrap();
        assert_eq!(reply, Reply::Values(vec![json!(1), json!([true])]));
    }

    #[tokio::test]
    async fn unbounded_recursion_aborts_the_worker_not_the_server() {
        // In-process this overflowed the stack and SIGABRTed the server.
        let error = run_as_test("def f: 1 + f; f", json!(null))
            .await
            .unwrap_err();
        assert!(matches!(error, SandboxError::Aborted(_)), "{error}");
    }

    #[tokio::test]
    async fn a_spinning_filter_is_killed_at_the_deadline() {
        let slot = acquire("test").await.unwrap();
        let error = run_with_timeout(
            slot,
            "last(repeat(1))",
            json!(null),
            Duration::from_millis(500),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, SandboxError::Timeout), "{error}");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn memory_exhaustion_aborts_the_worker_not_the_server() {
        let error = run_as_test("[range(1e10)] | length", json!(null))
            .await
            .unwrap_err();
        assert!(matches!(error, SandboxError::Aborted(_)), "{error}");
    }

    /// The biggest responses are a few MiB; a filter over ~8 MiB of JSON must
    /// fit the worker's address-space limit (enforced on Linux).
    #[tokio::test]
    async fn an_eight_mib_response_fits_the_worker_limits() {
        let rows: Vec<Value> = (0..20_000)
            .map(|i| {
                json!({
                    "symbol": format!("{i:05}.US"),
                    "name": "x".repeat(400),
                    "price": i as f64 / 7.0,
                    "tags": ["a", "b", "c"],
                })
            })
            .collect();
        let input = json!({"data": rows});
        assert!(input.to_string().len() > 8 * 1024 * 1024);
        let reply = run_as_test(
            "([.data[] | select(.price > 100) | {symbol, price}] | length), (.data | sort_by(-.price) | .[0].symbol)",
            input,
        )
        .await
        .unwrap();
        assert_eq!(reply, Reply::Values(vec![json!(19_299), json!("19999.US")]));
    }

    #[tokio::test]
    async fn one_caller_cannot_take_every_worker_slot() {
        let caller = "per-caller-limit-test";
        let first = acquire(caller).await.unwrap();
        let second = acquire_until(caller, Instant::now() + Duration::from_millis(200)).await;
        assert!(matches!(second, Err(SandboxError::Busy)));
        // Another caller is unaffected.
        let other = acquire("per-caller-limit-other").await.unwrap();
        drop((first, other));
        assert!(
            !CALLERS.lock().unwrap().contains_key(caller),
            "idle callers must not accumulate"
        );
        acquire(caller).await.unwrap();
    }

    #[test]
    fn callers_are_keyed_by_account_not_by_token() {
        use crate::auth::jwt::test_token;
        let token = |member: &str, jti: &str| {
            let sub = json!({"member_id": member, "account_no": "x"}).to_string();
            test_token(&json!({"sub": sub, "jti": jti}).to_string())
        };
        assert_eq!(caller_key(&token("1", "a")), caller_key(&token("1", "b")));
        assert_ne!(caller_key(&token("1", "a")), caller_key(&token("2", "a")));
        assert_ne!(caller_key("opaque-a"), caller_key("opaque-b"));
        assert_eq!(caller_key(&token("1", "a")).len(), 16);
    }
}
