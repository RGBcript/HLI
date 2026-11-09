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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiMode {
    Agent,
    Inspector,
}

impl TuiMode {
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            Some("inspect") => Self::Inspector,
            _ => Self::Agent,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
    User,
    Hli,
    Tool,
    Error,
    Info,
}

#[derive(Clone, Debug)]
pub struct AgentLine {
    pub kind: AgentKind,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    None,
    CreateWorkspace { name: String },
    CreateBranch { name: String },
}

#[derive(Clone, Debug)]
pub struct AgentState {
    pub root: Option<PathBuf>,
    pub workspace: Option<Value>,
    pub thread: Option<Value>,
    pub branch: Option<Value>,
    pub lines: Vec<AgentLine>,
    pub composer: String,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub palette: bool,
    pub palette_index: usize,
    pub offer: Offer,
    pub pending: Option<String>,
    pub busy: bool,
    pub status: String,
    pub provider: String,
    pub model: String,
    pub run_state: String,
    pub last_run: Option<String>,
    pub last_seq: i64,
    pub receipt_shown: bool,
    pub poll_at: Instant,
    pub prompts: Vec<String>,
    pub history_open: bool,
    pub history_query: String,
    pub history_sel: usize,
    pub resume_open: bool,
    pub resume_index: usize,
    pub resume_items: Vec<ResumeItem>,
}

#[derive(Clone, Debug)]
pub struct ResumeItem {
    pub workspace_id: String,
    pub workspace_name: String,
    pub thread_id: String,
    pub title: String,
    pub branch: String,
    pub state: String,
    pub model: String,
    pub last_activity: String,
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            root: None,
            workspace: None,
            thread: None,
            branch: None,
            lines: Vec::new(),
            composer: String::new(),
            history: Vec::new(),
            history_index: None,
            palette: false,
            palette_index: 0,
            offer: Offer::None,
            pending: None,
            busy: false,
            status: "ready · ? help · / commands · ^O inspector".into(),
            provider: "sim".into(),
            model: "sim-1".into(),
            run_state: "idle".into(),
            last_run: None,
            last_seq: 0,
            receipt_shown: false,
            poll_at: Instant::now(),
            prompts: Vec::new(),
            history_open: false,
            history_query: String::new(),
            history_sel: 0,
            resume_open: false,
            resume_index: 0,
            resume_items: Vec::new(),
        }
    }
}

impl AgentState {
    pub fn push(&mut self, kind: AgentKind, text: impl Into<String>) {
        self.lines.push(AgentLine {
            kind,
            text: text.into(),
        });
    }

    pub fn needs_thread(&self) -> bool {
        self.thread.is_none()
    }

    pub fn needs_branch(&self) -> bool {
        self.branch.is_none()
    }
}

