use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde_json::{Value, json};

use crate::client;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusArea {
    Navigation,
    Collection,
    Inspector,
    DetailTabs,
    CommandBar,
}

impl FocusArea {
    pub fn next(self) -> Self {
        match self {
            Self::Navigation => Self::Collection,
            Self::Collection => Self::Inspector,
            Self::Inspector => Self::DetailTabs,
            Self::DetailTabs => Self::CommandBar,
            Self::CommandBar => Self::Navigation,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Navigation => Self::CommandBar,
            Self::Collection => Self::Navigation,
            Self::Inspector => Self::Collection,
            Self::DetailTabs => Self::Inspector,
            Self::CommandBar => Self::DetailTabs,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Collection => "Collection",
            Self::Inspector => "Inspector",
            Self::DetailTabs => "Tabs",
            Self::CommandBar => "Command",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    Full,
    Medium,
    Compact,
    Reduced,
    TooSmall,
}

pub fn layout_mode(width: u16, height: u16) -> LayoutMode {
    if width < 40 || height < 10 {
        LayoutMode::TooSmall
    } else if width < 60 || height < 16 {
        LayoutMode::Reduced
    } else if width < 90 {
        LayoutMode::Compact
    } else if width < 120 {
        LayoutMode::Medium
    } else {
        LayoutMode::Full
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackKind {
    Success,
    Error,
    Info,
}

#[derive(Clone, Debug)]
pub struct CommandFeedback {
    pub kind: FeedbackKind,
    pub message: String,
    pub created_at: Instant,
}

impl CommandFeedback {
    pub fn new(kind: FeedbackKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            created_at: Instant::now(),
        }
    }
}

fn summarize_output(ok: bool, text: &str) -> String {
    if ok && let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        for key in ["id", "name", "state"] {
            if let Some(s) = v.get(key).and_then(Value::as_str) {
                return format!("ok · {s}");
            }
        }
        if let Some(arr) = v.as_array() {
            return format!("ok · {}", arr.len());
        }
    }
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(if ok { "ok" } else { "command failed" })
        .to_string()
}

pub struct App {
    pub home: PathBuf,
    pub no_color: bool,
    pub ascii: bool,
    pub tab: usize,
    pub focus: FocusArea,
    pub workspaces: Vec<Value>,
    pub threads: Vec<Value>,
    pub branches: Vec<Value>,
    pub runs: Vec<Value>,
    pub events: Vec<Value>,
    pub memories: Vec<Value>,
    pub artifacts: Vec<Value>,
    pub mchs: Vec<Value>,
    pub selected_run: Option<Value>,
    pub manifest: Option<Value>,
    pub input: String,
    pub status: String,
    pub status_ok: bool,
    pub feedback: Option<CommandFeedback>,
    pub feedback_ttl: Duration,
    pub help: bool,
    pub filter: Option<String>,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub nav_index: usize,
    pub collection_index: usize,
    pub scroll: usize,
    pub selected_run_id: Option<String>,
    pub selected_branch_id: Option<String>,
    pub daemon: bool,
    last_runs: Instant,
    last_events: Instant,
    last_lists: Instant,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            home: client::home(),
            no_color: std::env::var("NO_COLOR").is_ok(),
            ascii: std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false)
                || std::env::var("HLI_ASCII").is_ok(),
            tab: 0,
            focus: FocusArea::Navigation,
            workspaces: vec![],
            threads: vec![],
            branches: vec![],
            runs: vec![],
            events: vec![],
            memories: vec![],
            artifacts: vec![],
            mchs: vec![],
            selected_run: None,
            manifest: None,
            input: String::new(),
            status: "ready · ? help · : command · q quit".into(),
            status_ok: true,
            feedback: None,
            feedback_ttl: Duration::from_secs(5),
            help: false,
            filter: None,
            history: vec![],
            history_index: None,
            nav_index: 0,
            collection_index: 0,
            scroll: 0,
            selected_run_id: None,
            selected_branch_id: None,
            daemon: false,
            last_runs: Instant::now() - Duration::from_secs(10),
            last_events: Instant::now() - Duration::from_secs(10),
            last_lists: Instant::now() - Duration::from_secs(10),
        };
        app.refresh_lists();
        app.refresh_runs();
        app.refresh_events();
        app
    }

    pub fn demo() -> Self {
        let run = json!({
            "id": "run_demo", "state": "completed", "provider": "ollama", "model": "gemma4:31b",
            "workspace_id": "ws_demo", "thread_id": "thr_demo", "branch_id": "br_main",
            "branch_revision": 5,
            "receipt": {"usage": {"input_tokens": 120, "output_tokens": 8, "latency_total_ms": 540},
                        "cost": {"total": 0.0001, "currency": "USD"}}
        });
        Self {
            home: PathBuf::from("."),
            no_color: false,
            ascii: false,
            tab: 0,
            focus: FocusArea::Navigation,
            workspaces: vec![json!({"id": "ws_demo", "name": "HLI-Core"})],
            threads: vec![json!({"id": "thr_demo", "title": "Architecture"})],
            branches: vec![
                json!({"id": "br_main", "name": "main", "head_sequence": 5}),
                json!({"id": "br_adapt", "name": "adaptive-k", "head_sequence": 9}),
            ],
            runs: vec![run.clone()],
            events: vec![
                json!({"sequence": 4, "type": "context.manifest.created", "actor": {"kind":"agent"}, "created_at": "2026-11-07T10:00:00Z"}),
                json!({"sequence": 5, "type": "model.requested", "actor": {"kind":"agent"}, "created_at": "2026-11-07T10:00:01Z"}),
            ],
            memories: vec![
                json!({"id": "mem_1", "status": "active", "content": "SQLite es la fuente de verdad.", "source_authority": "user", "superseded_at": null}),
                json!({"id": "mem_0", "status": "superseded", "content": "PostgreSQL era la fuente.", "source_authority": "legacy", "superseded_at": "2026-11-07T09:00:00Z"}),
            ],
            artifacts: vec![
                json!({"id": "art_1", "mime": "text/plain", "bytes": 42, "sha256": "sha256:abc"}),
            ],
            mchs: vec![
                json!({"name": "memory-main", "profile": "memory", "state": "ready", "module": "hli-memory", "domain": "memory", "workspace_id": "ws_demo"}),
            ],
            selected_run: Some(run),
            manifest: Some(json!({
                "budget_tokens": 2048, "estimated_tokens": 120, "payload_hash": "sha256:deadbeef",
                "user_input": {"content": "¿Qué base usamos?"},
                "memories": [{"id": "mem_1", "content": "SQLite es la fuente de verdad."}],
                "artifact_slices": [{"slice_id": "sl_1", "reason": "error window"}],
                "excluded": [{"id": "mem_9", "reason": "budget"}],
                "recent_events": [{"event_id": "evt_4", "type": "context.manifest.created"}]
            })),
            input: String::new(),
            status: "demo".into(),
            status_ok: true,
            feedback: None,
            feedback_ttl: Duration::from_secs(5),
            help: false,
            filter: None,
            history: vec![],
            history_index: None,
            nav_index: 0,
            collection_index: 0,
            scroll: 0,
            selected_run_id: Some("run_demo".into()),
            selected_branch_id: Some("br_main".into()),
            daemon: true,
            last_runs: Instant::now(),
            last_events: Instant::now(),
            last_lists: Instant::now(),
        }
    }

    pub fn empty() -> Self {
        let mut app = Self::demo();
        app.workspaces.clear();
        app.threads.clear();
        app.branches.clear();
        app.runs.clear();
        app.events.clear();
        app.memories.clear();
        app.artifacts.clear();
        app.mchs.clear();
        app.selected_run = None;
        app.manifest = None;
        app
    }

    pub fn refresh_lists(&mut self) {
        self.daemon = client::call("daemon.ping", json!({})).is_ok();
        self.workspaces = arr(client::call("workspace.list", json!({})));
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.threads = arr(client::call("thread.list", json!({"workspace_id": id})));
            self.memories = arr(client::call("memory.list", json!({"workspace_id": id})));
            self.artifacts = arr(client::call("artifact.list", json!({"workspace_id": id})));
            if let Some(th) = self.threads.first() {
                self.branches = arr(client::call("branch.list", json!({"thread_id": th["id"]})));
            }
        }
        self.mchs = arr(client::call("mch.list", json!({})));
        self.last_lists = Instant::now();
    }

    pub fn refresh_runs(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.runs = arr(client::call("run.list", json!({"workspace_id": id})));
        }
        if self.runs.is_empty() {
            self.selected_run = None;
            self.manifest = None;
            self.selected_run_id = None;
            self.collection_index = 0;
            self.last_runs = Instant::now();
            return;
        }
        let idx = self
            .selected_run_id
            .as_ref()
            .and_then(|id| self.runs.iter().position(|r| r["id"].as_str() == Some(id)))
            .unwrap_or(0);
        self.collection_index = idx;
        let run = self.runs.get(idx).cloned();
        if let Some(id) = run.as_ref().and_then(|r| r["id"].as_str()) {
            self.selected_run_id = Some(id.to_string());
            if let Ok(got) = client::call("run.get", json!({"run_id": id})) {
                self.selected_run = Some(got["run"].clone());
                self.manifest = Some(got["manifest"].clone());
            }
        }
        self.last_runs = Instant::now();
    }

    pub fn refresh_events(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.events = arr(client::call("event.list", json!({"workspace_id": id})));
        }
        self.last_events = Instant::now();
    }

    pub fn tick(&mut self) {
        if self.last_runs.elapsed() >= Duration::from_millis(500) {
            self.refresh_runs();
        }
        if self.last_events.elapsed() >= Duration::from_secs(1) {
            self.refresh_events();
        }
        if self.last_lists.elapsed() >= Duration::from_secs(4) {
            self.refresh_lists();
        }
        if self
            .feedback
            .as_ref()
            .is_some_and(|f| f.created_at.elapsed() >= self.feedback_ttl)
        {
            self.feedback = None;
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focus = self.focus.next();
    }
    pub fn cycle_focus_back(&mut self) {
        self.focus = self.focus.prev();
    }
    pub fn next_tab(&mut self) {
        self.tab = (self.tab + 1) % 6;
        self.scroll = 0;
    }
    pub fn prev_tab(&mut self) {
        self.tab = (self.tab + 5) % 6;
        self.scroll = 0;
    }
    pub fn scroll_down(&mut self, max: usize) {
        if self.scroll + 1 < max {
            self.scroll += 1;
        }
    }
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
    pub fn open_help(&mut self) {
        self.help = true;
    }
    pub fn close_help(&mut self) {
        self.help = false;
    }
    pub fn start_command(&mut self) {
        self.focus = FocusArea::CommandBar;
        self.clear_feedback();
    }
    pub fn cancel_command(&mut self) {
        self.input.clear();
        self.history_index = None;
        self.focus = FocusArea::Navigation;
        self.clear_feedback();
    }

    pub fn set_feedback(&mut self, kind: FeedbackKind, message: impl Into<String>) {
        self.feedback = Some(CommandFeedback::new(kind, message));
    }

    pub fn clear_feedback(&mut self) {
        self.feedback = None;
    }

    /// Feedback vigente: existe y no ha superado el TTL.
    pub fn visible_feedback(&self) -> Option<&CommandFeedback> {
        self.feedback
            .as_ref()
            .filter(|f| f.created_at.elapsed() < self.feedback_ttl)
    }

    /// Deriva el feedback de la salida de un comando.
    pub fn apply_feedback(&mut self, ok: bool, text: &str) {
        let kind = if ok {
            FeedbackKind::Success
        } else {
            FeedbackKind::Error
        };
        self.set_feedback(kind, summarize_output(ok, text));
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let i = match self.history_index {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_index = Some(i);
        self.input = self.history[i].clone();
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
            }
        }
    }

    pub fn run_command(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
        if let Some(f) = line.strip_prefix('/') {
            self.filter = Some(f.trim().to_string());
            self.status = format!(
                "filter: {}",
                if f.trim().is_empty() {
                    "(none)"
                } else {
                    f.trim()
                }
            );
            self.status_ok = true;
            self.input.clear();
            self.focus = FocusArea::Navigation;
            return;
        }
        self.history.push(line.clone());
        self.history_index = None;
        let mut args = split_command(&line);
        if args.is_empty() {
            return;
        }
        if args[0] == "workspace" && args.get(1).map(String::as_str) == Some("open") {
            args[1] = "get".into();
        }
        let out = std::process::Command::new(client::program())
            .args(&args)
            .env("HLI_HOME", &self.home)
            .output();
        match out {
            Ok(o) => {
                let ok = o.status.success();
                let text = if ok {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                };
                self.finish_command(ok, &text);
            }
            Err(e) => {
                self.finish_command(false, &format!("error: {e}"));
            }
        }
    }

    /// Estado posterior a la ejecución de un comando: conserva el resultado en
    /// `feedback`, limpia la entrada y devuelve el foco a la navegación.
    pub fn finish_command(&mut self, ok: bool, text: &str) {
        self.status_ok = ok;
        self.status = text.lines().next().unwrap_or("ok").to_string();
        self.apply_feedback(ok, text);
        self.input.clear();
        self.focus = FocusArea::Navigation;
        self.refresh_lists();
        self.refresh_runs();
    }

    /// Procesa un evento de teclado. Devuelve `true` si la TUI debe cerrarse.
    ///
    /// Solo se atienden `Press` y `Repeat`: los eventos `Release` que Windows
    /// entrega junto a cada pulsación no deben ejecutar la acción dos veces.
    #[allow(clippy::collapsible_match)]
    pub fn handle_key(&mut self, k: KeyEvent) -> bool {
        if !matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }
        let in_command = self.focus == FocusArea::CommandBar;
        match k.code {
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('q') => {
                if self.help {
                    self.close_help();
                } else if in_command {
                    self.input.push('q');
                    self.clear_feedback();
                } else {
                    return true;
                }
            }
            KeyCode::Char('?') => {
                self.help = !self.help;
            }
            KeyCode::Esc => {
                if self.help {
                    self.close_help();
                } else if in_command {
                    self.cancel_command();
                } else {
                    self.filter = None;
                }
                self.clear_feedback();
            }
            KeyCode::Tab => {
                if !in_command {
                    self.cycle_focus();
                }
            }
            KeyCode::BackTab => {
                if !in_command {
                    self.cycle_focus_back();
                }
            }
            KeyCode::Char(':') => {
                self.start_command();
            }
            KeyCode::Char('/') => {
                self.input = "/".into();
                self.start_command();
            }
            KeyCode::Char(c @ '1'..='6') if !in_command => {
                self.tab = (c as usize) - ('1' as usize);
                self.focus = FocusArea::DetailTabs;
            }
            KeyCode::Enter => {
                if in_command {
                    self.run_command();
                } else {
                    self.focus = FocusArea::Inspector;
                }
            }
            KeyCode::Backspace => {
                if in_command {
                    self.input.pop();
                }
            }
            KeyCode::Up => match self.focus {
                FocusArea::Navigation => self.nav_index = self.nav_index.saturating_sub(1),
                FocusArea::Collection => {
                    self.collection_index = self.collection_index.saturating_sub(1)
                }
                FocusArea::CommandBar => self.history_up(),
                _ => self.scroll_up(),
            },
            KeyCode::Down => match self.focus {
                FocusArea::Navigation => self.nav_index += 1,
                FocusArea::Collection => {
                    if self.collection_index + 1 < self.runs.len() {
                        self.collection_index += 1;
                    }
                }
                FocusArea::CommandBar => self.history_down(),
                _ => self.scroll_down(self.events.len()),
            },
            KeyCode::Left => self.prev_tab(),
            KeyCode::Right => self.next_tab(),
            KeyCode::Char(c) => {
                if in_command {
                    self.input.push(c);
                    self.clear_feedback();
                }
            }
            _ => {}
        }
        false
    }

    pub fn selected_index(&self) -> usize {
        self.selected_run_id
            .as_ref()
            .and_then(|id| self.runs.iter().position(|r| r["id"].as_str() == Some(id)))
            .unwrap_or(0)
    }

    pub fn symbol(&self, unicode: &str, ascii: &str) -> String {
        if self.ascii {
            ascii.to_string()
        } else {
            unicode.to_string()
        }
    }
}

fn arr(v: Result<Value, String>) -> Vec<Value> {
    v.ok()
        .and_then(|x| x.as_array().cloned())
        .unwrap_or_default()
}

/// Divide respetando comillas; el parser real sigue siendo el de hli-cli.
pub fn split_command(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, ' ') => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
