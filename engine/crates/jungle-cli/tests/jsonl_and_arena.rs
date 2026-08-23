use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct EngineProcess {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl EngineProcess {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_jungle-cli"))
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start JSONL engine");
        let input = BufWriter::new(child.stdin.take().expect("stdin"));
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            input,
            output,
        }
    }

    fn request(&mut self, value: Value) -> Value {
        serde_json::to_writer(&mut self.input, &value).expect("serialize request");
        self.input.write_all(b"\n").expect("request newline");
        self.input.flush().expect("flush request");
        let mut line = String::new();
        self.output.read_line(&mut line).expect("read response");
        assert!(!line.is_empty(), "engine exited before responding");
        serde_json::from_str(&line).expect("JSON response")
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn jsonl_session_reports_strict_errors_and_structured_search() {
    let mut engine = EngineProcess::start();
    let hello = engine.request(json!({"type":"hello","id":1}));
    assert_eq!(hello["id"], 1);
    assert_eq!(hello["ok"], true);
    assert_eq!(hello["data"]["protocol"], "jungle-jsonl");

    let new_game = engine.request(json!({"type":"newGame","id":2,"seed":7}));
    assert_eq!(new_game["ok"], true);
    assert_eq!(
        new_game["data"]["state"]["board"].as_array().map(Vec::len),
        Some(63)
    );
    let restored = engine.request(json!({
        "type":"setPosition",
        "id":20,
        "state":new_game["data"]["state"].clone()
    }));
    assert_eq!(restored["ok"], true);
    assert_eq!(restored["data"]["state"], new_game["data"]["state"]);

    let illegal = engine.request(json!({
        "type":"applyMove",
        "id":3,
        "move":{"origin":42,"destination":0}
    }));
    assert_eq!(illegal["ok"], false);
    assert_eq!(illegal["error"]["code"], "ILLEGAL_MOVE");

    let search = engine.request(json!({
        "type":"chooseMove",
        "id":4,
        "limits":{"kind":"fixedNodes","nodes":100}
    }));
    assert_eq!(search["ok"], true);
    assert!(search["data"]["move"]["origin"].is_number());
    assert!(
        search["data"]["search"]["nodes"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 100
    );
    assert!(search["data"]["search"]["diagnostics"].is_object());

    let timed = engine.request(json!({
        "type":"chooseMove",
        "id":40,
        "limits":{"kind":"exactTime","timeMs":50}
    }));
    assert_eq!(timed["ok"], true);
    assert!(timed["data"]["search"]["elapsedMs"].is_number());

    let applied = engine.request(json!({
        "type":"applyMove",
        "id":5,
        "move":search["data"]["move"].clone()
    }));
    assert_eq!(applied["ok"], true);
    assert_eq!(
        applied["data"]["state"]["move_history"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let close = engine.request(json!({"type":"close","id":6}));
    assert_eq!(close["ok"], true);
}

#[test]
fn subprocess_arena_runs_a_complete_color_swapped_smoke_pair() {
    let executable = env!("CARGO_BIN_EXE_jungle-cli");
    let output = Command::new(executable)
        .args([
            "arena",
            "--candidate",
            executable,
            "--candidate-arg",
            "serve",
            "--champion",
            executable,
            "--champion-arg",
            "serve",
            "--smoke",
            "--nodes",
            "1",
            "--bootstrap-samples",
            "100",
        ])
        .output()
        .expect("run arena");
    assert!(
        output.status.success(),
        "arena failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).expect("arena summary");
    assert_eq!(summary["schema"], "jungle-arena-result");
    assert_eq!(summary["pairs"], 1);
    assert_eq!(summary["games"], 2);
    assert_eq!(summary["plyCap"], 256);
    assert_eq!(summary["strictFailures"], 0);
    assert_eq!(summary["promotion"]["ready"], false);
}
