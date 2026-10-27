use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use serde_json::Value;

use crate::config;
use crate::error::{Error, Result};
use crate::storage::Repository;

const CODEX_OVERRIDE_ENV: &str = "NOTEON_CODEX_PATH";
const MCP_SERVER_NAME: &str = "noteon";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum RunEvent {
    SnapshotReady(PathBuf),
    Thinking,
    ToolCall(String),
    Message(String),
    Finished(std::result::Result<(), String>),
}

pub struct RunRequest {
    pub prompt: String,
    pub data_directory: PathBuf,
    pub model: String,
    pub effort: String,
    pub context: egui::Context,
}

pub struct RunHandle {
    pub events: Receiver<RunEvent>,
    child: Arc<Mutex<Option<Child>>>,
}

impl RunHandle {
    pub fn cancel(&self) {
        if let Ok(mut guard) = self.child.lock()
            && let Some(child) = guard.as_mut()
        {
            let _ = child.kill();
        }
    }
}

pub fn start(request: RunRequest) -> RunHandle {
    let (sender, events) = mpsc::channel();
    let child = Arc::new(Mutex::new(None));
    let shared = Arc::clone(&child);
    std::thread::Builder::new()
        .name("noteon-assistant".into())
        .spawn(move || {
            let result = run(&request, &sender, &shared);
            let _ = sender.send(RunEvent::Finished(
                result.map_err(|error| error.to_string()),
            ));
            request.context.request_repaint();
        })
        .ok();
    RunHandle { events, child }
}

fn run(
    request: &RunRequest,
    sender: &Sender<RunEvent>,
    shared: &Arc<Mutex<Option<Child>>>,
) -> Result<()> {
    let snapshot = create_snapshot(&request.data_directory)?;
    let _ = sender.send(RunEvent::SnapshotReady(snapshot));
    request.context.request_repaint();
    let mut child = spawn_codex(request)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("Could not read Codex output.".into()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(request.prompt.as_bytes())?;
    }
    *shared.lock().map_err(|_| Error::WorkerUnavailable)? = Some(child);
    let outcome = read_events(stdout, sender, &request.context);
    let status = wait_for_exit(shared)?;
    match (outcome, status) {
        (Some(message), _) => Err(Error::Invalid(message)),
        (None, false) => Err(Error::Invalid("Codex stopped before finishing.".into())),
        (None, true) => Ok(()),
    }
}

fn wait_for_exit(shared: &Arc<Mutex<Option<Child>>>) -> Result<bool> {
    let mut guard = shared.lock().map_err(|_| Error::WorkerUnavailable)?;
    let success = match guard.as_mut() {
        Some(child) => child.wait()?.success(),
        None => false,
    };
    *guard = None;
    Ok(success)
}

fn create_snapshot(data_directory: &Path) -> Result<PathBuf> {
    let directory = data_directory.join("ai-snapshots");
    std::fs::create_dir_all(&directory)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let path = directory.join(format!("before-{stamp}.sqlite"));
    Repository::open(&data_directory.join(config::DATABASE_FILE))?.backup(&path)?;
    prune_snapshots(&directory);
    Ok(path)
}

