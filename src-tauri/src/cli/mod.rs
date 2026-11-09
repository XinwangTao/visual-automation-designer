//! `vadctl` — headless entry point for AI agents and scripts.
//!
//! Design goals:
//! - Every command prints machine-readable JSON and uses a stable exit code.
//! - `run`/`step` write evidence to `runs/<timestamp>/report.json`: per-step
//!   screenshot, score, hit rectangle, elapsed time, attempt and error.
//! - `--dry-run` resolves targets without touching the desktop.
//! - UIA clicks raise the window, re-read the control rect and refuse to click
//!   through a covering window unless `--force` is given.
//!
//! The CLI reuses the same `core`/`matching`/`platform` modules as the GUI; it
//! is a headless shell, not a second implementation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::Serialize;
use serde_json::{json, Value};

use crate::core::execution::{
    BlockEvidence, EventSink, ExecutionEvent, ExecutionStatus, Executor, HitRect,
};
use crate::core::flow::{FlowValidator, ValidationSeverity};
use crate::core::image_library::ImageLibraryManager;
use crate::error::AppError;
use crate::matching::{ImageMatcher, MatchConfig, SearchRegion};
use crate::models::{BlockConfig, BlockNode, ClickMode, Flow, ImageId, ImageMetadata};
use crate::platform::{InputController, MouseButton, ScreenCapture};

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_TARGET: i32 = 3;
const EXIT_ASSERT: i32 = 4;
const EXIT_RUNTIME: i32 = 5;

const DEFAULT_MATCH_THRESHOLD: f64 = 0.7;
const DEFAULT_SEARCH_BUDGET_MS: u64 = 3000;

/// Error carrying the process exit code.
#[derive(Debug)]
struct CliError {
    code: i32,
    message: String,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self { code: EXIT_USAGE, message: message.into() }
    }
    fn target(message: impl Into<String>) -> Self {
        Self { code: EXIT_TARGET, message: message.into() }
    }
    fn runtime(message: impl Into<String>) -> Self {
        Self { code: EXIT_RUNTIME, message: message.into() }
    }
}

impl From<AppError> for CliError {
    fn from(error: AppError) -> Self {
        let code = match &error {
            AppError::ImageNotFound(_) | AppError::BlockNotFound(_) | AppError::FlowNotFound(_) => {
                EXIT_TARGET
            }
            AppError::ExecutionFailed(message) => exit_code_for_message(message),
            AppError::IoError(_)
            | AppError::InternalError(_)
            | AppError::PlatformError(_)
            | AppError::ImageError(_)
            | AppError::SerializationError(_) => EXIT_RUNTIME,
            AppError::InvalidFlow(_) | AppError::ValidationError(_) => EXIT_USAGE,
        };
        Self { code, message: error.to_string() }
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self { code: EXIT_RUNTIME, message: error.to_string() }
    }
}

fn exit_code_for_message(message: &str) -> i32 {
    let m = message.to_lowercase();
    if m.contains("not found")
        || m.contains("未找到")
        || m.contains("遮挡")
        || m.contains("accuracy")
        || m.contains("低于阈值")
    {
        EXIT_TARGET
    } else if m.contains("assert") || m.contains("expect") || m.contains("timeout") {
        EXIT_ASSERT
    } else {
        EXIT_ASSERT
    }
}

// ============================================================================
// Argument parsing
// ============================================================================

const BOOLEAN_FLAGS: &[&str] = &[
    "json",
    "dry-run",
    "force",
    "no-screenshots",
    "list-windows",
    "uia",
    "help",
];

#[derive(Debug, Default)]
struct Args {
    command: String,
    positionals: Vec<String>,
    values: HashMap<String, String>,
    flags: HashSet<String>,
}

impl Args {
    fn parse<I: IntoIterator<Item = String>>(argv: I) -> Result<Self, CliError> {
        let mut iter = argv.into_iter().peekable();
        let mut args = Args::default();

        // First token is the sub-command. Accept `--help` before it.
        while let Some(token) = iter.next() {
            if token == "--help" || token == "-h" || token == "help" {
                args.command = "help".to_string();
                return Ok(args);
            }
            if token.starts_with('-') {
                return Err(CliError::usage(format!(
                    "expected a command before option {token:?}; run `vadctl help`"
                )));
            }
            args.command = token;
            break;
        }
        if args.command.is_empty() {
            args.command = "help".to_string();
            return Ok(args);
        }

        while let Some(token) = iter.next() {
            if let Some(rest) = token.strip_prefix("--") {
                if rest.is_empty() {
                    return Err(CliError::usage("empty option name"));
                }
                if let Some((name, value)) = rest.split_once('=') {
                    args.values.insert(name.to_string(), value.to_string());
                    continue;
                }
                if BOOLEAN_FLAGS.contains(&rest) {
                    args.flags.insert(rest.to_string());
                } else {
                    let value = iter.next().ok_or_else(|| {
                        CliError::usage(format!("option --{rest} requires a value"))
                    })?;
                    args.values.insert(rest.to_string(), value);
                }
            } else if token == "-h" {
                args.flags.insert("help".to_string());
            } else {
                args.positionals.push(token);
            }
        }
        Ok(args)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|s| s.as_str())
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn require(&self, name: &str) -> Result<&str, CliError> {
        self.value(name)
            .ok_or_else(|| CliError::usage(format!("missing required option --{name}")))
    }
}