pub struct PromptPlan {
    pub create_thread: bool,
    pub create_branch: bool,
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
    pub mode: TuiMode,
    pub agent: AgentState,
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
            mode: TuiMode::Inspector,
            agent: AgentState::default(),
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
            mode: TuiMode::Inspector,
            agent: AgentState::default(),
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
        if self.mode == TuiMode::Agent {
            self.poll_agent();
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
        self.input.clear();
        self.focus = FocusArea::Navigation;
        self.refresh_lists();
        self.refresh_runs();
        // El feedback se fija al final: un refresco lento no debe consumir su TTL.
        self.apply_feedback(ok, text);
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
        if self.mode == TuiMode::Agent {
            return self.handle_agent_key(k);
        }
        let in_command = self.focus == FocusArea::CommandBar;
        match k.code {
            KeyCode::Char('o') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_mode();
            }
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
                } else if self.agent.workspace.is_some() {
                    // Sesión de Agent Mode en curso: Esc vuelve a la conversación.
                    self.mode = TuiMode::Agent;
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

// ---------------------------------------------------------------------------
// Slice 3a: transcripción durable, resume e historial
// ---------------------------------------------------------------------------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn iso_to_secs(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let h: i64 = s.get(11..13)?.parse().ok()?;
    let mi: i64 = s.get(14..16)?.parse().ok()?;
    let se: i64 = s.get(17..19)?.parse().ok()?;
    Some(days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + se)
}

fn duration_secs(run: &Value) -> Option<f64> {
    let a = iso_to_secs(run["started_at"].as_str()?)?;
    let b = iso_to_secs(run["ended_at"].as_str()?)?;
    Some((b - a).max(0) as f64)
}

/// Bloque de resultado terminal: estado, duración, provider, tokens, coste,
/// receipt y artifacts. No usa la palabra "completed" si el estado no lo es.
pub fn terminal_summary(run: &Value) -> String {
    let state = run["state"].as_str().unwrap_or("?");
    let mut lines = vec![match duration_secs(run) {
        Some(d) => format!("{state} in {d:.1} s"),
        None => state.to_string(),
    }];
    let r = &run["receipt"];
    if r.is_object() {
        lines.push(format!(
            "Provider: {}/{}",
            r["provider"].as_str().unwrap_or("-"),
            r["model"].as_str().unwrap_or("-")
        ));
        let tin = r["usage"]["input_tokens"].as_i64().unwrap_or(0);
        let tout = r["usage"]["output_tokens"].as_i64().unwrap_or(0);
        lines.push(format!("Tokens: {tin} in / {tout} out"));
        let total = r["cost"]["total"].as_f64().unwrap_or(0.0);
        lines.push(if total > 0.0 {
            format!("Cost: ${total:.6}")
        } else {
            "Cost: unpriced".to_string()
        });
        let hash = r["result_hash"].as_str().unwrap_or("-");
        let short: String = hash.chars().take(16).collect();
        lines.push(format!(
            "Receipt: {short} · manifest {}",
            r["context_manifest"].as_str().unwrap_or("-")
        ));
        let arts: Vec<String> = r["result_artifacts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !arts.is_empty() {
            lines.push(format!("Artifacts: {}", arts.join(", ")));
        }
    }
    if let Some(err) = run["result"]["error"].as_str() {
        lines.push(format!("Error: {err}"));
    }
    lines.join("\n")
}

/// Proyecta el transcript de un thread desde runs y eventos durables.
///
/// Orden: runs por `started_at`; dentro de cada run, eventos por `sequence`.
/// Es puro y determinista: proyectar dos veces el mismo dato da lo mismo.
pub fn project_thread(runs: &[Value], events: &[Value]) -> Vec<AgentLine> {
    let mut ordered: Vec<&Value> = runs.iter().collect();
    ordered.sort_by(|a, b| {
        a["started_at"]
            .as_str()
            .unwrap_or("")
            .cmp(b["started_at"].as_str().unwrap_or(""))
    });
    let mut out = Vec::new();
    for run in ordered {
        let id = run["id"].as_str().unwrap_or("");
        if let Some(prompt) = run["prompt"].as_str()
            && !prompt.is_empty()
        {
            out.push(AgentLine {
                kind: AgentKind::User,
                text: prompt.to_string(),
            });
        }
        let mut evs: Vec<&Value> = events
            .iter()
            .filter(|e| e["run_id"].as_str() == Some(id))
            .collect();
        evs.sort_by_key(|e| e["sequence"].as_i64().unwrap_or(0));
        for e in evs {
            if let Some(line) = event_line(e) {
                out.push(AgentLine {
                    kind: AgentKind::Tool,
                    text: line,
                });
            }
        }
        if let Some(text) = run["result"]["text"].as_str()
            && !text.trim().is_empty()
        {
            out.push(AgentLine {
                kind: AgentKind::Hli,
                text: text.to_string(),
            });
        }
        let ok = run["state"].as_str() == Some("completed");
        out.push(AgentLine {
            kind: if ok {
                AgentKind::Info
            } else {
                AgentKind::Error
            },
            text: terminal_summary(run),
        });
    }
    out
}

impl App {
    /// Reconstruye el transcript del thread activo desde el Event Store.
    pub fn load_transcript(&mut self) {
        let Some(ws) = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["id"].as_str())
            .map(str::to_string)
        else {
            return;
        };
        let Some(th) = self
            .agent
            .thread
            .as_ref()
            .and_then(|t| t["id"].as_str())
            .map(str::to_string)
        else {
            return;
        };
        let mut runs = client::call("run.list", json!({ "workspace_id": ws }))
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        runs.retain(|r| r["thread_id"].as_str() == Some(th.as_str()));
        runs.sort_by(|a, b| {
            a["started_at"]
                .as_str()
                .unwrap_or("")
                .cmp(b["started_at"].as_str().unwrap_or(""))
        });
        let mut args = json!({ "workspace_id": ws });
        if let Some(br) = self.agent.branch.as_ref().and_then(|b| b["id"].as_str()) {
            args["branch_id"] = json!(br);
        }
        let events = client::call("event.list", args)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        self.agent.prompts = runs
            .iter()
            .filter_map(|r| r["prompt"].as_str().map(str::to_string))
            .collect();
        self.agent.lines = project_thread(&runs, &events);
        self.agent.last_seq = events
            .iter()
            .filter_map(|e| e["sequence"].as_i64())
            .max()
            .unwrap_or(0);
    }

