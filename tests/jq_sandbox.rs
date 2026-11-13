//! End-to-end check of `_jq` sandboxing through the real executable.
//!
//! Unit tests run the jq worker inside the libtest harness, so they never
//! exercise `main`'s `__jq-worker` dispatch. This test starts the built binary
//! in `--stdio` mode and sends filters that used to kill the server, then
//! checks that each fails alone and the server keeps answering.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

/// Kills the server if the test hangs, so a regression fails instead of
/// stalling CI.
const WATCHDOG: Duration = Duration::from_secs(60);

struct Server {
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Server {
    fn start() -> Self {
        let config_dir = std::env::temp_dir().join("longbridge-mcp-jq-sandbox-test");
        let mut child = Command::new(env!("CARGO_BIN_EXE_longbridge-mcp"))
            .arg("--stdio")
            // Keep a developer's ~/.longbridge/mcp/config.json out of the test.
            .env("LONGBRIDGE_MCP_CONFIG_DIR", &config_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary must start");
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
        let child = Arc::new(Mutex::new(child));
        let watchdog = Arc::clone(&child);
        std::thread::spawn(move || {
            std::thread::sleep(WATCHDOG);
            let _ = watchdog.lock().expect("watchdog lock").kill();
        });
        let mut server = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        server.request(
            "initialize",
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "jq-sandbox-test", "version": "1"}
            }),
        );
        server.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        server
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").expect("the server must accept input");
        self.stdin.flush().expect("the server must accept input");
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("the server must answer");
        assert!(
            !line.is_empty(),
            "the server exited while handling {params}"
        );
        let response: Value = serde_json::from_str(&line).expect("a JSON-RPC response");
        assert_eq!(response["id"], id, "{response}");
        response
    }

    /// Call the credential-free `now` tool with `_jq`; return the text result
    /// and whether it is an error.
    fn now_with_jq(&mut self, jq: &str) -> (String, bool) {
        let response = self.request(
            "tools/call",
            json!({"name": "now", "arguments": {"_jq": jq}}),
        );
        let result = &response["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("text content expected: {response}"))
            .to_owned();
        (text, result["isError"] == true)
    }

    fn assert_alive(&mut self) {
        let (text, is_error) = self.now_with_jq("{kind: type}");
        assert!(!is_error, "{text}");
        assert_eq!(text, r#"{"kind":"string"}"#);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.lock().map(|mut child| child.kill());
    }
}

#[test]
fn filters_that_used_to_kill_the_server_fail_alone() {
    let mut server = Server::start();
    // The worker dispatch works at all: a normal filter round-trips.
    server.assert_alive();

    let deep = format!("{}.{}", "(".repeat(100_000), ")".repeat(100_000));
    let cases = [
        // Stack overflow in the worker (was: SIGABRT of the server).
        ("def f: 1 + f; f", "jq_filter_error"),
        // `halt_error` (was: `std::process::exit(1)`, the production incident).
        ("halt_error(1)", "disabled in _jq"),
        ("$ENV | keys", "disabled in _jq"),
        // Parser recursion during validation (was: SIGABRT of the server).
        (deep.as_str(), "jq_syntax_error"),
    ];
    for (jq, expected) in cases {
        let (text, is_error) = server.now_with_jq(jq);
        let shown: String = jq.chars().take(40).collect();
        assert!(is_error, "{shown}: {text}");
        assert!(text.contains(expected), "{shown}: {text}");
        server.assert_alive();
    }
}