// ============================================================================
// Public entry
// ============================================================================

/// Run `vadctl` with the process arguments (including argv[0]).
pub fn run<I: IntoIterator<Item = String>>(argv: I) -> i32 {
    let parsed = Args::parse(argv.into_iter().skip(1));
    let args = match parsed {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{}", error.message);
            return error.code;
        }
    };

    let result = match args.command.as_str() {
        "help" => {
            print_help();
            Ok(EXIT_OK)
        }
        "doctor" => cmd_doctor(&args),
        "validate" => cmd_validate(&args),
        "list" => cmd_list(&args),
        "get" => cmd_get(&args),
        "find" => cmd_find(&args),
        "uia" => cmd_uia(&args),
        "click-target" => cmd_click_target(&args),
        "run" => block_on(cmd_run(&args)),
        "step" => block_on(cmd_step(&args)),
        other => Err(CliError::usage(format!(
            "unknown command {other:?}; run `vadctl help`"
        ))),
    };

    match result {
        Ok(code) => code,
        Err(error) => {
            let payload = json!({
                "ok": false,
                "error": error.message,
                "exitCode": error.code,
            });
            println!("{}", pretty(&payload));
            eprintln!("vadctl: {}", error.message);
            error.code
        }
    }
}

fn print_help() {
    println!(
        r#"vadctl — headless automation debugger for Visual Automation Designer

USAGE:
  vadctl <command> [options]

COMMANDS:
  doctor                         Environment self-check (data dir, monitors, input, UIA)
  validate --flow FILE           Validate a flow and print structured diagnostics
  list [flows|images]            List stored flows or image library entries
  get --flow FILE | --flow-id ID Print one flow JSON
  find --template FILE           Match a template on the current screen
       [--roi x,y,w,h] [--threshold T] [--budget-ms N]
  uia --window TITLE             Dump/filter UIA controls
       [--name X] [--automation-id X] [--class X] [--control-type X]
       [--index N] [--max-nodes N] [--max-depth N]
       [--list-windows]
  click-target --coords x,y      Resolve and click a target
       | --template FILE
       | --uia --window TITLE [--name X|--automation-id X|--class X|--control-type X]
       [--dry-run] [--force]
  run --flow FILE                Execute a flow headlessly and write evidence
       [--dry-run] [--retry N] [--report-dir DIR] [--no-screenshots]
       [--dpi-scale F]
  step --flow FILE --index N     Execute exactly N blocks from the entry

GLOBAL:
  --json                         (all commands already print JSON)
  --data-dir DIR                 App data dir; default %APPDATA%\com.visualautomation.designer

EXIT CODES:
  0 success   2 usage/parse/validation   3 target missing/occluded
  4 assertion/expect failed              5 runtime/IO/COM error
"#
    );
}

// ============================================================================
// Shared helpers
// ============================================================================

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Run an async command on a small runtime.
///
/// The CLI is synchronous at the edge (exit code), but the execution engine is
/// async and shared with Tauri.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
        .block_on(future)
}

fn data_dir(args: &Args) -> PathBuf {
    if let Some(dir) = args.value("data-dir") {
        return PathBuf::from(dir);
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("com.visualautomation.designer");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local/share")
            .join("com.visualautomation.designer");
    }
    PathBuf::from(".")
}

fn resolve_flow(args: &Args, data_dir: &Path) -> Result<(Flow, String), CliError> {
    let source = if let Some(path) = args.value("flow") {
        path.to_string()
    } else if let Some(id) = args.value("flow-id") {
        data_dir
            .join("flows")
            .join(format!("{id}.json"))
            .to_string_lossy()
            .to_string()
    } else if let Some(path) = args.positionals.first() {
        path.clone()
    } else {
        return Err(CliError::usage(
            "provide --flow <path> or --flow-id <id>",
        ));
    };

    let text = fs::read_to_string(&source).map_err(|e| {
        CliError::usage(format!("cannot read flow {source:?}: {e}"))
    })?;
    let flow: Flow = serde_json::from_str(&text)
        .map_err(|e| CliError::usage(format!("invalid flow JSON in {source:?}: {e}")))?;
    Ok((flow, source))
}