    /// Carga los threads recientes (con su último run) para el selector.
    pub fn open_resume(&mut self) {
        self.agent.resume_items.clear();
        self.agent.resume_index = 0;
        self.agent.resume_open = true;
        let Some(list) = client::call("workspace.list", json!({})).ok() else {
            return;
        };
        for ws in list.as_array().cloned().unwrap_or_default().iter().take(5) {
            let wid = ws["id"].as_str().unwrap_or("");
            let wname = ws["name"].as_str().unwrap_or("-").to_string();
            let threads = client::call("thread.list", json!({ "workspace_id": wid }))
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            let runs = client::call("run.list", json!({ "workspace_id": wid }))
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            for th in threads.iter().rev().take(8) {
                let tid = th["id"].as_str().unwrap_or("").to_string();
                let title = th["title"].as_str().unwrap_or("-").to_string();
                let branches = client::call("branch.list", json!({ "thread_id": tid }))
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                let br = branches.last().cloned().unwrap_or(Value::Null);
                let last = runs
                    .iter()
                    .filter(|r| r["thread_id"].as_str() == Some(tid.as_str()))
                    .max_by_key(|r| r["started_at"].as_str().unwrap_or("").to_string());
                let (state, model, activity) = match last {
                    Some(r) => (
                        r["state"].as_str().unwrap_or("-").to_string(),
                        format!(
                            "{}/{}",
                            r["receipt"]["provider"].as_str().unwrap_or("-"),
                            r["receipt"]["model"].as_str().unwrap_or("-")
                        ),
                        r["started_at"].as_str().unwrap_or("-").to_string(),
                    ),
                    None => ("-".into(), "-".into(), "-".into()),
                };
                self.agent.resume_items.push(ResumeItem {
                    workspace_id: wid.to_string(),
                    workspace_name: wname.clone(),
                    thread_id: tid,
                    title,
                    branch: br["name"].as_str().unwrap_or("-").to_string(),
                    state,
                    model,
                    last_activity: activity,
                });
            }
        }
        self.agent
            .resume_items
            .sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    }

    /// Recupera el thread seleccionado sin crear nada nuevo.
    pub fn resume_select(&mut self) {
        let Some(item) = self
            .agent
            .resume_items
            .get(self.agent.resume_index)
            .cloned()
        else {
            return;
        };
        self.agent.workspace = client::call(
            "workspace.get",
            json!({ "workspace_id": item.workspace_id }),
        )
        .ok();
        self.agent.thread = None;
        self.agent.branch = None;
        self.load_threads();
        if let Some(t) = self
            .threads
            .iter()
            .find(|t| t["id"].as_str() == Some(item.thread_id.as_str()))
            .cloned()
        {
            self.agent.thread = Some(t);
            self.load_branches();
            self.agent.branch = self
                .branches
                .iter()
                .find(|b| b["name"].as_str() == Some(item.branch.as_str()))
                .cloned()
                .or_else(|| self.branches.last().cloned());
            self.load_transcript();
            self.agent.push(
                AgentKind::Info,
                format!("Resumed: {} · {}", item.workspace_name, item.title),
            );
        }
        self.agent.resume_open = false;
        self.mode = TuiMode::Agent;
        self.agent.status = self.agent_status_line();
    }

