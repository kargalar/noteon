pub mod mcp;
pub mod runner;
mod schema;
pub mod tools;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::domain::model::PageId;
use runner::{RunEvent, RunHandle, RunRequest};

pub const MCP_FLAG: &str = "--mcp";
const SETTINGS_FILE: &str = "assistant.json";
const HISTORY_LIMIT: usize = 12;
const LIVE_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
const READ_ONLY_TOOLS: [&str; 4] = ["list_pages", "search_notes", "read_page", "get_database"];
pub const DEFAULT_MODEL: &str = "gpt-6-luna";
pub const DEFAULT_EFFORT: &str = "high";
pub const EFFORT_LEVELS: [&str; 3] = ["low", "medium", "high"];

const SYSTEM_PREAMBLE: &str = "You are the Noteon Assistant, built into the user's local notes app Noteon. \
Use ONLY the `noteon` MCP tools to read and change the workspace (pages, blocks, tables/databases). \
Do not run shell commands and do not touch files directly. \
Reply in the user's language (Turkish if unsure) and keep answers short. \
Workflow: look things up first (list_pages / search_notes / read_page / get_database), then act with bulk tools \
(add_rows, update_rows, insert_content, update_blocks) instead of many single calls, and finally say briefly what you changed. \
Blocks of kind `subpage_link` represent subpages: never delete them unless the user asks. \
'This page' / 'bu sayfa' means the active page given below. A 'table' means a database: \
use create_database with inline_in_page_id to put one inside the active page. \
The user can undo your changes, so act directly without asking for confirmation unless the request is ambiguous or destructive.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Activity,
    Error,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantSettings {
    pub model: String,
    pub effort: String,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            effort: DEFAULT_EFFORT.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodexQuota {
    pub remaining_percent: f32,
    pub resets_at: Option<i64>,
    pub plan_type: Option<String>,
}

impl CodexQuota {
    pub fn compact_label(&self) -> String {
        let remaining = self.remaining_percent.round().clamp(0.0, 100.0) as u32;
        match self.reset_countdown() {
            Some(countdown) => format!("Codex {remaining}% left · resets in {countdown}"),
            None => format!("Codex {remaining}% left"),
        }
    }

    fn reset_countdown(&self) -> Option<String> {
        let resets_at = self.resets_at?;
        let now = chrono::Utc::now().timestamp();
        let remaining_secs = (resets_at - now).max(0);
        let days = remaining_secs / 86_400;
        let hours = (remaining_secs % 86_400) / 3600;
        let minutes = (remaining_secs % 3600) / 60;
        if days > 0 {
            Some(format!("{days}d {hours}h"))
        } else if hours > 0 {
            Some(format!("{hours}h {minutes}m"))
        } else {
            Some(format!("{minutes}m"))
        }
    }
}

#[derive(Clone, Debug)]
pub struct PageContext {
    pub id: PageId,
    pub title: String,
    pub is_database: bool,
}

#[derive(Default)]
pub struct Assistant {
    pub open: bool,
    pub settings_open: bool,
    pub focus_input: bool,
    pub input: String,
    pub messages: Vec<ChatMessage>,
    pub settings: AssistantSettings,
    pub quota: Option<CodexQuota>,
    handle: Option<RunHandle>,
    snapshots: Vec<PathBuf>,
    current_snapshot: Option<PathBuf>,
    changed_data: bool,
    pending_refresh: bool,
    last_refresh: Option<Instant>,
}

impl Assistant {
    pub fn load(data_directory: &Path) -> Self {
        let mut settings: AssistantSettings =
            std::fs::read_to_string(data_directory.join(SETTINGS_FILE))
                .ok()
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_default();
        if settings.model.trim().is_empty() {
            settings.model = DEFAULT_MODEL.to_string();
        }
        if settings.effort.trim().is_empty() {
            settings.effort = DEFAULT_EFFORT.to_string();
        }
        Self {
            settings,
            quota: runner::read_latest_quota(),
            ..Self::default()
        }
    }

    pub fn save_settings(&self, data_directory: &Path) {
        if let Ok(raw) = serde_json::to_string_pretty(&self.settings) {
            let _ = std::fs::write(data_directory.join(SETTINGS_FILE), raw);
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    pub fn can_undo(&self) -> bool {
        !self.snapshots.is_empty() && !self.is_running()
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focus_input = self.open;
    }

    pub fn clear(&mut self) {
        if !self.is_running() {
            self.messages.clear();
        }
    }

    pub fn cancel(&self) {
        if let Some(handle) = &self.handle {
            handle.cancel();
        }
    }

    pub fn submit(
        &mut self,
        context: &egui::Context,
        data_directory: &Path,
        page: Option<PageContext>,
    ) {
        let request = self.input.trim().to_string();
        if request.is_empty() || self.is_running() {
            return;
        }
        let prompt = build_prompt(&self.messages, &request, page.as_ref());
        self.messages.push(ChatMessage {
            role: Role::User,
            text: request,
        });
        self.input.clear();
        self.changed_data = false;
        self.handle = Some(runner::start(RunRequest {
            prompt,
            data_directory: data_directory.to_path_buf(),
            model: self.settings.model.clone(),
            effort: self.settings.effort.clone(),
            context: context.clone(),
        }));
    }

    pub fn pop_undo_snapshot(&mut self) -> Option<PathBuf> {
        if self.is_running() {
            return None;
        }
        self.snapshots.pop()
    }

    pub fn discard_snapshot(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    pub fn note(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(ChatMessage {
            role,
            text: text.into(),
        });
    }

    /// Drains runner events; returns true when the app should reload workspace data.
    pub fn poll(&mut self) -> bool {
        let events: Vec<RunEvent> = self
            .handle
            .as_ref()
            .map(|handle| handle.events.try_iter().collect())
            .unwrap_or_default();
        let mut finished = false;
        for event in events {
            finished |= self.apply_event(event);
        }
        if finished {
            self.handle = None;
            self.pending_refresh = false;
            return true;
        }
        self.live_refresh_due()
    }

    fn live_refresh_due(&mut self) -> bool {
        let due = self.pending_refresh
            && self
                .last_refresh
                .is_none_or(|time| time.elapsed() >= LIVE_REFRESH_INTERVAL);
        if due {
            self.pending_refresh = false;
            self.last_refresh = Some(Instant::now());
        }
        due
    }

    fn apply_event(&mut self, event: RunEvent) -> bool {
        match event {
            RunEvent::SnapshotReady(path) => self.current_snapshot = Some(path),
            RunEvent::Thinking => {}
            RunEvent::ToolCall(name) => self.record_tool_call(&name),
            RunEvent::Message(text) => self.note(Role::Assistant, text),
            RunEvent::Quota(quota) => self.quota = Some(quota),
            RunEvent::Finished(result) => {
                self.finish_run(result);
                if self.quota.is_none()
                    && let Some(quota) = runner::read_latest_quota()
                {
                    self.quota = Some(quota);
                }
                return true;
            }
        }
        false
    }

    fn record_tool_call(&mut self, name: &str) {
        if !READ_ONLY_TOOLS.contains(&name) {
            self.changed_data = true;
            self.pending_refresh = true;
        }
        self.note(Role::Activity, describe_tool(name));
    }

    fn finish_run(&mut self, result: Result<(), String>) {
        if let Err(message) = result {
            self.note(Role::Error, message);
        }
        let Some(snapshot) = self.current_snapshot.take() else {
            return;
        };
        if self.changed_data {
            self.snapshots.push(snapshot);
        } else {
            Self::discard_snapshot(&snapshot);
        }
    }
}

fn build_prompt(history: &[ChatMessage], request: &str, page: Option<&PageContext>) -> String {
    let mut prompt = String::from(SYSTEM_PREAMBLE);
    prompt.push_str("\n\n");
    match page {
        Some(page) => prompt.push_str(&format!(
            "Active page: id={} title={:?} is_database={}\n",
            page.id, page.title, page.is_database
        )),
        None => prompt.push_str("Active page: none\n"),
    }
    let recent: Vec<&ChatMessage> = history
        .iter()
        .filter(|message| matches!(message.role, Role::User | Role::Assistant))
        .collect();
    let skip = recent.len().saturating_sub(HISTORY_LIMIT);
    for message in recent.into_iter().skip(skip) {
        let speaker = if message.role == Role::User {
            "User"
        } else {
            "Assistant"
        };
        prompt.push_str(&format!("\n{speaker}: {}\n", message.text));
    }
    prompt.push_str(&format!("\nUser: {request}\n"));
    prompt
}

fn describe_tool(name: &str) -> String {
    let label = match name {
        "list_pages" => "Browsing pages",
        "search_notes" => "Searching notes",
        "read_page" => "Reading a page",
        "get_database" => "Reading a table",
        "create_page" => "Creating a page",
        "update_page" => "Updating a page",
        "move_page" => "Moving a page",
        "trash_pages" => "Moving pages to trash",
        "restore_pages" => "Restoring pages",
        "duplicate_page" => "Duplicating a page",
        "insert_content" => "Writing content",
        "replace_page_content" => "Rewriting a page",
        "update_blocks" => "Editing blocks",
        "delete_blocks" => "Deleting blocks",
        "move_block" => "Moving a block",
        "set_page_property" => "Setting a page property",
        "create_database" => "Creating a table",
        "add_property" | "update_property" | "delete_property" => "Editing table columns",
        "add_rows" => "Adding rows",
        "update_rows" => "Updating rows",
        "set_database_view" => "Changing table view",
        other => other,
    };
    label.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_active_page_and_recent_history() {
        let history = vec![
            ChatMessage {
                role: Role::User,
                text: "selam".into(),
            },
            ChatMessage {
                role: Role::Activity,
                text: "Reading a page".into(),
            },
            ChatMessage {
                role: Role::Assistant,
                text: "merhaba".into(),
            },
        ];
        let page = PageContext {
            id: 7,
            title: "Plan".into(),
            is_database: false,
        };
        let prompt = build_prompt(&history, "tablo ekle", Some(&page));
        assert!(prompt.contains("id=7"));
        assert!(prompt.contains("User: selam"));
        assert!(!prompt.contains("Reading a page"));
        assert!(prompt.ends_with("User: tablo ekle\n"));
    }

    #[test]
    fn read_only_run_discards_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("s.sqlite");
        std::fs::write(&snapshot, b"x").unwrap();
        let mut assistant = Assistant {
            current_snapshot: Some(snapshot.clone()),
            ..Default::default()
        };
        assistant.finish_run(Ok(()));
        assert!(!snapshot.exists());
        assert!(!assistant.can_undo());
    }
}