fn parse_pair<T>(value: &str, name: &str) -> Result<(T, T), CliError>
where
    T: std::str::FromStr,
{
    let mut parts = value.split(',');
    let parse = |s: Option<&str>| -> Result<T, CliError> {
        s.ok_or_else(|| CliError::usage(format!("--{name} expects two comma-separated values")))?
            .trim()
            .parse::<T>()
            .map_err(|_| CliError::usage(format!("--{name} has an invalid number: {value:?}")))
    };
    let a = parse(parts.next())?;
    let b = parse(parts.next())?;
    Ok((a, b))
}

fn parse_roi(value: &str) -> Result<(i32, i32, u32, u32), CliError> {
    let nums: Vec<i64> = value
        .split(',')
        .map(|s| s.trim().parse::<i64>())
        .collect::<Result<_, _>>()
        .map_err(|_| CliError::usage(format!("--roi expects x,y,w,h; got {value:?}")))?;
    if nums.len() != 4 {
        return Err(CliError::usage(format!(
            "--roi expects x,y,w,h; got {value:?}"
        )));
    }
    Ok((
        nums[0] as i32,
        nums[1] as i32,
        nums[2].max(0) as u32,
        nums[3].max(0) as u32,
    ))
}

fn block_label(node: &BlockNode) -> String {
    match &node.config {
        BlockConfig::Click { mode, .. } => match mode {
            ClickMode::Coordinates { .. } => "click:coords".to_string(),
            ClickMode::Image { .. } => "click:image".to_string(),
            ClickMode::Uia { .. } => "click:uia".to_string(),
        },
        BlockConfig::WaitImage { .. } => "wait_image".to_string(),
        BlockConfig::WaitTime { .. } => "wait_time".to_string(),
        BlockConfig::InputText { .. } => "input_text".to_string(),
        BlockConfig::TextExtract { .. } => "text_extract".to_string(),
        BlockConfig::ScreenshotAssert { .. } => "screenshot_assert".to_string(),
        BlockConfig::Loop { .. } => "loop".to_string(),
        BlockConfig::LoopInfinite => "loop_infinite".to_string(),
        BlockConfig::Condition { .. } => "condition".to_string(),
        BlockConfig::TextCheck { .. } => "text_check".to_string(),
    }
}

fn block_labels(flow: &Flow) -> HashMap<String, String> {
    flow.blocks
        .iter()
        .map(|(id, node)| (id.to_string(), block_label(node)))
        .collect()
}

fn sanitize_file_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn image_library_map(images_dir: &Path) -> Result<HashMap<ImageId, ImageMetadata>, CliError> {
    let manager = ImageLibraryManager::new(images_dir)?;
    Ok(manager
        .list_images()?
        .into_iter()
        .map(|metadata| (metadata.id.clone(), metadata))
        .collect())
}

fn image_rect(center_x: i32, center_y: i32, width: u32, height: u32) -> HitRect {
    HitRect::new(
        center_x - (width / 2) as i32,
        center_y - (height / 2) as i32,
        width,
        height,
    )
}

// ============================================================================
// doctor
// ============================================================================

fn cmd_doctor(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let monitors = ScreenCapture::list_monitors().unwrap_or_default();
    let capture = ScreenCapture::new().capture_screen();
    let input_ok = InputController::new().is_ok();

    let flows_dir = dir.join("flows");
    let flow_count = fs::read_dir(&flows_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().and_then(|x| x.to_str()) == Some("json")
                })
                .count()
        })
        .unwrap_or(0);
    let images_dir = dir.join("images");
    let image_count = fs::read_dir(&images_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    matches!(
                        e.path().extension().and_then(|x| x.to_str()),
                        Some("png") | Some("jpg") | Some("jpeg") | Some("bmp")
                    )
                })
                .count()
        })
        .unwrap_or(0);

    let windows = uia_window_titles();

    let payload = json!({
        "ok": capture.is_ok() && input_ok,
        "platform": std::env::consts::OS,
        "dataDir": dir.to_string_lossy(),
        "dataDirExists": dir.exists(),
        "flowCount": flow_count,
        "imageCount": image_count,
        "monitors": monitors.iter().map(|m| json!({
            "index": m.index,
            "x": m.x, "y": m.y,
            "width": m.width, "height": m.height,
            "scaleFactor": m.scale_factor,
        })).collect::<Vec<_>>(),
        "capture": capture.as_ref().map(|c| json!({"ok": true, "width": c.width, "height": c.height, "originX": c.origin_x, "originY": c.origin_y}))
            .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()})),
        "input": input_ok,
        "uia": {
            "available": cfg!(windows),
            "windowCount": windows.len(),
            "windows": windows.into_iter().take(20).collect::<Vec<_>>(),
        },
    });
    println!("{}", pretty(&payload));
    Ok(if capture.is_ok() && input_ok { EXIT_OK } else { EXIT_RUNTIME })
}