fn prune_snapshots(directory: &Path) {
    const KEPT_SNAPSHOTS: usize = 20;
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    files.sort();
    let excess = files.len().saturating_sub(KEPT_SNAPSHOTS);
    for path in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

fn spawn_codex(request: &RunRequest) -> Result<Child> {
    let executable = std::env::current_exe()?;
    let mut command = Command::new(locate_codex());
    command
        .args([
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--ignore-user-config",
        ])
        .args(["--ignore-rules", "--ephemeral", "-s", "read-only"])
        .args(["--disable", "apps", "--disable", "plugins"])
        .arg("-C")
        .arg(&request.data_directory)
        .args(config_overrides(&executable, &request.data_directory));
    if !request.model.trim().is_empty() {
        command.args(["-m", request.model.trim()]);
    }
    if !request.effort.trim().is_empty() {
        command.args([
            "-c",
            &format!("model_reasoning_effort=\"{}\"", request.effort.trim()),
        ]);
    }
    command
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map_err(|error| {
        Error::Invalid(format!(
            "Could not start Codex ({error}). Install it with `npm i -g @openai/codex` and run `codex login`."
        ))
    })
}

fn config_overrides(executable: &Path, data_directory: &Path) -> Vec<String> {
    let server = format!("mcp_servers.{MCP_SERVER_NAME}");
    let settings: Vec<(String, String)> = vec![
        ("approval_policy".into(), "\"never\"".into()),
        (format!("{server}.command"), toml_literal(executable)),
        (format!("{server}.args"), "[\"--mcp\"]".into()),
        (
            format!("{server}.env"),
            format!("{{ NOTEON_DATA_DIR = {} }}", toml_literal(data_directory)),
        ),
        (format!("{server}.startup_timeout_sec"), "30".into()),
        (
            format!("{server}.default_tools_approval_mode"),
            "\"approve\"".into(),
        ),
    ];
    settings
        .into_iter()
        .flat_map(|(key, value)| ["-c".to_string(), format!("{key}={value}")])
        .collect()
}
fn toml_literal(path: &Path) -> String {
    format!("'{}'", path.display())
}

pub fn locate_codex() -> PathBuf {
    if let Some(path) = std::env::var_os(CODEX_OVERRIDE_ENV) {
        return PathBuf::from(path);
    }
    let directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    for directory in &directories {
        let direct = directory.join("codex.exe");
        if direct.is_file() {
            return direct;
        }
        if let Some(native) = find_npm_native(directory) {
            return native;
        }
    }
    PathBuf::from("codex")
}

fn find_npm_native(npm_directory: &Path) -> Option<PathBuf> {
    let scoped = npm_directory.join("node_modules/@openai/codex/node_modules/@openai");
    std::fs::read_dir(scoped)
        .ok()?
        .flatten()
        .find_map(|package| {
            let vendor = package.path().join("vendor");
            std::fs::read_dir(vendor)
                .ok()?
                .flatten()
                .find_map(|target| {
                    let binary = target.path().join("bin/codex.exe");
                    binary.is_file().then_some(binary)
                })
        })
}

fn read_events(
    stdout: impl std::io::Read,
    sender: &Sender<RunEvent>,
    context: &egui::Context,
) -> Option<String> {
    let mut failure = None;
    for line in BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(message) = failure_message(&event) {
            failure = Some(message);
        }
        if let Some(run_event) = translate_event(&event) {
            let _ = sender.send(run_event);
            context.request_repaint();
        }
    }
    failure
}

fn failure_message(event: &Value) -> Option<String> {
    if event.get("type")?.as_str()? != "turn.failed" {
        return None;
    }
    let raw = event.pointer("/error/message")?.as_str()?;
    let readable = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.pointer("/error/message")?.as_str().map(String::from));
    Some(readable.unwrap_or_else(|| raw.to_string()))
}

fn translate_event(event: &Value) -> Option<RunEvent> {
    let kind = event.get("type")?.as_str()?;
    let item = event.get("item")?;
    let item_kind = item.get("type")?.as_str()?;
    match (kind, item_kind) {
        ("item.completed", "agent_message") => {
            Some(RunEvent::Message(item.get("text")?.as_str()?.to_string()))
        }
        ("item.started", "mcp_tool_call") => {
            if item.get("server").and_then(Value::as_str) == Some(MCP_SERVER_NAME) {
                Some(RunEvent::ToolCall(item.get("tool")?.as_str()?.to_string()))
            } else {
                None
            }
        }
        ("item.started" | "item.completed", "reasoning") => Some(RunEvent::Thinking),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_agent_messages_and_tool_calls() {
        let message = json!({ "type": "item.completed", "item": { "type": "agent_message", "text": "Tamam" } });
        assert!(
            matches!(translate_event(&message), Some(RunEvent::Message(text)) if text == "Tamam")
        );
        let call = json!({ "type": "item.started", "item": { "type": "mcp_tool_call", "server": "noteon", "tool": "add_rows" } });
        assert!(
            matches!(translate_event(&call), Some(RunEvent::ToolCall(name)) if name == "add_rows")
        );
    }

    #[test]
    fn extracts_readable_failure() {
        let event = json!({ "type": "turn.failed", "error": { "message": "{\"error\":{\"message\":\"nope\"}}" } });
        assert_eq!(failure_message(&event).as_deref(), Some("nope"));
    }
}