    pub fn history_matches(&self) -> Vec<String> {
        let q = self.agent.history_query.to_lowercase();
        self.agent
            .prompts
            .iter()
            .filter(|p| q.is_empty() || p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn history_choose(&mut self) {
        let matches = self.history_matches();
        if let Some(p) = matches.get(self.agent.history_sel).cloned() {
            self.agent.composer = p;
        }
        self.agent.history_open = false;
    }

    #[allow(clippy::collapsible_match)]
    fn handle_history_key(&mut self, k: KeyEvent) -> bool {
        let n = self.history_matches().len();
        match k.code {
            KeyCode::Esc => {
                self.agent.history_open = false;
                self.agent.history_query.clear();
            }
            KeyCode::Enter => self.history_choose(),
            KeyCode::Up => {
                self.agent.history_sel = self.agent.history_sel.saturating_sub(1);
            }
            KeyCode::Down => {
                if n > 0 && self.agent.history_sel + 1 < n {
                    self.agent.history_sel += 1;
                }
            }
            KeyCode::Char('r') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                if n > 0 {
                    self.agent.history_sel = (self.agent.history_sel + 1) % n;
                }
            }
            KeyCode::Backspace => {
                self.agent.history_query.pop();
                self.agent.history_sel = 0;
            }
            KeyCode::Char(c) => {
                self.agent.history_query.push(c);
                self.agent.history_sel = 0;
            }
            _ => {}
        }
        false
    }

    #[allow(clippy::collapsible_match)]
    fn handle_resume_key(&mut self, k: KeyEvent) -> bool {
        let n = self.agent.resume_items.len();
        match k.code {
            KeyCode::Esc => self.agent.resume_open = false,
            KeyCode::Enter => self.resume_select(),
            KeyCode::Up => {
                self.agent.resume_index = self.agent.resume_index.saturating_sub(1);
            }
            KeyCode::Down => {
                if n > 0 && self.agent.resume_index + 1 < n {
                    self.agent.resume_index += 1;
                }
            }
            _ => {}
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Agent Mode
// ---------------------------------------------------------------------------

pub const SLASH_COMMANDS: [&str; 16] = [
    "help",
    "status",
    "inspect",
    "context",
    "memory",
    "artifacts",
    "mch",
    "branch",
    "new",
    "resume",
    "model",
    "provider",
    "permissions",
    "diff",
    "clear",
    "exit",
];

fn normalize_str(s: &str) -> String {
    s.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn normalize_path(p: &std::path::Path) -> String {
    normalize_str(&p.to_string_lossy())
}

fn dir_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "workspace".into())
}

pub fn is_terminal(state: &str) -> bool {
    matches!(
        state,
        "completed" | "failed" | "cancelled" | "timed_out" | "interrupted"
    )
}

pub fn is_active(state: &str) -> bool {
    matches!(
        state,
        "queued" | "running" | "cancelling" | "waiting_approval" | "paused"
    )
}

fn event_line(e: &Value) -> Option<String> {
    let kind = e["type"].as_str()?;
    if !(kind.starts_with("tool.")
        || kind.starts_with("run.")
        || kind.starts_with("model.")
        || kind.starts_with("context."))
    {
        return None;
    }
    let detail = e["payload"]["tool"]
        .as_str()
        .or_else(|| e["payload"]["model"].as_str())
        .unwrap_or("");
    Some(if detail.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} · {detail}")
    })
}

impl App {
    pub fn begin_agent(&mut self, root: Option<PathBuf>) {
        self.mode = TuiMode::Agent;
        self.agent = AgentState::default();
        self.agent.root = root.or_else(|| std::env::current_dir().ok());
        self.agent.push(
            AgentKind::Info,
            "Agent Mode · escribe la tarea y pulsa Enter",
        );
        self.link_directory();
    }

    pub fn link_directory(&mut self) {
        let Some(root) = self.agent.root.clone() else {
            return;
        };
        let list = client::call("workspace.list", json!({})).unwrap_or(Value::Null);
        let items = list.as_array().cloned().unwrap_or_default();
        let want = normalize_path(&root);
        let found = items
            .iter()
            .find(|w| w["root_path"].as_str().map(normalize_str).as_deref() == Some(want.as_str()))
            .cloned();
        match found {
            Some(w) => {
                let name = w["name"].as_str().unwrap_or("-").to_string();
                let id = w["id"].as_str().unwrap_or("-").to_string();
                self.agent.workspace = Some(w);
                self.agent
                    .push(AgentKind::Info, format!("Linked workspace: {name} ({id})"));
                self.load_threads();
                self.pick_last_thread();
                self.load_transcript();
                self.agent.offer = Offer::None;
            }
            None => {
                self.agent.offer = Offer::CreateWorkspace {
                    name: dir_name(&root),
                };
                self.agent.push(
                    AgentKind::Info,
                    "No HLI workspace is linked to this directory.",
                );
            }
        }
        self.agent.status = self.agent_status_line();
    }