#[cfg(windows)]
fn uia_window_titles() -> Vec<String> {
    crate::platform::uia::list_window_titles()
}

#[cfg(not(windows))]
fn uia_window_titles() -> Vec<String> {
    Vec::new()
}

// ============================================================================
// validate / list / get
// ============================================================================

fn cmd_validate(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let (flow, source) = resolve_flow(args, &dir)?;
    let diagnostics = FlowValidator::new().validate(&flow);

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for diagnostic in diagnostics {
        let severity = match diagnostic.severity {
            ValidationSeverity::Error => "error",
            ValidationSeverity::Warning => "warning",
        };
        let entry = json!({
            "severity": severity,
            "code": diagnostic.code,
            "message": diagnostic.message,
            "blockId": diagnostic.block_id.map(|b| b.to_string()),
            "connectionId": diagnostic.connection_id,
        });
        if matches!(diagnostic.severity, ValidationSeverity::Error) {
            errors.push(entry);
        } else {
            warnings.push(entry);
        }
    }

    let ok = errors.is_empty();
    println!(
        "{}",
        pretty(&json!({
            "ok": ok,
            "flow": source,
            "errorCount": errors.len(),
            "warningCount": warnings.len(),
            "errors": errors,
            "warnings": warnings,
        }))
    );
    Ok(if ok { EXIT_OK } else { EXIT_USAGE })
}

fn cmd_list(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let kind = args.positionals.first().map(|s| s.as_str()).unwrap_or("flows");
    let payload = match kind {
        "flows" => {
            let mut manager = crate::core::flow::FlowManager::new(&dir)?;
            let flows = manager.list_flows()?;
            json!({"ok": true, "kind": "flows", "items": flows})
        }
        "images" => {
            let manager = ImageLibraryManager::new(dir.join("images"))?;
            let images = manager.list_images()?;
            json!({"ok": true, "kind": "images", "items": images})
        }
        other => {
            return Err(CliError::usage(format!(
                "unknown list kind {other:?}; use `flows` or `images`"
            )))
        }
    };
    println!("{}", pretty(&payload));
    Ok(EXIT_OK)
}

fn cmd_get(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let (flow, source) = resolve_flow(args, &dir)?;
    println!(
        "{}",
        pretty(&json!({"ok": true, "flowPath": source, "flow": flow}))
    );
    Ok(EXIT_OK)
}

// ============================================================================
// find
// ============================================================================

fn cmd_find(args: &Args) -> Result<i32, CliError> {
    let template_path = args.require("template")?;
    let threshold = args
        .value("threshold")
        .map(|v| v.parse::<f64>().map_err(|_| CliError::usage("invalid --threshold")))
        .transpose()?
        .unwrap_or(DEFAULT_MATCH_THRESHOLD);
    let budget_ms = args
        .value("budget-ms")
        .map(|v| v.parse::<u64>().map_err(|_| CliError::usage("invalid --budget-ms")))
        .transpose()?
        .unwrap_or(DEFAULT_SEARCH_BUDGET_MS);
    let roi = args.value("roi").map(parse_roi).transpose()?;

    let capture = ScreenCapture::capture_virtual_desktop()
        .map_err(|e| CliError::runtime(format!("screen capture failed: {e}")))?;

    let region = roi.map(|(x, y, w, h)| {
        let ix = (x - capture.origin_x).max(0) as u32;
        let iy = (y - capture.origin_y).max(0) as u32;
        SearchRegion::new(ix, iy, w, h)
    });

    let template = image::open(template_path)
        .map_err(|e| CliError::usage(format!("cannot open template {template_path:?}: {e}")))?;

    let matcher = ImageMatcher::with_config(MatchConfig::with_threshold(threshold));
    let started = Instant::now();
    let result = matcher.find_image_bounded(
        &capture.image,
        &template,
        region,
        Duration::from_millis(budget_ms),
    );
    let elapsed_ms = started.elapsed().as_millis();

    let found = result.found;
    let confidence = result.confidence.unwrap_or(0.0);
    let center = if found {
        Some((
            result.center_x.unwrap_or(0) as i32 + capture.origin_x,
            result.center_y.unwrap_or(0) as i32 + capture.origin_y,
        ))
    } else {
        None
    };
    let hit_rect = center.map(|(cx, cy)| {
        image_rect(
            cx,
            cy,
            result.width.unwrap_or(0),
            result.height.unwrap_or(0),
        )
    });

    println!(
        "{}",
        pretty(&json!({
            "ok": found,
            "found": found,
            "score": confidence,
            "threshold": threshold,
            "center": center.map(|(x, y)| json!({"x": x, "y": y})),
            "hitRect": hit_rect,
            "elapsedMs": elapsed_ms,
            "budgetMs": budget_ms,
            "usedRoi": region.is_some(),
        }))
    );
    Ok(if found { EXIT_OK } else { EXIT_TARGET })
}

