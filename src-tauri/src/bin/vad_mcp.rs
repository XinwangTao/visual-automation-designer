//! `vad-mcp` — a deliberately thin MCP stdio server.
//!
//! It exposes `vadctl` commands as MCP tools by spawning the CLI and returning
//! its JSON output. No automation logic lives here: the CLI and the agent see
//! exactly the same behaviour, evidence and exit codes.
//!
//! Protocol: JSON-RPC 2.0 over stdio, one message per line.
//! Supported methods: initialize, notifications/initialized, ping,
//! tools/list, tools/call, shutdown.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::Command;

use serde_json::{json, Value};

const SERVER_NAME: &str = "vad-mcp";
const SERVER_VERSION: &str = "1.0.0";
const PROTOCOL_VERSION: &str = "2024-11-05";

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                let response = error_response(Value::Null, -32700, &format!("parse error: {error}"));
                let _ = writeln!(stdout, "{}", response);
                let _ = stdout.flush();
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        // Notifications carry no id and must not receive a response.
        let is_notification = request.get("id").is_none();

        let response = match method {
            "initialize" => Some(success_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                }),
            )),
            "notifications/initialized" | "initialized" => None,
            "ping" => Some(success_response(id, json!({}))),
            "tools/list" => Some(success_response(id, json!({"tools": tool_definitions()}))),
            "tools/call" => Some(handle_tool_call(id, &params)),
            "shutdown" => {
                let response = success_response(id, json!({}));
                let _ = writeln!(stdout, "{}", response);
                let _ = stdout.flush();
                return;
            }
            other => {
                if is_notification {
                    None
                } else {
                    Some(error_response(
                        id,
                        -32601,
                        &format!("method not found: {other}"),
                    ))
                }
            }
        };

        if let Some(response) = response {
            let _ = writeln!(stdout, "{}", response);
            let _ = stdout.flush();
        }
    }
}

fn success_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "vad_doctor",
            "description": "Check the Visual Automation Designer runtime: data dir, monitors, input and UIA availability.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "vad_validate",
            "description": "Validate a flow JSON file; returns structured errors/warnings with block ids.",
            "inputSchema": {
                "type": "object",
                "properties": {"flow": {"type": "string", "description": "Path to a flow JSON file"}},
                "required": ["flow"]
            }
        }),
        json!({
            "name": "vad_run",
            "description": "Run a flow headlessly. Writes runs/<timestamp>/report.json with per-step screenshot, score, hit rect, elapsed time and error. Use dryRun to resolve targets without clicking.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "flow": {"type": "string"},
                    "dryRun": {"type": "boolean"},
                    "retry": {"type": "integer", "minimum": 0},
                    "reportDir": {"type": "string"},
                    "dataDir": {"type": "string"},
                    "dpiScale": {"type": "number"}
                },
                "required": ["flow"]
            }
        }),
        json!({
            "name": "vad_step",
            "description": "Execute exactly N blocks from the flow entry, writing the same evidence report.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "flow": {"type": "string"},
                    "index": {"type": "integer", "minimum": 1},
                    "dryRun": {"type": "boolean"},
                    "dataDir": {"type": "string"}
                },
                "required": ["flow", "index"]
            }
        }),
        json!({
            "name": "vad_find",
            "description": "Template-match the current virtual desktop (ROI-first, 3s budget by default); returns score, screen center and hit rect.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "template": {"type": "string"},
                    "roi": {"type": "string", "description": "x,y,w,h in virtual-screen pixels"},
                    "threshold": {"type": "number"},
                    "budgetMs": {"type": "integer"}
                },
                "required": ["template"]
            }
        }),
        json!({
            "name": "vad_uia_find",
            "description": "Dump or filter Windows UIA controls of a window. Name is a case-insensitive substring; automationId/class/controlType are exact and case-insensitive.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window": {"type": "string"},
                    "name": {"type": "string"},
                    "automationId": {"type": "string"},
                    "class": {"type": "string"},
                    "controlType": {"type": "string"},
                    "index": {"type": "integer"},
                    "maxNodes": {"type": "integer"},
                    "maxDepth": {"type": "integer"},
                    "listWindows": {"type": "boolean"}
                }
            }
        }),
        json!({
            "name": "vad_click_target",
            "description": "Click a coordinate, template match or UIA control. UIA raises the window and refuses to click through a covering window unless force=true. dryRun resolves only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "coords": {"type": "string", "description": "x,y virtual-screen pixels"},
                    "template": {"type": "string"},
                    "uia": {"type": "boolean"},
                    "window": {"type": "string"},
                    "name": {"type": "string"},
                    "automationId": {"type": "string"},
                    "class": {"type": "string"},
                    "controlType": {"type": "string"},
                    "index": {"type": "integer"},
                    "roi": {"type": "string"},
                    "threshold": {"type": "number"},
                    "dryRun": {"type": "boolean"},
                    "force": {"type": "boolean"}
                }
            }
        }),
        json!({
            "name": "vad_list",
            "description": "List stored flows or image library entries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["flows", "images"]},
                    "dataDir": {"type": "string"}
                }
            }
        }),
    ]
}