    pub fn agent_status_line(&self) -> String {
        let ws = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["name"].as_str())
            .unwrap_or("no-workspace");
        let th = self
            .agent
            .thread
            .as_ref()
            .and_then(|t| t["title"].as_str())
            .unwrap_or("no-thread");
        let br = self
            .agent
            .branch
            .as_ref()
            .and_then(|b| b["name"].as_str())
            .unwrap_or("no-branch");
        format!(
            "{ws}/{th}/{br} │ {}/{} │ {} │ ctx - │ tools 0 │ unpriced",
            self.agent.provider, self.agent.model, self.agent.run_state
        )
    }

    pub fn load_threads(&mut self) {
        let Some(ws) = self.agent.workspace.clone() else {
            return;
        };
        let id = ws["id"].as_str().unwrap_or("").to_string();
        let list =
            client::call("thread.list", json!({ "workspace_id": id })).unwrap_or(Value::Null);
        self.threads = list.as_array().cloned().unwrap_or_default();
        self.workspaces = vec![ws];
    }

    pub fn pick_last_thread(&mut self) {
        if let Some(t) = self.threads.last().cloned() {
            self.agent.thread = Some(t);
            self.load_branches();
            self.pick_last_branch();
        }
    }

    pub fn load_branches(&mut self) {
        let Some(t) = self.agent.thread.clone() else {
            return;
        };
        let id = t["id"].as_str().unwrap_or("").to_string();
        let list = client::call("branch.list", json!({ "thread_id": id })).unwrap_or(Value::Null);
        self.branches = list.as_array().cloned().unwrap_or_default();
    }

    pub fn pick_last_branch(&mut self) {
        if let Some(b) = self.branches.last().cloned() {
            self.agent.branch = Some(b);
            self.detect_provider();
        }
    }

    fn detect_provider(&mut self) {
        let Some(ws) = self.agent.workspace.clone() else {
            return;
        };
        let id = ws["id"].as_str().unwrap_or("").to_string();
        let list = client::call("run.list", json!({ "workspace_id": id })).unwrap_or(Value::Null);
        let runs = list.as_array().cloned().unwrap_or_default();
        if let Some(run) = runs.iter().rev().find(|r| {
            r["receipt"]["provider"]
                .as_str()
                .is_some_and(|p| !p.is_empty())
        }) {
            if let Some(p) = run["receipt"]["provider"].as_str() {
                self.agent.provider = p.to_string();
            }
            if let Some(m) = run["receipt"]["model"].as_str() {
                self.agent.model = m.to_string();
            }
        }
    }

    pub fn plan_prompt(&self) -> PromptPlan {
        PromptPlan {
            create_thread: self.agent.needs_thread(),
            create_branch: self.agent.needs_branch(),
        }
    }

    pub fn accept_offer(&mut self) -> bool {
        match self.agent.offer.clone() {
            Offer::CreateWorkspace { name } => {
                let root = self
                    .agent
                    .root
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".into());
                match client::call("workspace.create", json!({ "name": name, "root": root })) {
                    Ok(w) => {
                        self.agent.workspace = Some(w.clone());
                        self.workspaces = vec![w.clone()];
                        self.agent.offer = Offer::None;
                        self.agent.push(
                            AgentKind::Info,
                            format!("Workspace created: {}", w["id"].as_str().unwrap_or("-")),
                        );
                        self.load_threads();
                        if let Some(p) = self.agent.pending.take() {
                            self.agent.push(AgentKind::User, p.clone());
                            self.run_prompt(&p);
                        }
                    }
                    Err(e) => self.agent.push(AgentKind::Error, e),
                }
                true
            }
            Offer::CreateBranch { name } => {
                let Some(t) = self.agent.thread.clone() else {
                    self.agent.offer = Offer::None;
                    return false;
                };
                let tid = t["id"].as_str().unwrap_or("").to_string();
                match client::call("branch.create", json!({ "thread_id": tid, "name": name })) {
                    Ok(b) => {
                        self.agent.branch = Some(b.clone());
                        self.branches.push(b.clone());
                        self.agent.offer = Offer::None;
                        self.agent.push(
                            AgentKind::Info,
                            format!("Created branch: {}", b["name"].as_str().unwrap_or("-")),
                        );
                    }
                    Err(e) => self.agent.push(AgentKind::Error, e),
                }
                true
            }
            Offer::None => false,
        }
    }