// ============================================================================
// uia
// ============================================================================

fn cmd_uia(args: &Args) -> Result<i32, CliError> {
    if args.flag("list-windows") {
        let windows = uia_window_titles();
        println!("{}", pretty(&json!({"ok": true, "windows": windows})));
        return Ok(EXIT_OK);
    }

    #[cfg(not(windows))]
    {
        let _ = args;
        return Err(CliError::runtime("UIA is only available on Windows"));
    }

    #[cfg(windows)]
    {
        use crate::platform::uia::{self, UiaSelector};

        let window = args.require("window")?;
        let selector = UiaSelector {
            name: args.value("name").map(str::to_string),
            automation_id: args
                .value("automation-id")
                .map(str::to_string),
            class_name: args.value("class").map(str::to_string),
            control_type: args.value("control-type").map(str::to_string),
            index: args
                .value("index")
                .map(|v| {
                    v.parse::<usize>()
                        .map_err(|_| CliError::usage("invalid --index"))
                })
                .transpose()?,
        };
        let selector_given = !selector.is_empty();
        let max_nodes = args
            .value("max-nodes")
            .map(|v| v.parse::<usize>().map_err(|_| CliError::usage("invalid --max-nodes")))
            .transpose()?
            .unwrap_or(400);
        let max_depth = args
            .value("max-depth")
            .map(|v| v.parse::<usize>().map_err(|_| CliError::usage("invalid --max-depth")))
            .transpose()?
            .unwrap_or(8);

        let nodes = uia::dump(
            window,
            max_nodes,
            max_depth,
            if selector_given { Some(selector) } else { None },
        )
        .map_err(CliError::runtime)?;

        let ok = !(selector_given && nodes.is_empty());
        println!(
            "{}",
            pretty(&json!({
                "ok": ok,
                "window": window,
                "count": nodes.len(),
                "nodes": nodes,
            }))
        );
        Ok(if ok { EXIT_OK } else { EXIT_TARGET })
    }
}

// ============================================================================
// click-target
// ============================================================================

fn cmd_click_target(args: &Args) -> Result<i32, CliError> {
    let dry_run = args.flag("dry-run");
    let force = args.flag("force");

    let (x, y, target, evidence) = if let Some(value) = args.value("coords") {
        let (x, y) = parse_pair::<i32>(value, "coords")?;
        (
            x,
            y,
            "coords".to_string(),
            BlockEvidence {
                score: None,
                hit_rect: None,
                target: Some(format!("coords ({x},{y})")),
            },
        )
    } else if let Some(template) = args.value("template") {
        let (x, y, evidence) = resolve_template(args, template)?;
        (x, y, "template".to_string(), evidence)
    } else if args.flag("uia") {
        #[cfg(not(windows))]
        {
            let _ = (force, dry_run);
            return Err(CliError::runtime("UIA is only available on Windows"));
        }
        #[cfg(windows)]
        {
            use crate::platform::uia::{self, UiaSelector};
            let window = args.require("window")?;
            let selector = UiaSelector {
                name: args.value("name").map(str::to_string),
                automation_id: args.value("automation-id").map(str::to_string),
                class_name: args.value("class").map(str::to_string),
                control_type: args.value("control-type").map(str::to_string),
                index: args
                    .value("index")
                    .map(|v| {
                        v.parse::<usize>()
                            .map_err(|_| CliError::usage("invalid --index"))
                    })
                    .transpose()?,
            };
            let node = if dry_run {
                uia::find(window, &selector)
                    .map_err(CliError::runtime)?
                    .ok_or_else(|| CliError::target("UIA control not found"))?
            } else {
                uia::prepare_click(window, &selector, force).map_err(|e| {
                    if e.contains("遮挡") || e.contains("未找到") {
                        CliError::target(e)
                    } else {
                        CliError::runtime(e)
                    }
                })?
            };
            let (x, y) = (node.cx(), node.cy());
            let evidence = BlockEvidence {
                score: Some(1.0),
                hit_rect: Some(HitRect::new(node.x, node.y, node.w.max(0) as u32, node.h.max(0) as u32)),
                target: Some(format!("uia {} \"{}\"", node.control_type_name, node.name)),
            };
            (x, y, "uia".to_string(), evidence)
        }
    } else {
        return Err(CliError::usage(
            "click-target needs --coords x,y, --template FILE, or --uia",
        ));
    };

    let clicked = if dry_run {
        false
    } else {
        let mut input = InputController::new()?;
        input.click_at(x, y, MouseButton::Left)?;
        true
    };

    println!(
        "{}",
        pretty(&json!({
            "ok": true,
            "clicked": clicked,
            "dryRun": dry_run,
            "target": target,
            "point": {"x": x, "y": y},
            "evidence": evidence,
        }))
    );
    Ok(EXIT_OK)
}