fn handle_tool_call(id: Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let cli_args = match build_args(name, &args) {
        Ok(args) => args,
        Err(message) => {
            return success_response(
                id,
                json!({
                    "content": [{"type": "text", "text": message}],
                    "isError": true,
                }),
            )
        }
    };

    match run_vadctl(&cli_args) {
        Ok((text, success)) => success_response(
            id,
            json!({
                "content": [{"type": "text", "text": text}],
                "isError": !success,
            }),
        ),
        Err(error) => success_response(
            id,
            json!({
                "content": [{"type": "text", "text": error}],
                "isError": true,
            }),
        ),
    }
}

fn get_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn get_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn get_int(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn push_common(args: &Value, out: &mut Vec<String>) {
    if let Some(dir) = get_str(args, "dataDir") {
        out.push("--data-dir".into());
        out.push(dir);
    }
}

fn build_args(tool: &str, args: &Value) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    match tool {
        "vad_doctor" => {
            out.push("doctor".into());
            push_common(args, &mut out);
        }
        "vad_validate" => {
            let flow = get_str(args, "flow").ok_or("missing required argument: flow")?;
            out.extend(["validate".into(), "--flow".into(), flow]);
            push_common(args, &mut out);
        }
        "vad_run" => {
            let flow = get_str(args, "flow").ok_or("missing required argument: flow")?;
            out.extend(["run".into(), "--flow".into(), flow]);
            if get_bool(args, "dryRun") {
                out.push("--dry-run".into());
            }
            if let Some(retry) = get_int(args, "retry") {
                out.extend(["--retry".into(), retry.to_string()]);
            }
            if let Some(dir) = get_str(args, "reportDir") {
                out.extend(["--report-dir".into(), dir]);
            }
            if let Some(scale) = args.get("dpiScale").and_then(Value::as_f64) {
                out.extend(["--dpi-scale".into(), scale.to_string()]);
            }
            push_common(args, &mut out);
        }
        "vad_step" => {
            let flow = get_str(args, "flow").ok_or("missing required argument: flow")?;
            let index = get_int(args, "index").ok_or("missing required argument: index")?;
            out.extend([
                "step".into(),
                "--flow".into(),
                flow,
                "--index".into(),
                index.to_string(),
            ]);
            if get_bool(args, "dryRun") {
                out.push("--dry-run".into());
            }
            push_common(args, &mut out);
        }
        "vad_find" => {
            let template =
                get_str(args, "template").ok_or("missing required argument: template")?;
            out.extend(["find".into(), "--template".into(), template]);
            if let Some(roi) = get_str(args, "roi") {
                out.extend(["--roi".into(), roi]);
            }
            if let Some(threshold) = args.get("threshold").and_then(Value::as_f64) {
                out.extend(["--threshold".into(), threshold.to_string()]);
            }
            if let Some(budget) = get_int(args, "budgetMs") {
                out.extend(["--budget-ms".into(), budget.to_string()]);
            }
        }
        "vad_uia_find" => {
            if get_bool(args, "listWindows") {
                out.extend(["uia".into(), "--list-windows".into()]);
            } else {
                let window =
                    get_str(args, "window").ok_or("missing required argument: window")?;
                out.extend(["uia".into(), "--window".into(), window]);
            }
            for (key, flag) in [
                ("name", "--name"),
                ("automationId", "--automation-id"),
                ("class", "--class"),
                ("controlType", "--control-type"),
            ] {
                if let Some(value) = get_str(args, key) {
                    out.extend([flag.into(), value]);
                }
            }
            if let Some(index) = get_int(args, "index") {
                out.extend(["--index".into(), index.to_string()]);
            }
            if let Some(nodes) = get_int(args, "maxNodes") {
                out.extend(["--max-nodes".into(), nodes.to_string()]);
            }
            if let Some(depth) = get_int(args, "maxDepth") {
                out.extend(["--max-depth".into(), depth.to_string()]);
            }
        }
        "vad_click_target" => {
            out.push("click-target".into());
            if let Some(coords) = get_str(args, "coords") {
                out.extend(["--coords".into(), coords]);
            } else if let Some(template) = get_str(args, "template") {
                out.extend(["--template".into(), template]);
            } else if get_bool(args, "uia") {
                out.push("--uia".into());
                let window =
                    get_str(args, "window").ok_or("missing required argument: window")?;
                out.extend(["--window".into(), window]);
                for (key, flag) in [
                    ("name", "--name"),
                    ("automationId", "--automation-id"),
                    ("class", "--class"),
                    ("controlType", "--control-type"),
                ] {
                    if let Some(value) = get_str(args, key) {
                        out.extend([flag.into(), value]);
                    }
                }
                if let Some(index) = get_int(args, "index") {
                    out.extend(["--index".into(), index.to_string()]);
                }
            } else {
                return Err("provide coords, template or uia=true".into());
            }
            if let Some(roi) = get_str(args, "roi") {
                out.extend(["--roi".into(), roi]);
            }
            if let Some(threshold) = args.get("threshold").and_then(Value::as_f64) {
                out.extend(["--threshold".into(), threshold.to_string()]);
            }
            if get_bool(args, "dryRun") {
                out.push("--dry-run".into());
            }
            if get_bool(args, "force") {
                out.push("--force".into());
            }
        }
        "vad_list" => {
            let kind = get_str(args, "kind").unwrap_or_else(|| "flows".to_string());
            out.extend(["list".into(), kind]);
            push_common(args, &mut out);
        }
        other => return Err(format!("unknown tool: {other}")),
    }
    Ok(out)
}