    pub fn submit_prompt(&mut self) -> bool {
        let text = self.agent.composer.trim().to_string();
        if text.is_empty() {
            return false;
        }
        if self.agent.workspace.is_none() {
            self.agent.pending = Some(text);
            if self.agent.offer == Offer::None {
                self.agent.offer = Offer::CreateWorkspace {
                    name: self
                        .agent
                        .root
                        .as_ref()
                        .map(|p| dir_name(p))
                        .unwrap_or_else(|| "workspace".into()),
                };
            }
            return false;
        }
        self.agent.history.push(text.clone());
        self.agent.history_index = None;
        self.agent.composer.clear();
        self.agent.push(AgentKind::User, text.clone());
        self.run_prompt(&text);
        true
    }

    fn ensure_thread(&mut self, text: &str) -> Option<String> {
        if let Some(t) = self.agent.thread.as_ref().and_then(|t| t["id"].as_str()) {
            return Some(t.to_string());
        }
        let ws = self
            .agent
            .workspace
            .as_ref()?
            .get("id")?
            .as_str()?
            .to_string();
        let title: String = text.chars().take(48).collect();
        match client::call(
            "thread.create",
            json!({ "workspace_id": ws, "title": title }),
        ) {
            Ok(t) => {
                let id = t["id"].as_str().unwrap_or("").to_string();
                self.agent.thread = Some(t.clone());
                self.threads.push(t);
                self.agent.push(
                    AgentKind::Tool,
                    format!(
                        "Created thread: {}",
                        self.agent
                            .thread
                            .as_ref()
                            .and_then(|t| t["id"].as_str())
                            .unwrap_or("-")
                    ),
                );
                Some(id)
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, e);
                None
            }
        }
    }

    fn ensure_branch(&mut self) -> Option<String> {
        if let Some(b) = self.agent.branch.as_ref().and_then(|b| b["id"].as_str()) {
            return Some(b.to_string());
        }
        let tid = self.agent.thread.as_ref()?.get("id")?.as_str()?.to_string();
        match client::call("branch.create", json!({ "thread_id": tid, "name": "main" })) {
            Ok(b) => {
                let id = b["id"].as_str().unwrap_or("").to_string();
                self.agent.branch = Some(b.clone());
                self.branches.push(b);
                self.agent
                    .push(AgentKind::Tool, "Created branch: main".to_string());
                Some(id)
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, e);
                None
            }
        }
    }

    pub fn run_prompt(&mut self, text: &str) {
        let Some(ws) = self.agent.workspace.as_ref().and_then(|w| w["id"].as_str()) else {
            return;
        };
        let ws = ws.to_string();
        let Some(th) = self.ensure_thread(text) else {
            return;
        };
        let Some(br) = self.ensure_branch() else {
            return;
        };
        let mut args = json!({
            "workspace_id": ws,
            "thread_id": th,
            "branch_id": br,
            "prompt": text,
            "async": true,
        });
        if !self.agent.provider.is_empty() {
            args["provider"] = json!(self.agent.provider);
            args["model"] = json!(self.agent.model);
        }
        if self.agent.provider == "sim"
            && let Ok(ms) = std::env::var("HLI_SIM_DELAY_MS")
            && let Ok(ms) = ms.parse::<u64>()
        {
            args["sim_delay_ms"] = json!(ms);
        }
        self.agent.busy = true;
        self.agent.run_state = "starting".into();
        match client::call("run.start", args) {
            Ok(run) => {
                let id = run["id"].as_str().unwrap_or("-").to_string();
                let state = run["state"].as_str().unwrap_or("queued").to_string();
                let provider = run["receipt"]["provider"]
                    .as_str()
                    .unwrap_or(&self.agent.provider)
                    .to_string();
                let model = run["receipt"]["model"]
                    .as_str()
                    .unwrap_or(&self.agent.model)
                    .to_string();
                self.agent.last_run = Some(id.clone());
                self.agent.last_seq = 0;
                self.agent.receipt_shown = false;
                self.agent.run_state = state.clone();
                self.agent.poll_at = Instant::now();
                self.agent.push(
                    AgentKind::Hli,
                    format!("Run {id} · {state} · {provider}/{model}"),
                );
                if is_terminal(&state) {
                    self.apply_run_status(&json!({ "run": run }));
                }
            }
            Err(e) => {
                self.agent.run_state = "failed".into();
                self.agent.push(AgentKind::Error, e);
            }
        }
        self.agent.busy = false;
        self.agent.status = self.agent_status_line();
        self.refresh_lists();
        self.refresh_runs();
    }

    pub fn apply_run_status(&mut self, v: &Value) {
        let run = v.get("run").unwrap_or(v);
        let state = run["state"].as_str().unwrap_or("").to_string();
        if state.is_empty() {
            return;
        }
        let changed = state != self.agent.run_state;
        self.agent.run_state = state.clone();
        if changed {
            self.agent.push(AgentKind::Info, format!("run {state}"));
        }
        if is_terminal(&state) && !self.agent.receipt_shown {
            self.agent.receipt_shown = true;
            let ok = state == "completed";
            self.agent.push(
                if ok { AgentKind::Hli } else { AgentKind::Error },
                terminal_summary(run),
            );
        }
        self.agent.status = self.agent_status_line();
    }

    /// Añade al transcript los eventos nuevos del run actual. Devuelve cuántos.
    pub fn ingest_events(&mut self, events: &[Value]) -> usize {
        let Some(run) = self.agent.last_run.clone() else {
            return 0;
        };
        let mut added = 0;
        let mut max_seq = self.agent.last_seq;
        for e in events {
            if e["run_id"].as_str() != Some(run.as_str()) {
                continue;
            }
            let seq = e["sequence"].as_i64().unwrap_or(0);
            if seq <= self.agent.last_seq {
                continue;
            }
            if let Some(line) = event_line(e)
                && added < 4
            {
                self.agent.push(AgentKind::Tool, line);
                added += 1;
            }
            max_seq = max_seq.max(seq);
        }
        self.agent.last_seq = max_seq;
        added
    }

    fn poll_agent(&mut self) {
        let Some(id) = self.agent.last_run.clone() else {
            return;
        };
        if is_terminal(&self.agent.run_state) {
            return;
        }
        if self.agent.poll_at.elapsed() < Duration::from_millis(350) {
            return;
        }
        self.agent.poll_at = Instant::now();
        if let Ok(v) = client::call("run.status", json!({ "run_id": id })) {
            self.apply_run_status(&v);
        }
        let ws = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["id"].as_str())
            .map(str::to_string);
        let br = self
            .agent
            .branch
            .as_ref()
            .and_then(|b| b["id"].as_str())
            .map(str::to_string);
        if let Some(ws) = ws {
            let mut args = json!({ "workspace_id": ws });
            if let Some(br) = br {
                args["branch_id"] = json!(br);
            }
            if let Ok(list) = client::call("event.list", args)
                && let Some(arr) = list.as_array()
            {
                self.ingest_events(arr);
            }
        }
    }

    pub fn mark_cancelling(&mut self) {
        if !is_terminal(&self.agent.run_state) && self.agent.last_run.is_some() {
            self.agent.run_state = "cancelling".into();
            self.agent.push(AgentKind::Info, "cancel requested");
        }
    }

    pub fn request_cancel(&mut self) {
        if let Some(id) = self.agent.last_run.clone() {
            self.mark_cancelling();
            let _ = client::call("run.cancel", json!({ "run_id": id }));
        }
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            TuiMode::Agent => {
                self.agent.palette = false;
                self.help = false;
                TuiMode::Inspector
            }
            TuiMode::Inspector => TuiMode::Agent,
        };
    }

    pub fn palette_items(&self) -> Vec<&'static str> {
        let needle = self.agent.composer.trim_start_matches('/').to_lowercase();
        SLASH_COMMANDS
            .iter()
            .copied()
            .filter(|c| needle.is_empty() || c.starts_with(&needle))
            .collect()
    }

    pub fn run_slash(&mut self, line: &str) -> bool {
        let cmd = line
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        self.agent.palette = false;
        self.agent.composer.clear();
        match cmd.as_str() {
            "" => {}
            "help" => self.help = true,
            "inspect" => self.toggle_mode(),
            "context" => {
                self.mode = TuiMode::Inspector;
                self.tab = 2;
            }
            "memory" => {
                self.mode = TuiMode::Inspector;
                self.tab = 3;
            }
            "artifacts" => {
                self.mode = TuiMode::Inspector;
                self.tab = 4;
            }
            "mch" => {
                self.mode = TuiMode::Inspector;
                self.tab = 5;
            }
            "clear" => self.agent.lines.clear(),
            "cancel" => self.request_cancel(),
            "exit" => return true,
            "new" => {
                let ws = self
                    .agent
                    .workspace
                    .as_ref()
                    .and_then(|w| w["id"].as_str())
                    .map(str::to_string);
                if let Some(ws) = ws {
                    match client::call(
                        "thread.create",
                        json!({ "workspace_id": ws, "title": "session" }),
                    ) {
                        Ok(t) => {
                            self.agent.thread = Some(t.clone());
                            self.threads.push(t);
                            self.agent.branch = None;
                            self.agent.push(AgentKind::Info, "New thread".to_string());
                        }
                        Err(e) => self.agent.push(AgentKind::Error, e),
                    }
                }
            }
            "branch" => {
                self.agent.offer = Offer::CreateBranch {
                    name: format!("alternative-{}", self.branches.len() + 1),
                };
            }
            "status" => {
                let s = self.agent_status_line();
                self.agent.push(AgentKind::Info, s);
            }
            other => self.agent.push(
                AgentKind::Info,
                format!("/{other}: no disponible en este slice"),
            ),
        }
        false
    }

    fn agent_history_up(&mut self) {
        if self.agent.history.is_empty() {
            return;
        }
        let i = match self.agent.history_index {
            None => self.agent.history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.agent.history_index = Some(i);
        self.agent.composer = self.agent.history[i].clone();
    }

    fn agent_history_down(&mut self) {
        match self.agent.history_index {
            Some(i) if i + 1 < self.agent.history.len() => {
                self.agent.history_index = Some(i + 1);
                self.agent.composer = self.agent.history[i + 1].clone();
            }
            Some(_) => {
                self.agent.history_index = None;
                self.agent.composer.clear();
            }
            None => {}
        }
    }

    pub fn handle_agent_key(&mut self, k: KeyEvent) -> bool {
        if !matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }
        if self.agent.resume_open {
            return self.handle_resume_key(k);
        }
        if self.agent.history_open {
            return self.handle_history_key(k);
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        match k.code {
            KeyCode::Char('c') if ctrl => {
                if is_active(&self.agent.run_state) && self.agent.last_run.is_some() {
                    self.request_cancel();
                } else if !self.agent.composer.is_empty() {
                    self.agent.composer.clear();
                } else {
                    return true;
                }
            }
            KeyCode::Char('d') if ctrl => {
                if self.agent.composer.is_empty() {
                    return true;
                }
            }
            KeyCode::Char('o') if ctrl => self.toggle_mode(),
            KeyCode::Char('j') if ctrl => self.agent.composer.push('\n'),
            KeyCode::Enter if shift => self.agent.composer.push('\n'),
            KeyCode::Enter => {
                if self.agent.offer != Offer::None {
                    self.accept_offer();
                } else if self.agent.palette {
                    let line = self.agent.composer.clone();
                    return self.run_slash(&line);
                } else {
                    self.submit_prompt();
                }
            }
            KeyCode::Esc => {
                if self.agent.palette {
                    self.agent.palette = false;
                    self.agent.composer.clear();
                } else if self.agent.offer != Offer::None {
                    self.agent.offer = Offer::None;
                    self.agent.pending = None;
                } else {
                    self.agent.composer.clear();
                }
                self.help = false;
            }
            KeyCode::Up => {
                if !self.agent.palette && self.agent.composer.is_empty() {
                    self.agent_history_up();
                }
            }
            KeyCode::Down => {
                if !self.agent.palette && self.agent.composer.is_empty() {
                    self.agent_history_down();
                }
            }
            KeyCode::Char('r') if ctrl => {
                self.agent.history_open = true;
                self.agent.history_query.clear();
                self.agent.history_sel = 0;
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('/') => {
                self.agent.palette = true;
                self.agent.composer.push('/');
            }
            KeyCode::Char('@') => self.agent.composer.push('@'),
            KeyCode::Backspace => {
                self.agent.composer.pop();
                if !self.agent.composer.starts_with('/') {
                    self.agent.palette = false;
                }
            }
            KeyCode::Char(c) => self.agent.composer.push(c),
            _ => {}
        }
        false
    }
}