fn resolve_template(args: &Args, template_path: &str) -> Result<(i32, i32, BlockEvidence), CliError> {
    let threshold = args
        .value("threshold")
        .map(|v| v.parse::<f64>().map_err(|_| CliError::usage("invalid --threshold")))
        .transpose()?
        .unwrap_or(DEFAULT_MATCH_THRESHOLD);
    let budget_ms = args
        .value("budget-ms")
        .map(|v| v.parse::<u64>().map_err(|_| CliError::usage("invalid --budget-ms")))
        .transpose()?
        .unwrap_or(DEFAULT_SEARCH_BUDGET_MS);
    let roi = args.value("roi").map(parse_roi).transpose()?;

    let capture = ScreenCapture::capture_virtual_desktop()
        .map_err(|e| CliError::runtime(format!("screen capture failed: {e}")))?;
    let region = roi.map(|(x, y, w, h)| {
        SearchRegion::new(
            (x - capture.origin_x).max(0) as u32,
            (y - capture.origin_y).max(0) as u32,
            w,
            h,
        )
    });
    let template = image::open(template_path)
        .map_err(|e| CliError::usage(format!("cannot open template {template_path:?}: {e}")))?;
    let matcher = ImageMatcher::with_config(MatchConfig::with_threshold(threshold));
    let result = matcher.find_image_bounded(
        &capture.image,
        &template,
        region,
        Duration::from_millis(budget_ms),
    );
    if !result.found {
        return Err(CliError::target(format!(
            "template not found (best {:.1}% < {:.0}%)",
            result.confidence.unwrap_or(0.0) * 100.0,
            threshold * 100.0
        )));
    }
    let cx = result.center_x.unwrap_or(0) as i32 + capture.origin_x;
    let cy = result.center_y.unwrap_or(0) as i32 + capture.origin_y;
    let evidence = BlockEvidence {
        score: result.confidence,
        hit_rect: Some(image_rect(
            cx,
            cy,
            result.width.unwrap_or(0),
            result.height.unwrap_or(0),
        )),
        target: Some(format!("template {template_path}")),
    };
    Ok((cx, cy, evidence))
}

// ============================================================================
// Evidence sink and report
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepReport {
    index: usize,
    block_id: String,
    action: String,
    ok: bool,
    attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hit_rect: Option<HitRect>,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    schema_version: u32,
    flow: String,
    name: String,
    ok: bool,
    dry_run: bool,
    status: String,
    started_at: String,
    finished_at: String,
    report_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    steps: Vec<StepReport>,
}

struct CliSink {
    steps: Mutex<Vec<StepReport>>,
    labels: HashMap<String, String>,
    report_dir: PathBuf,
    screenshots: bool,
    counter: AtomicUsize,
}

impl CliSink {
    fn new(report_dir: PathBuf, labels: HashMap<String, String>, screenshots: bool) -> Self {
        Self {
            steps: Mutex::new(Vec::new()),
            labels,
            report_dir,
            screenshots,
            counter: AtomicUsize::new(0),
        }
    }

    fn capture(&self, index: usize, block_id: &str) -> Option<String> {
        if !self.screenshots {
            return None;
        }
        let capture = ScreenCapture::new().capture_screen().ok()?;
        let file_name = format!(
            "{:02}_{}.png",
            index,
            sanitize_file_component(block_id)
        );
        let path = self.report_dir.join(&file_name);
        capture.image.save(&path).ok()?;
        Some(path.to_string_lossy().to_string())
    }

    fn record(
        &self,
        block_id: &str,
        ok: bool,
        attempts: u32,
        elapsed_ms: u64,
        evidence: Option<BlockEvidence>,
        error: Option<String>,
    ) {
        let index = self.counter.fetch_add(1, Ordering::SeqCst);
        let screenshot = self.capture(index, block_id);
        let evidence = evidence.unwrap_or_default();
        let report = StepReport {
            index,
            block_id: block_id.to_string(),
            action: self
                .labels
                .get(block_id)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            ok,
            attempts,
            score: evidence.score,
            hit_rect: evidence.hit_rect,
            elapsed_ms,
            screenshot,
            target: evidence.target,
            error,
        };
        if let Ok(mut steps) = self.steps.lock() {
            steps.push(report);
        }
    }