fn vadctl_path() -> PathBuf {
    if let Ok(path) = std::env::var("VADCTL_PATH") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let name = if cfg!(windows) { "vadctl.exe" } else { "vadctl" };
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(if cfg!(windows) { "vadctl.exe" } else { "vadctl" })
}

fn run_vadctl(args: &[String]) -> Result<(String, bool), String> {
    let mut command = Command::new(vadctl_path());
    command.args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|e| format!("failed to launch vadctl: {e}. Build it with `cargo build --bins` or set VADCTL_PATH."))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut text = stdout;
    if !stderr.trim().is_empty() {
        if !text.trim().is_empty() {
            text.push('\n');
        }
        text.push_str(stderr.trim_end());
    }
    Ok((text, output.status.success()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_run_args() {
        let args = json!({"flow": "a.json", "dryRun": true, "retry": 1});
        let built = build_args("vad_run", &args).unwrap();
        assert_eq!(built[0], "run");
        assert!(built.contains(&"--flow".to_string()));
        assert!(built.contains(&"--dry-run".to_string()));
        assert!(built.contains(&"1".to_string()));
    }

    #[test]
    fn rejects_unknown_tool() {
        assert!(build_args("nope", &json!({})).is_err());
    }

    #[test]
    fn uia_find_accepts_list_windows() {
        let built = build_args("vad_uia_find", &json!({"listWindows": true})).unwrap();
        assert_eq!(built, vec!["uia", "--list-windows"]);
    }
}