    fn steps(&self) -> Vec<StepReport> {
        self.steps.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl EventSink for CliSink {
    fn emit(&self, event: &ExecutionEvent) {
        match event {
            ExecutionEvent::BlockCompleted {
                block_id,
                success,
                elapsed_ms,
                attempt,
                evidence,
                ..
            } => self.record(
                &block_id.to_string(),
                *success,
                *attempt,
                *elapsed_ms,
                evidence.clone(),
                None,
            ),
            ExecutionEvent::BlockError {
                block_id,
                message,
                elapsed_ms,
                attempt,
                evidence,
                ..
            } => self.record(
                &block_id.to_string(),
                false,
                *attempt,
                *elapsed_ms,
                evidence.clone(),
                Some(message.clone()),
            ),
            _ => {}
        }
    }
}

fn write_report(
    report_dir: &Path,
    report: &RunReport,
) -> Result<(), CliError> {
    fs::create_dir_all(report_dir)?;
    let path = report_dir.join("report.json");
    let content = serde_json::to_string_pretty(report)
        .map_err(|e| CliError::runtime(format!("cannot serialize report: {e}")))?;
    fs::write(&path, content)?;
    Ok(())
}

fn status_name(status: ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Idle => "idle",
        ExecutionStatus::Running => "running",
        ExecutionStatus::Paused => "paused",
        ExecutionStatus::Completed => "completed",
        ExecutionStatus::Stopped => "stopped",
        ExecutionStatus::Error => "error",
    }
}

// ============================================================================
// run / step
// ============================================================================

fn build_executor(
    args: &Args,
    flow: &Flow,
    data_dir: &Path,
) -> Result<(Executor, Arc<CliSink>, PathBuf), CliError> {
    let images_dir = data_dir.join("images");
    let library = image_library_map(&images_dir)?;

    let run_id = Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
    let report_root = args
        .value("report-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("runs"));
    let report_dir = report_root.join(run_id);
    fs::create_dir_all(&report_dir)?;

    let sink = Arc::new(CliSink::new(
        report_dir.clone(),
        block_labels(flow),
        !args.flag("no-screenshots"),
    ));

    let mut executor = Executor::new(flow.clone(), sink.clone(), images_dir);
    executor.set_image_library(library);
    executor.set_retry_limit(
        args.value("retry")
            .map(|v| v.parse::<u32>().map_err(|_| CliError::usage("invalid --retry")))
            .transpose()?
            .unwrap_or(2),
    );
    executor.set_dry_run(args.flag("dry-run"));
    if let Some(scale) = args.value("dpi-scale") {
        let scale = scale
            .parse::<f32>()
            .map_err(|_| CliError::usage("invalid --dpi-scale"))?;
        executor.set_dpi_scale(scale);
    }

    Ok((executor, sink, report_dir))
}

fn validate_or_exit(flow: &Flow, source: &str) -> Result<bool, CliError> {
    let diagnostics = FlowValidator::new().validate(flow);
    let errors: Vec<Value> = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, ValidationSeverity::Error))
        .map(|d| {
            json!({
                "code": d.code.clone(),
                "message": d.message.clone(),
                "blockId": d.block_id.clone().map(|b| b.to_string()),
            })
        })
        .collect();
    if errors.is_empty() {
        return Ok(true);
    }
    println!(
        "{}",
        pretty(&json!({
            "ok": false,
            "flow": source,
            "error": "flow validation failed",
            "errors": errors,
            "exitCode": EXIT_USAGE,
        }))
    );
    Ok(false)
}

async fn cmd_run(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let (flow, source) = resolve_flow(args, &dir)?;
    if !validate_or_exit(&flow, &source)? {
        return Ok(EXIT_USAGE);
    }

    let (mut executor, sink, report_dir) = build_executor(args, &flow, &dir)?;
    let dry_run = args.flag("dry-run");
    let started_at = Local::now().to_rfc3339();

    let result = executor.start().await;
    let status = executor.status().await;
    let finished_at = Local::now().to_rfc3339();
    let error = result.as_ref().err().map(|e| e.to_string());

    let steps = sink.steps();
    let ok = result.is_ok() && steps.iter().all(|s| s.ok);
    let report = RunReport {
        schema_version: 1,
        flow: source.clone(),
        name: flow.name.clone(),
        ok,
        dry_run,
        status: status_name(status).to_string(),
        started_at,
        finished_at,
        report_dir: report_dir.to_string_lossy().to_string(),
        error: error.clone(),
        steps: steps.clone(),
    };
    write_report(&report_dir, &report)?;

    let exit_code = match &result {
        Ok(()) if ok => EXIT_OK,
        Ok(()) => EXIT_ASSERT,
        Err(error) => CliError::from(AppError::ExecutionFailed(error.to_string())).code,
    };

    println!(
        "{}",
        pretty(&json!({
            "ok": ok,
            "dryRun": dry_run,
            "status": report.status,
            "flow": source,
            "report": report_dir.to_string_lossy(),
            "stepCount": steps.len(),
            "failedSteps": steps.iter().filter(|s| !s.ok).count(),
            "error": error,
            "exitCode": exit_code,
        }))
    );

    Ok(exit_code)
}

async fn cmd_step(args: &Args) -> Result<i32, CliError> {
    let dir = data_dir(args);
    let (flow, source) = resolve_flow(args, &dir)?;
    if !validate_or_exit(&flow, &source)? {
        return Ok(EXIT_USAGE);
    }

    let count = args
        .value("index")
        .map(|v| v.parse::<usize>().map_err(|_| CliError::usage("invalid --index")))
        .transpose()?
        .unwrap_or(1);
    if count == 0 {
        return Err(CliError::usage("--index must be >= 1"));
    }

    let (mut executor, sink, report_dir) = build_executor(args, &flow, &dir)?;
    let started_at = Local::now().to_rfc3339();

    let mut error: Option<String> = None;
    for _ in 0..count {
        match executor.step().await {
            Ok(()) => {
                if matches!(executor.status().await, ExecutionStatus::Completed) {
                    break;
                }
            }
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }
    let status = executor.status().await;
    let finished_at = Local::now().to_rfc3339();

    let steps = sink.steps();
    let ok = error.is_none() && steps.iter().all(|s| s.ok);
    let report = RunReport {
        schema_version: 1,
        flow: source.clone(),
        name: flow.name.clone(),
        ok,
        dry_run: args.flag("dry-run"),
        status: status_name(status).to_string(),
        started_at,
        finished_at,
        report_dir: report_dir.to_string_lossy().to_string(),
        error: error.clone(),
        steps: steps.clone(),
    };
    write_report(&report_dir, &report)?;

    let exit_code = match &error {
        None if ok => EXIT_OK,
        None => EXIT_ASSERT,
        Some(message) => CliError::from(AppError::ExecutionFailed(message.clone())).code,
    };

    println!(
        "{}",
        pretty(&json!({
            "ok": ok,
            "status": report.status,
            "flow": source,
            "report": report_dir.to_string_lossy(),
            "stepsRequested": count,
            "stepCount": steps.len(),
            "error": error,
            "exitCode": exit_code,
        }))
    );
    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Args {
        let mut argv = vec!["vadctl".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        Args::parse(argv.into_iter().skip(1)).expect("args should parse")
    }

    #[test]
    fn parses_command_and_options() {
        let args = parse(&["run", "--flow", "a.json", "--dry-run", "--retry", "3"]);
        assert_eq!(args.command, "run");
        assert_eq!(args.value("flow"), Some("a.json"));
        assert!(args.flag("dry-run"));
        assert_eq!(args.value("retry"), Some("3"));
    }

    #[test]
    fn parses_equals_form() {
        let args = parse(&["find", "--template=t.png", "--threshold=0.9"]);
        assert_eq!(args.value("template"), Some("t.png"));
        assert_eq!(args.value("threshold"), Some("0.9"));
    }

    #[test]
    fn missing_value_is_usage_error() {
        let mut argv = vec!["vadctl".to_string(), "validate".to_string(), "--flow".to_string()];
        let err = Args::parse(argv.drain(..)).unwrap_err();
        assert_eq!(err.code, EXIT_USAGE);
    }

    #[test]
    fn parses_negative_pair() {
        let (x, y) = parse_pair::<i32>("-1920,80", "coords").unwrap();
        assert_eq!((x, y), (-1920, 80));
    }

    #[test]
    fn parses_roi() {
        let (x, y, w, h) = parse_roi("10,20,300,400").unwrap();
        assert_eq!((x, y, w, h), (10, 20, 300, 400));
    }

    #[test]
    fn maps_target_errors_to_exit_three() {
        assert_eq!(exit_code_for_message("image not found on screen"), EXIT_TARGET);
        assert_eq!(exit_code_for_message("UIA 控件被遮挡"), EXIT_TARGET);
        assert_eq!(exit_code_for_message("assertion failed"), EXIT_ASSERT);
    }

    #[test]
    fn labels_blocks() {
        let node = BlockNode::new(
            crate::models::BlockType::Action {
                action: crate::models::ActionType::Click,
            },
            crate::models::BlockPosition::new(0.0, 0.0),
            BlockConfig::Click {
                mode: ClickMode::Coordinates { x: -10, y: 20 },
                count: 1,
            },
        );
        assert_eq!(block_label(&node), "click:coords");
    }
}
