use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::viewport::{TranscriptViewport, wheel_speed_from_env, wrapped_lines};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

/// Primeras decisiones, siempre visibles y recorribles.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AgentTab {
    Workspace,
    Session,
    #[default]
    Chat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiMode {
    Agent,
    Inspector,
    /// Revisión de la conversación a pantalla completa, separada del
    /// Inspector (que administra runs, contexto y memoria).
    Transcript,
    /// Vista de configuración: no es una pestaña, se abre con `/settings`.
    Settings,
}

impl TuiMode {
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            Some("inspect") => Self::Inspector,
            Some("settings") => Self::Settings,
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

#[derive(Clone, Debug, Default)]
pub struct ActivityDetail {
    pub kind: String,
    pub status: String,
    pub started: String,
    pub duration: Option<f64>,
    pub input: String,
    pub output: String,
    pub causation: Option<String>,
    pub correlation: Option<String>,
    pub artifacts: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AgentLine {
    pub kind: AgentKind,
    pub text: String,
    pub detail: Option<ActivityDetail>,
    pub expanded: bool,
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
    pub details_all: bool,
    pub scroll_top: Option<usize>,
    pub new_activity: usize,
    pub activity_sel: Option<usize>,
    pub composer: String,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub palette: bool,
    /// `/clear` exige dos invocaciones seguidas: borrar el transcript no debe
    /// depender de una tecla mal pulsada.
    pub clear_armed: Option<Instant>,
    pub palette_index: usize,
    pub agent_tab: AgentTab,
    pub ws_sel: usize,
    pub thread_sel: usize,
    pub model_open: bool,
    pub model_sel: usize,
    pub model_loading: bool,
    pub lists_loading: bool,
    pub transcript_loading: bool,
    pub settings_sel: usize,
    pub settings_cat: usize,
    pub settings_loading: bool,
    pub settings_providers: Vec<Value>,
    pub settings_modules: Vec<Value>,
    pub settings_mchs: Vec<Value>,
    pub settings_prices: usize,
    pub daemon_closed: bool,
    pub providers: Vec<Value>,
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
    pub references: Vec<Reference>,
    pub ref_open: bool,
    pub ref_query: String,
    pub ref_index: usize,
    pub ref_category: usize,
    pub ref_items: Vec<Vec<RefItem>>,
    pub ref_start: usize,
    pub preflight: Option<Preflight>,
    pub viewport: TranscriptViewport,
    pub approval_kind: Option<String>,
    pub plan: Option<PlanView>,
    pub perm: Option<PermissionView>,
    pub reject_open: bool,
    pub reject_reason: String,
}

#[derive(Clone, Debug)]
pub struct PreflightIssue {
    pub index: usize,
    pub stable_id: String,
    pub reason: String,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    pub refreshable: bool,
}

/// Estado de una memoria para preflight (puro, sin I/O).
pub fn classify_memory(
    status: &str,
    valid_from: Option<&str>,
    valid_until: Option<&str>,
    now: i64,
) -> Option<&'static str> {
    if status != "active" {
        return match status {
            "superseded" => Some("superseded"),
            "revoked" => Some("revoked"),
            _ => Some("not active"),
        };
    }
    if let Some(v) = valid_from {
        match iso_to_secs(v) {
            Some(t) if t <= now => {}
            Some(_) => return Some("not yet valid"),
            None => return Some("invalid timestamp"),
        }
    }
    if let Some(v) = valid_until {
        match iso_to_secs(v) {
            Some(t) if t > now => {}
            Some(_) => return Some("expired"),
            None => return Some("invalid timestamp"),
        }
    }
    None
}

pub fn classify_artifact_hash(
    expected: Option<&str>,
    current: Option<&str>,
) -> Option<&'static str> {
    match (expected, current) {
        (_, None) => Some("not found"),
        (Some(w), Some(c))
            if w.trim_start_matches("sha256:") != c.trim_start_matches("sha256:") =>
        {
            Some("hash mismatch")
        }
        _ => None,
    }
}

pub fn classify_branch_revision(expected: Option<i64>, head: i64) -> Option<&'static str> {
    match expected {
        Some(rev) if rev != head => Some("revision mismatch"),
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct TranscriptReview {
    pub scroll: usize,
    pub query: String,
    pub search_open: bool,
    pub matches: Vec<usize>,
    pub match_index: usize,
    pub saved_scroll: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Preflight {
    pub prompt: String,
    pub issues: Vec<PreflightIssue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    File,
    Artifact,
    Memory,
    Run,
    Branch,
}

impl RefKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Artifact => "artifact",
            Self::Memory => "memory",
            Self::Run => "run",
            Self::Branch => "branch",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::File => "Files",
            Self::Artifact => "Artifacts",
            Self::Memory => "Memories",
            Self::Run => "Runs",
            Self::Branch => "Branches",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub kind: RefKind,
    pub stable_id: String,
    pub display: String,
    pub content_hash: Option<String>,
    pub revision: Option<i64>,
    pub size: Option<u64>,
    pub selected_at: i64,
}

#[derive(Clone, Debug)]
pub struct RefItem {
    pub kind: RefKind,
    pub stable_id: String,
    pub display: String,
    pub subtitle: String,
    pub content_hash: Option<String>,
    pub revision: Option<i64>,
    pub size: Option<u64>,
}

pub fn reference_token(r: &Reference) -> String {
    format!("@{}:{}", r.kind.label(), r.stable_id)
}

/// Vista read-only del plan durable para Agent Mode e Inspector.
#[derive(Clone, Debug, Default)]
pub struct PlanView {
    pub run_id: String,
    pub plan_id: String,
    pub revision: i64,
    pub supersedes: Option<String>,
    pub origin: String,
    pub status: String,
    pub summary: String,
    pub steps: Vec<PlanStepView>,
    pub effects: Vec<String>,
    pub invalidation_reasons: Vec<String>,
    pub plan_hash: String,
    pub execution_hash: String,
    pub provider: String,
    pub model: String,
    pub manifest_id: String,
    pub branch_revision: i64,
    pub tools: Vec<String>,
    pub modules: Vec<String>,
    pub mch: Vec<String>,
    pub secrets: Vec<String>,
    pub approval_scope: Option<String>,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub details: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlanStepView {
    pub order: i64,
    pub description: String,
    pub effect: String,
    pub requires_approval: bool,
    pub tool: Option<String>,
}

impl PlanView {
    pub fn from_value(v: &Value) -> Option<Self> {
        let plan = v.get("plan")?;
        if plan.is_null() {
            return None;
        }
        let exec = v.get("execution").cloned().unwrap_or(Value::Null);
        let steps = plan["steps"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|s| PlanStepView {
                        order: s["order"].as_i64().unwrap_or(0),
                        description: s["description"].as_str().unwrap_or("").to_string(),
                        effect: s["effect"].as_str().unwrap_or("unknown").to_string(),
                        requires_approval: s["requires_approval"].as_bool().unwrap_or(false),
                        tool: s["tool_name"].as_str().map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let list = |v: Option<&Value>| -> Vec<String> {
            v.and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        Some(Self {
            run_id: plan["run_id"].as_str().unwrap_or("").to_string(),
            plan_id: plan["plan_id"].as_str().unwrap_or("").to_string(),
            revision: plan["revision"].as_i64().unwrap_or(1),
            supersedes: plan["supersedes_plan_id"].as_str().map(str::to_string),
            origin: plan["origin"].as_str().unwrap_or("-").to_string(),
            status: plan["status"].as_str().unwrap_or("-").to_string(),
            summary: plan["summary"].as_str().unwrap_or("").to_string(),
            steps,
            effects: list(plan.get("requested_effects")),
            invalidation_reasons: list(plan.get("invalidation_reasons")),
            plan_hash: plan["content_hash"].as_str().unwrap_or("-").to_string(),
            execution_hash: exec["content_hash"].as_str().unwrap_or("-").to_string(),
            provider: format!(
                "{} ({})",
                exec["provider_id"].as_str().unwrap_or("-"),
                exec["provider_kind"].as_str().unwrap_or("-")
            ),
            model: exec["model"].as_str().unwrap_or("-").to_string(),
            manifest_id: plan["context_manifest_id"]
                .as_str()
                .unwrap_or("-")
                .to_string(),
            branch_revision: plan["branch_revision"].as_i64().unwrap_or(0),
            tools: exec["tool_policy"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or("")))
                        .collect()
                })
                .unwrap_or_default(),
            modules: exec["module_versions"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|m| {
                            format!(
                                "{} {}",
                                m["name"].as_str().unwrap_or("-"),
                                m["version"].as_str().unwrap_or("")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            mch: exec["mch_instances"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|m| {
                            format!(
                                "{} ({})",
                                m["name"].as_str().unwrap_or("-"),
                                m["profile"].as_str().unwrap_or("-")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            secrets: exec["secret_handles"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|h| {
                            format!(
                                "{}@{}",
                                h["id"].as_str().unwrap_or("-"),
                                h["version"].as_str().unwrap_or("-")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            approval_scope: plan["approval_scope"].as_str().map(str::to_string),
            approved_by: plan["approved_by"].as_str().map(str::to_string),
            created_at: plan["created_at"].as_str().unwrap_or("-").to_string(),
            decided_at: plan["approved_at"]
                .as_str()
                .or_else(|| plan["rejected_at"].as_str())
                .map(str::to_string),
            details: false,
        })
    }

    pub fn waiting(&self) -> bool {
        self.status == "waiting_approval"
    }

    pub fn invalidated(&self) -> bool {
        !self.invalidation_reasons.is_empty() && self.status == "waiting_approval"
    }

    /// Texto del overlay; nunca incluye valores de secreto.
    pub fn overlay_lines(&self) -> Vec<String> {
        let mut out = vec![
            "Plan requires approval".to_string(),
            String::new(),
            format!("Run: {}", self.run_id),
            format!("Plan: {}", self.plan_id),
            format!("Revision: {}", self.revision),
            format!("Origin: {}", self.origin.replace('_', "-")),
            String::new(),
        ];
        for s in &self.steps {
            out.push(format!(
                "{}. {:<40} {}{}",
                s.order,
                s.description,
                s.effect,
                if s.requires_approval {
                    " · approval required"
                } else {
                    ""
                }
            ));
        }
        out.push(String::new());
        out.push(format!("Requested effects: {}", self.effects.join(", ")));
        if !self.invalidation_reasons.is_empty() {
            out.push(String::new());
            out.push("Plan invalidated:".to_string());
            for r in &self.invalidation_reasons {
                out.push(format!("  · {r}"));
            }
        }
        if self.details {
            out.push(String::new());
            out.extend(self.detail_lines());
        }
        out.push(String::new());
        if self.invalidated() {
            out.push("[r] Replan   [v] View details   [Esc] Cancel approval".to_string());
        } else {
            out.push(
                "[y] Approve once   [a] This run   [w] This workspace   [n] Reject   [v] Details   [r] Replan"
                    .to_string(),
            );
            out.push("[Esc] Cancel approval".to_string());
        }
        out
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut out = vec![
            format!("plan hash:        {}", self.plan_hash),
            format!("snapshot hash:    {}", self.execution_hash),
            format!("provider/model:   {} / {}", self.provider, self.model),
            format!("manifest:         {}", self.manifest_id),
            format!("branch revision:  {}", self.branch_revision),
            format!(
                "tool policy:      {}",
                if self.tools.is_empty() {
                    "-".into()
                } else {
                    self.tools.join(", ")
                }
            ),
            format!(
                "modules:          {}",
                if self.modules.is_empty() {
                    "-".into()
                } else {
                    self.modules.join(", ")
                }
            ),
            format!(
                "MCH instances:    {}",
                if self.mch.is_empty() {
                    "-".into()
                } else {
                    self.mch.join(", ")
                }
            ),
            format!(
                "secret handles:   {}",
                if self.secrets.is_empty() {
                    "-".into()
                } else {
                    self.secrets.join(", ")
                }
            ),
            format!("created:          {}", self.created_at),
        ];
        if let Some(s) = &self.supersedes {
            out.push(format!("supersedes:       {s}"));
        }
        if let Some(s) = &self.approval_scope {
            out.push(format!("approval scope:   {s}"));
        }
        if let Some(s) = &self.approved_by {
            out.push(format!("approved by:      {s}"));
        }
        if let Some(s) = &self.decided_at {
            out.push(format!("decided at:       {s}"));
        }
        out
    }
}

/// Vista read-only de una solicitud de permiso por invocación.
#[derive(Clone, Debug, Default)]
pub struct PermissionView {
    pub request_id: String,
    pub run_id: String,
    pub plan_id: String,
    pub plan_revision: i64,
    pub tool: String,
    pub operation: String,
    pub effect: String,
    pub target: Option<String>,
    pub arguments_hash: String,
    pub arguments_summary: String,
    pub reason: String,
    pub status: String,
    pub decision: Option<String>,
    pub decision_scope: Option<String>,
    pub actor: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub effective_policy: String,
    pub grants: Vec<String>,
    pub details: bool,
}

impl PermissionView {
    pub fn from_value(v: &Value) -> Option<Self> {
        let req = v.get("request").unwrap_or(v);
        if req.is_null() || req.get("request_id").is_none() {
            return None;
        }
        Some(Self {
            request_id: req["request_id"].as_str().unwrap_or("").to_string(),
            run_id: req["run_id"].as_str().unwrap_or("").to_string(),
            plan_id: req["plan_id"].as_str().unwrap_or("").to_string(),
            plan_revision: req["plan_revision"].as_i64().unwrap_or(0),
            tool: req["tool_name"].as_str().unwrap_or("-").to_string(),
            operation: req["operation"].as_str().unwrap_or("-").to_string(),
            effect: req["effect"].as_str().unwrap_or("unknown").to_string(),
            target: req["target"].as_str().map(str::to_string),
            arguments_hash: req["arguments_hash"].as_str().unwrap_or("-").to_string(),
            arguments_summary: req["arguments_summary"].as_str().unwrap_or("-").to_string(),
            reason: req["reason"].as_str().unwrap_or("").to_string(),
            status: req["status"].as_str().unwrap_or("-").to_string(),
            decision: req["decision"].as_str().map(str::to_string),
            decision_scope: req["decision_scope"].as_str().map(str::to_string),
            actor: req["decided_by"].as_str().map(str::to_string),
            created_at: req["created_at"].as_str().unwrap_or("-").to_string(),
            decided_at: req["decided_at"].as_str().map(str::to_string),
            effective_policy: String::new(),
            grants: vec![],
            details: false,
        })
    }

    pub fn pending(&self) -> bool {
        self.status == "pending"
    }

    pub fn overlay_lines(&self) -> Vec<String> {
        let mut out = vec![
            "Permission required".to_string(),
            String::new(),
            format!("Request: {}", self.request_id),
            format!("Tool: {}", self.tool),
            format!("Operation: {}", self.operation),
            format!(
                "Target: {}",
                self.target.clone().unwrap_or_else(|| "(sin target)".into())
            ),
            format!("Effect: {}", self.effect),
            format!("Reason: {}", self.reason),
            format!("Plan: {} revision {}", self.plan_id, self.plan_revision),
        ];
        if self.details {
            out.push(String::new());
            out.extend(self.detail_lines());
        }
        out.push(String::new());
        out.push(
            "[y] Allow once   [a] This run   [w] This workspace   [n] Deny   [v] Details   [Esc] Cancel"
                .to_string(),
        );
        out
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut out = vec![
            format!("arguments_hash:   {}", self.arguments_hash),
            format!("arguments:        {}", self.arguments_summary),
            format!(
                "target:           {}",
                self.target.clone().unwrap_or_else(|| "-".into())
            ),
            format!(
                "effective policy: {}",
                if self.effective_policy.is_empty() {
                    "ask (sin grant aplicable)"
                } else {
                    &self.effective_policy
                }
            ),
            format!(
                "existing grants:  {}",
                if self.grants.is_empty() {
                    "-".to_string()
                } else {
                    self.grants.join(", ")
                }
            ),
            format!("plan effect:      {}", self.effect),
            format!("causation:        {}", self.request_id),
            format!("correlation:      {}", self.run_id),
            format!("status:           {}", self.status),
            format!("created:          {}", self.created_at),
        ];
        if let Some(d) = &self.decision {
            out.push(format!("decision:         {d}"));
        }
        if let Some(s) = &self.decision_scope {
            out.push(format!("decision scope:   {s}"));
        }
        if let Some(a) = &self.actor {
            out.push(format!("actor:            {a}"));
        }
        if let Some(t) = &self.decided_at {
            out.push(format!("decided at:       {t}"));
        }
        out
    }
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
            details_all: false,
            scroll_top: None,
            new_activity: 0,
            activity_sel: None,
            composer: String::new(),
            history: Vec::new(),
            history_index: None,
            palette: false,
            clear_armed: None,
            palette_index: 0,
            agent_tab: AgentTab::default(),
            ws_sel: 0,
            thread_sel: 0,
            model_open: false,
            model_sel: 0,
            model_loading: false,
            lists_loading: false,
            transcript_loading: false,
            settings_sel: 0,
            settings_cat: 0,
            settings_loading: false,
            settings_providers: Vec::new(),
            settings_modules: Vec::new(),
            settings_mchs: Vec::new(),
            settings_prices: 0,
            daemon_closed: false,
            providers: Vec::new(),
            offer: Offer::None,
            pending: None,
            busy: false,
            status: String::new(),
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
            references: Vec::new(),
            ref_open: false,
            ref_query: String::new(),
            ref_index: 0,
            ref_category: 0,
            ref_items: vec![Vec::new(); 5],
            ref_start: 0,
            preflight: None,
            viewport: TranscriptViewport {
                wheel_speed: wheel_speed_from_env(),
                ..TranscriptViewport::default()
            },
            approval_kind: None,
            plan: None,
            perm: None,
            reject_open: false,
            reject_reason: String::new(),
        }
    }
}

impl AgentState {
    /// Techo del transcript en memoria. El Event Store conserva todo; al
    /// recargar el thread se reconstruye. Sin esto, una sesión de días crece
    /// sin límite y el dibujo se degrada.
    const MAX_LINES: usize = 5_000;
    const PRUNE_LINES: usize = 1_000;

    fn push_line(&mut self, line: AgentLine) {
        if self.lines.len() >= Self::MAX_LINES {
            self.lines.drain(0..Self::PRUNE_LINES);
        }
        self.lines.push(line);
    }

    pub fn push(&mut self, kind: AgentKind, text: impl Into<String>) {
        self.note_push();
        self.push_line(AgentLine {
            kind,
            text: sanitize(&text.into()),
            detail: None,
            expanded: false,
        });
    }

    pub fn push_activity(&mut self, text: impl Into<String>, detail: ActivityDetail) {
        self.note_push();
        let expanded = self.details_all;
        self.push_line(AgentLine {
            kind: AgentKind::Tool,
            text: text.into(),
            detail: Some(detail),
            expanded,
        });
        self.activity_sel = Some(self.lines.len() - 1);
    }

    fn note_push(&mut self) {
        // El modelo cuenta las novedades sólo cuando el usuario está revisando
        // historia; con follow activo la ventana baja sola.
        self.viewport.on_content(1);
        if self.scroll_top.is_some() {
            self.new_activity += 1;
        }
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
    pub daemon_async: crate::daemon_async::AsyncDaemon,
    pub branches: Vec<Value>,
    pub runs: Vec<Value>,
    pub events: Vec<Value>,
    pub memories: Vec<Value>,
    pub artifacts: Vec<Value>,
    pub mchs: Vec<Value>,
    pub selected_run: Option<Value>,
    pub manifest: Option<Value>,
    /// Plan durable del run seleccionado (Inspector, sólo lectura).
    pub inspector_plan: Option<PlanView>,
    pub inspector_perm: Option<PermissionView>,
    pub input: String,
    pub status: String,
    pub status_ok: bool,
    pub feedback: Option<CommandFeedback>,
    pub feedback_ttl: Duration,
    pub mode: TuiMode,
    pub agent: AgentState,
    pub transcript: TranscriptReview,
    pub mouse_enabled: bool,
    /// Salida del comando de statusline configurado en `HLI_HOME/ui.toml`.
    pub statusline_cache: Option<String>,
    pub statusline_at: Instant,
    pub spinner_at: Instant,
    pub terminal_width: usize,
    pub terminal_height: usize,
    pub transcript_top: usize,
    /// Filas renderizadas → índice de línea lógica.
    pub hit_rows: Vec<(usize, usize)>,
    pub sel: Option<usize>,
    pub sel_end: Option<usize>,
    pub transcript_width: usize,
    pub transcript_height: usize,
    /// Petición de volcar el transcript al scrollback (se atiende en main).
    pub transcript_dump: bool,
    /// Ruta de exportación solicitada desde Transcript Mode.
    pub transcript_export: Option<std::path::PathBuf>,
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
            daemon_async: crate::daemon_async::AsyncDaemon::new(),
            branches: vec![],
            runs: vec![],
            events: vec![],
            memories: vec![],
            artifacts: vec![],
            mchs: vec![],
            selected_run: None,
            manifest: None,
            inspector_plan: None,
            inspector_perm: None,
            input: String::new(),
            status: String::new(),
            status_ok: true,
            feedback: None,
            feedback_ttl: Duration::from_secs(5),
            mode: TuiMode::Inspector,
            agent: AgentState::default(),
            transcript: TranscriptReview::default(),
            mouse_enabled: crate::terminal::mouse_enabled_from_env(),
            statusline_cache: None,
            statusline_at: Instant::now() - Duration::from_secs(60),
            spinner_at: Instant::now(),
            terminal_width: 80,
            terminal_height: 30,
            transcript_top: 3,
            hit_rows: Vec::new(),
            sel: None,
            sel_end: None,
            transcript_width: 80,
            transcript_height: 20,
            transcript_dump: false,
            transcript_export: None,
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
            daemon_async: crate::daemon_async::AsyncDaemon::new(),
            branches: vec![
                json!({"id": "br_main", "name": "main", "head_sequence": 5}),
                json!({"id": "br_adapt", "name": "adaptive-k", "head_sequence": 9}),
            ],
            runs: vec![run.clone()],
            inspector_plan: None,
            inspector_perm: None,
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
            status: String::new(),
            status_ok: true,
            feedback: None,
            feedback_ttl: Duration::from_secs(5),
            mode: TuiMode::Inspector,
            agent: AgentState::default(),
            transcript: TranscriptReview::default(),
            mouse_enabled: crate::terminal::mouse_enabled_from_env(),
            statusline_cache: None,
            statusline_at: Instant::now() - Duration::from_secs(60),
            spinner_at: Instant::now(),
            terminal_width: 80,
            terminal_height: 30,
            transcript_top: 3,
            hit_rows: Vec::new(),
            sel: None,
            sel_end: None,
            transcript_width: 80,
            transcript_height: 20,
            transcript_dump: false,
            transcript_export: None,
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
        self.daemon = client::call_ui("daemon.ping", json!({})).is_ok();
        self.workspaces = arr(client::call_ui("workspace.list", json!({})));
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.threads = arr(client::call_ui("thread.list", json!({"workspace_id": id})));
            self.memories = arr(client::call_ui("memory.list", json!({"workspace_id": id})));
            self.artifacts = arr(client::call_ui(
                "artifact.list",
                json!({"workspace_id": id}),
            ));
            if let Some(th) = self.threads.first() {
                self.branches = arr(client::call_ui(
                    "branch.list",
                    json!({"thread_id": th["id"]}),
                ));
            }
        }
        self.mchs = arr(client::call_ui("mch.list", json!({})));
        self.last_lists = Instant::now();
    }

    pub fn refresh_runs(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.runs = arr(client::call_ui("run.list", json!({"workspace_id": id})));
        }
        if self.runs.is_empty() {
            self.selected_run = None;
            self.manifest = None;
            self.inspector_plan = None;
            self.inspector_perm = None;
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
            if let Ok(got) = client::call_ui("run.get", json!({"run_id": id})) {
                self.selected_run = Some(got["run"].clone());
                self.manifest = Some(got["manifest"].clone());
            }
            // El plan del Inspector también se lee del daemon, nunca de SQLite.
            self.inspector_plan = client::call_ui("run.plan", json!({"run_id": id}))
                .ok()
                .and_then(|v| PlanView::from_value(&v));
            self.inspector_perm = client::call_ui("permission.list", json!({"run_id": id}))
                .ok()
                .and_then(|v| v.as_array().and_then(|a| a.last()).cloned())
                .and_then(|v| PermissionView::from_value(&v));
        }
        self.last_runs = Instant::now();
    }

    pub fn refresh_events(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.events = arr(client::call_ui("event.list", json!({"workspace_id": id})));
        }
        self.last_events = Instant::now();
    }

    pub fn tick(&mut self) {
        self.drain_daemon();
        // El refresco del Inspector solo corre cuando se está mirando: en
        // Agent o Settings haría 8 llamadas síncronas cada 4 s y congelaría
        // el dibujo.
        if self.mode == TuiMode::Inspector {
            if self.last_runs.elapsed() >= Duration::from_millis(500) {
                self.refresh_runs();
            }
            if self.last_events.elapsed() >= Duration::from_secs(1) {
                self.refresh_events();
            }
            if self.last_lists.elapsed() >= Duration::from_secs(4) {
                self.refresh_lists();
            }
        }
        if self.mode == TuiMode::Agent {
            self.poll_agent();
        }
        if self.statusline_at.elapsed() >= Duration::from_secs(5) {
            self.statusline_at = Instant::now();
            self.refresh_statusline();
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
        if self.mode == TuiMode::Transcript {
            return self.transcript_key(k);
        }
        if self.mode == TuiMode::Agent {
            return self.handle_agent_key(k);
        }
        if self.mode == TuiMode::Settings {
            return self.settings_key(k);
        }
        let in_command = self.focus == FocusArea::CommandBar;
        // Acciones del Inspector sobre el plan seleccionado: reutilizan las
        // mismas operaciones del daemon que el overlay de Agent Mode.
        if !in_command
            && let Some(perm) = self.inspector_perm.clone()
            && perm.pending()
        {
            match k.code {
                KeyCode::Char('y') => {
                    self.agent.perm = Some(perm);
                    self.approve_permission("once");
                    self.refresh_runs();
                }
                KeyCode::Char('a') => {
                    self.agent.perm = Some(perm);
                    self.approve_permission("run");
                    self.refresh_runs();
                }
                KeyCode::Char('w') => {
                    self.agent.perm = Some(perm);
                    self.approve_permission("workspace");
                    self.refresh_runs();
                }
                KeyCode::Char('n') => {
                    self.agent.perm = Some(perm);
                    self.deny_permission();
                    self.refresh_runs();
                }
                KeyCode::Char('v') => {
                    if let Some(mut p) = self.inspector_perm.clone() {
                        p.details = !p.details;
                        self.inspector_perm = Some(p);
                    }
                }
                _ => {}
            }
            return false;
        }
        if !in_command
            && let Some(plan) = self.inspector_plan.clone()
            && plan.waiting()
        {
            match k.code {
                KeyCode::Char('y') if !plan.invalidated() => self.approve_plan("once"),
                KeyCode::Char('a') if !plan.invalidated() => self.approve_plan("run"),
                KeyCode::Char('w') if !plan.invalidated() => self.approve_plan("workspace"),
                KeyCode::Char('n') => self.reject_plan("rejected_from_inspector"),
                KeyCode::Char('r') if plan.invalidated() => self.replan_plan(),
                KeyCode::Char('v') => {
                    if let Some(mut p) = self.inspector_plan.clone() {
                        p.details = !p.details;
                        self.inspector_plan = Some(p);
                    }
                }
                _ => {}
            }
            return false;
        }
        match k.code {
            KeyCode::Char('o') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_mode();
            }
            KeyCode::Char('i') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = TuiMode::Agent;
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
    let human = match state {
        "completed" => "Completed",
        "failed" => "Failed",
        "cancelled" => "Cancelled",
        "timed_out" => "Timed out",
        "interrupted" => "Interrupted",
        other => other,
    };
    let mut text = match duration_secs(run) {
        Some(d) => format!("{human} in {d:.1} s"),
        None => human.to_string(),
    };
    let r = &run["receipt"];
    if r.is_object() {
        let tin = r["usage"]["input_tokens"].as_i64().unwrap_or(0);
        let tout = r["usage"]["output_tokens"].as_i64().unwrap_or(0);
        if tin > 0 || tout > 0 {
            text.push_str(&format!(" · {tin} in / {tout} out"));
        }
    }
    if let Some(err) = run["result"]["error"].as_str() {
        text.push_str(&format!(" · {err}"));
    }
    text
}

/// Quita secuencias de control (ESC/ANSI, NUL, BEL) y conserva saltos y tabs.
/// El texto viene de un modelo o de un receipt y no debe poder inyectar
/// secuencias en el terminal ni al exportar.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                // ESC [ ... letra final
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for n in chars.by_ref() {
                        if n.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Proyecta el transcript de un thread desde runs y eventos durables.
///
/// Orden: runs por `started_at` y, en empate, por id; dentro de cada run,
/// eventos por `sequence`. Los eventos sin run no se descartan.
pub fn project_thread(runs: &[Value], events: &[Value]) -> Vec<AgentLine> {
    let mut ordered: Vec<&Value> = runs.iter().collect();
    // Orden estable: por inicio y, si coincide o falta, por id.
    ordered.sort_by(|a, b| {
        a["started_at"]
            .as_str()
            .unwrap_or("")
            .cmp(b["started_at"].as_str().unwrap_or(""))
            .then_with(|| {
                a["id"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["id"].as_str().unwrap_or(""))
            })
    });
    let mut out = Vec::new();
    for run in ordered {
        let id = run["id"].as_str().unwrap_or("");
        if let Some(prompt) = run["prompt"].as_str()
            && !prompt.is_empty()
        {
            out.push(AgentLine {
                kind: AgentKind::User,
                text: sanitize(prompt),
                detail: None,
                expanded: false,
            });
        }
        let mut evs: Vec<&Value> = events
            .iter()
            .filter(|e| e["run_id"].as_str() == Some(id))
            .collect();
        evs.sort_by_key(|e| e["sequence"].as_i64().unwrap_or(0));
        let is_terminal_event = |e: &Value| {
            matches!(
                e["type"].as_str().unwrap_or(""),
                "run.completed"
                    | "run.failed"
                    | "run.cancelled"
                    | "run.timed_out"
                    | "run.interrupted"
            )
        };
        let answer = run["result"]["text"].as_str().unwrap_or("").trim();
        let mut answer_placed = false;
        for e in evs {
            // En el transcript se oculta el ruido de máquina; el Inspector y
            // `event_line` conservan todos los eventos.
            if matches!(
                e["type"].as_str().unwrap_or(""),
                "run.queued"
                    | "run.planning"
                    | "run.running"
                    | "model.requested"
                    | "context.manifest.created"
            ) {
                continue;
            }
            // La respuesta del proveedor se muestra antes del cierre del run,
            // nunca después del evento terminal.
            if is_terminal_event(e) && !answer_placed && !answer.is_empty() {
                out.push(AgentLine {
                    kind: AgentKind::Hli,
                    text: sanitize(answer),
                    detail: None,
                    expanded: false,
                });
                answer_placed = true;
            }
            if let Some(line) = event_line(e) {
                out.push(AgentLine {
                    kind: AgentKind::Tool,
                    text: line,
                    detail: event_detail(e),
                    expanded: false,
                });
            }
        }
        if !answer_placed && !answer.is_empty() {
            out.push(AgentLine {
                kind: AgentKind::Hli,
                text: sanitize(answer),
                detail: None,
                expanded: false,
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
            detail: None,
            expanded: false,
        });
    }
    // Eventos sin run asociado (plan, permisos sueltos): no se descartan, se
    // muestran al final en orden de secuencia.
    let mut orphans: Vec<&Value> = events
        .iter()
        .filter(|e| {
            let rid = e["run_id"].as_str().unwrap_or("");
            !runs.iter().any(|r| r["id"].as_str() == Some(rid))
        })
        .collect();
    orphans.sort_by_key(|e| e["sequence"].as_i64().unwrap_or(0));
    for e in orphans {
        if let Some(line) = event_line(e) {
            out.push(AgentLine {
                kind: AgentKind::Tool,
                text: sanitize(&line),
                detail: event_detail(e),
                expanded: false,
            });
        }
    }
    out
}

impl App {
    /// Reconstruye el transcript del thread activo desde el Event Store.
    pub fn load_transcript(&mut self) {
        let Some(ws) = self.agent.workspace.as_ref().and_then(|w| w["id"].as_str()) else {
            return;
        };
        let branch = self
            .agent
            .branch
            .as_ref()
            .and_then(|b| b["id"].as_str())
            .map(str::to_string);
        self.agent.transcript_loading = true;
        self.daemon_async
            .request(crate::daemon_async::Request::Transcript {
                workspace_id: ws.to_string(),
                branch_id: branch,
            });
    }

    /// Aplica el paquete que compone el worker (runs + eventos).
    fn apply_transcript(&mut self, v: &Value) {
        let runs = v["runs"].as_array().cloned().unwrap_or_default();
        let events = v["events"].as_array().cloned().unwrap_or_default();
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
        self.agent.transcript_loading = false;
        self.agent.viewport.follow_mode = true;
        self.agent.scroll_top = None;
    }

    /// Carga los threads recientes (con su último run) para el selector.
    pub fn open_resume(&mut self) {
        self.agent.resume_items.clear();
        self.agent.resume_index = 0;
        self.agent.resume_open = true;
        let Some(list) = client::call_ui("workspace.list", json!({})).ok() else {
            return;
        };
        for ws in list.as_array().cloned().unwrap_or_default().iter().take(5) {
            let wid = ws["id"].as_str().unwrap_or("");
            let wname = ws["name"].as_str().unwrap_or("-").to_string();
            let threads = client::call_ui("thread.list", json!({ "workspace_id": wid }))
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            let runs = client::call_ui("run.list", json!({ "workspace_id": wid }))
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            for th in threads.iter().rev().take(8) {
                let tid = th["id"].as_str().unwrap_or("").to_string();
                let title = th["title"].as_str().unwrap_or("-").to_string();
                let branches = client::call_ui("branch.list", json!({ "thread_id": tid }))
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
    }

    /// Referencias estructuradas que se envían al daemon (identidad estable,
    /// no el token visible).
    pub fn explicit_reference_args(&self) -> Vec<Value> {
        self.agent
            .references
            .iter()
            .map(|r| {
                json!({
                    "reference_type": r.kind.label(),
                    "stable_id": r.stable_id,
                    "display_text": r.display,
                    "selected_at": r.selected_at.to_string(),
                    "selection_reason": "user_selected",
                    "content_hash": r.content_hash,
                    "revision": r.revision,
                    "source_scope": "workspace",
                })
            })
            .collect()
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
}

// ---------------------------------------------------------------------------
// Slice 3b: actividad expandible y scroll del transcript
// ---------------------------------------------------------------------------

pub const TRANSCRIPT_PAGE: usize = 12;

fn detail_line_count(d: &ActivityDetail) -> usize {
    let mut n = 8; // type/status/started/duration/input/output/causation/correlation
    if !d.artifacts.is_empty() {
        n += 1;
    }
    if d.error.is_some() {
        n += 1;
    }
    n
}

impl App {
    /// Líneas renderizadas aproximadas (texto + detalles expandidos).
    pub fn rendered_len(&self) -> usize {
        self.agent
            .lines
            .iter()
            .map(|l| {
                let text = l.text.split('\n').count();
                let detail = if l.expanded {
                    l.detail.as_ref().map(detail_line_count).unwrap_or(0)
                } else {
                    0
                };
                2 + text + detail
            })
            .sum()
    }

    pub fn toggle_selected_activity(&mut self) {
        let idx = self.agent.activity_sel.or_else(|| {
            self.agent
                .lines
                .iter()
                .rposition(|l| l.kind == AgentKind::Tool)
        });
        if let Some(i) = idx
            && let Some(line) = self.agent.lines.get_mut(i)
            && line.detail.is_some()
        {
            line.expanded = !line.expanded;
            self.agent.activity_sel = Some(i);
        }
    }

    pub fn toggle_all_details(&mut self) {
        self.agent.details_all = !self.agent.details_all;
        let v = self.agent.details_all;
        for l in &mut self.agent.lines {
            if l.detail.is_some() {
                l.expanded = v;
            }
        }
    }

    pub fn scroll_page_up(&mut self) {
        let total = self.rendered_len();
        let cur = self
            .agent
            .scroll_top
            .unwrap_or_else(|| total.saturating_sub(TRANSCRIPT_PAGE));
        self.agent.scroll_top = Some(cur.saturating_sub(TRANSCRIPT_PAGE));
    }

    pub fn scroll_page_down(&mut self) {
        let total = self.rendered_len();
        if let Some(cur) = self.agent.scroll_top {
            let next = cur + TRANSCRIPT_PAGE;
            if next + 1 >= total {
                self.scroll_end();
            } else {
                self.agent.scroll_top = Some(next);
            }
        }
    }

    pub fn scroll_home(&mut self) {
        self.agent.scroll_top = Some(0);
    }

    pub fn scroll_end(&mut self) {
        self.agent.scroll_top = None;
        self.agent.new_activity = 0;
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        let total = self.rendered_len() as isize;
        if total <= 0 {
            return;
        }
        let cur = self
            .agent
            .scroll_top
            .map(|t| t as isize)
            .unwrap_or(total - TRANSCRIPT_PAGE as isize);
        let next = (cur + delta).clamp(0, total - 1);
        self.agent.scroll_top = if next + 1 >= total {
            None
        } else {
            Some(next as usize)
        };
    }
}

// ---------------------------------------------------------------------------
// Slice 3b: referencias estables con @
// ---------------------------------------------------------------------------

fn file_hash(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("sha256:{hex}"))
}

fn walk_files(root: &std::path::Path, rel: &str, out: &mut Vec<(String, u64)>, depth: usize) {
    const CAP: usize = 400;
    if out.len() >= CAP || depth > 6 {
        return;
    }
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= CAP {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "target" | "dist" | "node_modules") {
            continue;
        }
        let child = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_files(root, &child, out, depth + 1),
            Ok(t) if t.is_file() => {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((child, size));
            }
            _ => {}
        }
    }
}

impl App {
    pub fn open_reference_selector(&mut self, at: usize) {
        self.agent.ref_open = true;
        self.agent.ref_start = at;
        self.agent.ref_query.clear();
        self.agent.ref_index = 0;
        self.agent.ref_category = 0;
        self.load_reference_sources();
    }

    pub fn load_reference_sources(&mut self) {
        let mut cats: Vec<Vec<RefItem>> = vec![Vec::new(); 5];
        let ws = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["id"].as_str())
            .map(str::to_string);
        let thread = self
            .agent
            .thread
            .as_ref()
            .and_then(|t| t["id"].as_str())
            .map(str::to_string);
        let root = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["root_path"].as_str())
            .map(std::path::PathBuf::from);

        if let Some(root) = root {
            let mut files = Vec::new();
            walk_files(&root, "", &mut files, 0);
            for (path, size) in files {
                cats[0].push(RefItem {
                    kind: RefKind::File,
                    stable_id: path.clone(),
                    display: path,
                    subtitle: format!("{size} bytes"),
                    content_hash: None,
                    revision: None,
                    size: Some(size),
                });
            }
        }

        if let Some(ws) = ws {
            if let Ok(list) = client::call("artifact.list", json!({ "workspace_id": ws }))
                && let Some(arr) = list.as_array()
            {
                for a in arr {
                    let id = a["id"].as_str().unwrap_or("").to_string();
                    if id.is_empty() {
                        continue;
                    }
                    cats[1].push(RefItem {
                        kind: RefKind::Artifact,
                        stable_id: id.clone(),
                        display: id.clone(),
                        subtitle: format!(
                            "{} · {} bytes",
                            a["mime"].as_str().unwrap_or("-"),
                            a["bytes"].as_i64().unwrap_or(0)
                        ),
                        content_hash: a["sha256"].as_str().map(str::to_string),
                        revision: None,
                        size: a["bytes"].as_u64(),
                    });
                }
            }
            if let Ok(list) = client::call("memory.list", json!({ "workspace_id": ws }))
                && let Some(arr) = list.as_array()
            {
                for m in arr {
                    if m["status"].as_str() != Some("active") {
                        continue;
                    }
                    let id = m["id"].as_str().unwrap_or("").to_string();
                    let content = m["content"].as_str().unwrap_or("");
                    let brief: String = content.chars().take(60).collect();
                    cats[2].push(RefItem {
                        kind: RefKind::Memory,
                        stable_id: id.clone(),
                        display: brief,
                        subtitle: format!(
                            "{} · {}",
                            id,
                            m["source_authority"].as_str().unwrap_or("user")
                        ),
                        content_hash: None,
                        revision: None,
                        size: None,
                    });
                }
            }
            if let Ok(list) = client::call("run.list", json!({ "workspace_id": ws }))
                && let Some(arr) = list.as_array()
            {
                for r in arr {
                    if let Some(t) = thread.as_deref()
                        && r["thread_id"].as_str() != Some(t)
                    {
                        continue;
                    }
                    let id = r["id"].as_str().unwrap_or("").to_string();
                    let prompt = r["prompt"].as_str().unwrap_or("");
                    let brief: String = prompt.chars().take(50).collect();
                    cats[3].push(RefItem {
                        kind: RefKind::Run,
                        stable_id: id.clone(),
                        display: brief,
                        subtitle: format!("{id} · {}", r["state"].as_str().unwrap_or("-")),
                        content_hash: None,
                        revision: None,
                        size: None,
                    });
                }
            }
        }

        if let Some(thread) = thread
            && let Ok(list) = client::call("branch.list", json!({ "thread_id": thread }))
            && let Some(arr) = list.as_array()
        {
            for b in arr {
                let name = b["name"].as_str().unwrap_or("").to_string();
                cats[4].push(RefItem {
                    kind: RefKind::Branch,
                    stable_id: name.clone(),
                    display: name,
                    subtitle: format!("seq {}", b["head_sequence"].as_i64().unwrap_or(0)),
                    content_hash: None,
                    revision: b["head_sequence"].as_i64(),
                    size: None,
                });
            }
        }

        self.agent.ref_items = cats;
    }

    pub fn ref_category_kind(&self) -> RefKind {
        match self.agent.ref_category {
            0 => RefKind::File,
            1 => RefKind::Artifact,
            2 => RefKind::Memory,
            3 => RefKind::Run,
            _ => RefKind::Branch,
        }
    }

    pub fn ref_filtered(&self) -> Vec<&RefItem> {
        let q = self.agent.ref_query.to_lowercase();
        self.agent
            .ref_items
            .get(self.agent.ref_category)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter(|i| {
                q.is_empty()
                    || i.display.to_lowercase().contains(&q)
                    || i.stable_id.to_lowercase().contains(&q)
            })
            .collect()
    }

    fn reference_root(&self) -> Option<std::path::PathBuf> {
        self.agent
            .workspace
            .as_ref()
            .and_then(|w| w["root_path"].as_str())
            .map(std::path::PathBuf::from)
    }

    /// Referencias de archivo que no pueden usarse tal como se seleccionaron.
    pub fn preflight_issues(&self) -> Vec<PreflightIssue> {
        let root = self.reference_root();
        let mut out = Vec::new();
        for (index, r) in self.agent.references.iter().enumerate() {
            if r.kind != RefKind::File {
                continue;
            }
            let Some(root) = root.as_ref() else {
                continue;
            };
            let current = file_hash(&root.join(&r.stable_id));
            match (&r.content_hash, current) {
                (_, None) => out.push(PreflightIssue {
                    index,
                    stable_id: r.stable_id.clone(),
                    reason: "missing".into(),
                    selected_hash: r.content_hash.clone(),
                    current_hash: None,
                    refreshable: false,
                }),
                (Some(sel), Some(cur)) if sel != &cur => out.push(PreflightIssue {
                    index,
                    stable_id: r.stable_id.clone(),
                    reason: "changed".into(),
                    selected_hash: Some(sel.clone()),
                    current_hash: Some(cur),
                    refreshable: true,
                }),
                (None, Some(cur)) => out.push(PreflightIssue {
                    index,
                    stable_id: r.stable_id.clone(),
                    reason: "changed".into(),
                    selected_hash: None,
                    current_hash: Some(cur),
                    refreshable: true,
                }),
                _ => {}
            }
        }
        self.preflight_other_kinds(&mut out);
        out
    }

    fn preflight_other_kinds(&self, out: &mut Vec<PreflightIssue>) {
        let ws = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["id"].as_str())
            .map(str::to_string);
        let thread = self
            .agent
            .thread
            .as_ref()
            .and_then(|t| t["id"].as_str())
            .map(str::to_string);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        for (index, r) in self.agent.references.iter().enumerate() {
            if r.kind == RefKind::File {
                continue;
            }
            let reason = match r.kind {
                RefKind::Memory => {
                    match client::call("memory.get", json!({ "memory_id": r.stable_id })) {
                        Ok(m) => classify_memory(
                            m["status"].as_str().unwrap_or(""),
                            m["valid_from"].as_str(),
                            m["valid_until"].as_str(),
                            now,
                        )
                        .map(str::to_string),
                        Err(_) => Some("not found".to_string()),
                    }
                }
                RefKind::Artifact => {
                    let current = ws.as_ref().and_then(|w| {
                        client::call("artifact.list", json!({ "workspace_id": w }))
                            .ok()
                            .and_then(|v| v.as_array().cloned())
                            .and_then(|list| {
                                list.iter()
                                    .find(|a| a["id"].as_str() == Some(r.stable_id.as_str()))
                                    .and_then(|a| a["sha256"].as_str().map(str::to_string))
                            })
                    });
                    classify_artifact_hash(r.content_hash.as_deref(), current.as_deref())
                        .map(str::to_string)
                }
                RefKind::Run => match client::call("run.get", json!({ "run_id": r.stable_id })) {
                    Ok(v) => {
                        let owner = v["run"]["workspace_id"].as_str().unwrap_or("");
                        if Some(owner) != ws.as_deref() {
                            Some("out of scope".to_string())
                        } else {
                            None
                        }
                    }
                    Err(_) => Some("not found".to_string()),
                },
                RefKind::Branch => {
                    let branches = thread
                        .as_ref()
                        .and_then(|t| {
                            client::call("branch.list", json!({ "thread_id": t }))
                                .ok()
                                .and_then(|v| v.as_array().cloned())
                        })
                        .unwrap_or_default();
                    match branches.iter().find(|b| {
                        b["id"].as_str() == Some(r.stable_id.as_str())
                            || b["name"].as_str() == Some(r.stable_id.as_str())
                    }) {
                        Some(b) => classify_branch_revision(
                            r.revision,
                            b["head_sequence"].as_i64().unwrap_or(0),
                        )
                        .map(str::to_string),
                        None => Some("not found".to_string()),
                    }
                }
                RefKind::File => None,
            };
            if let Some(reason) = reason {
                out.push(PreflightIssue {
                    index,
                    stable_id: r.stable_id.clone(),
                    reason,
                    selected_hash: None,
                    current_hash: None,
                    refreshable: false,
                });
            }
        }
    }

    pub fn cancel_preflight(&mut self) {
        self.agent.preflight = None;
    }

    pub fn refresh_preflight(&mut self) {
        let Some(pf) = self.agent.preflight.take() else {
            return;
        };
        let root = self.reference_root();
        for issue in &pf.issues {
            let Some(root) = root.as_ref() else { break };
            if let Some(r) = self.agent.references.get_mut(issue.index) {
                let path = root.join(&r.stable_id);
                if let Some(h) = file_hash(&path) {
                    r.content_hash = Some(h);
                    r.size = std::fs::metadata(&path).ok().map(|m| m.len());
                }
            }
        }
        let remaining = self.preflight_issues();
        if remaining.is_empty() {
            self.agent.history.push(pf.prompt.clone());
            self.agent.history_index = None;
            if self.run_prompt(&pf.prompt) {
                self.agent.composer.clear();
                self.agent.push(AgentKind::User, pf.prompt.clone());
            }
        } else {
            self.agent.preflight = Some(Preflight {
                prompt: pf.prompt,
                issues: remaining,
            });
        }
    }

    pub fn reference_insert(&mut self) {
        let picked = self
            .ref_filtered()
            .get(self.agent.ref_index)
            .map(|i| (*i).clone());
        let Some(item) = picked else {
            return;
        };
        let root = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["root_path"].as_str())
            .map(std::path::PathBuf::from);
        let (hash, size) = match item.kind {
            RefKind::File => {
                let path = root.map(|r| r.join(&item.stable_id));
                match path.as_deref().map(file_hash) {
                    Some(h) => (h, item.size),
                    None => (None, item.size),
                }
            }
            _ => (item.content_hash.clone(), item.size),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let r = Reference {
            kind: item.kind,
            stable_id: item.stable_id.clone(),
            display: item.display.clone(),
            content_hash: hash,
            revision: item.revision,
            size,
            selected_at: now,
        };
        self.agent.composer.truncate(self.agent.ref_start);
        self.agent.composer.push_str(&reference_token(&r));
        self.agent.composer.push(' ');
        self.agent.references.push(r);
        self.agent.ref_open = false;
        self.agent.ref_query.clear();
    }

    pub fn reference_close(&mut self) {
        self.agent.composer.truncate(self.agent.ref_start);
        self.agent.ref_open = false;
        self.agent.ref_query.clear();
    }

    /// Referencias de archivo que ya no coinciden con lo seleccionado.
    /// No sustituye contenido: informa.
    pub fn stale_references(&self) -> Vec<String> {
        let root = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["root_path"].as_str())
            .map(std::path::PathBuf::from);
        let mut out = Vec::new();
        for r in &self.agent.references {
            if r.kind != RefKind::File {
                continue;
            }
            let Some(root) = root.clone() else {
                continue;
            };
            let path = root.join(&r.stable_id);
            match file_hash(&path) {
                None => out.push(format!("reference missing: {}", r.stable_id)),
                Some(h) if Some(&h) != r.content_hash.as_ref() => out.push(format!(
                    "reference changed: {} (se conserva la revisión seleccionada)",
                    r.stable_id
                )),
                _ => {}
            }
        }
        out
    }

    #[allow(clippy::collapsible_match)]
    fn handle_ref_key(&mut self, k: KeyEvent) -> bool {
        let n = self.ref_filtered().len();
        match k.code {
            KeyCode::Esc => self.reference_close(),
            KeyCode::Enter => self.reference_insert(),
            KeyCode::Up => {
                self.agent.ref_index = self.agent.ref_index.saturating_sub(1);
            }
            KeyCode::Down => {
                if n > 0 && self.agent.ref_index + 1 < n {
                    self.agent.ref_index += 1;
                }
            }
            KeyCode::Tab => {
                self.agent.ref_category = (self.agent.ref_category + 1) % 5;
                self.agent.ref_index = 0;
            }
            KeyCode::BackTab => {
                self.agent.ref_category = (self.agent.ref_category + 4) % 5;
                self.agent.ref_index = 0;
            }
            KeyCode::Backspace => {
                self.agent.ref_query.pop();
                self.agent.ref_index = 0;
            }
            KeyCode::Char(c) => {
                self.agent.ref_query.push(c);
                self.agent.ref_index = 0;
            }
            _ => {}
        }
        false
    }
}

impl App {
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

pub const SLASH_COMMANDS: [(&str, &str); 21] = [
    ("help", "comandos y atajos"),
    ("status", "estado de la sesión"),
    ("model", "elegir proveedor y modelo"),
    ("provider", "proveedores configurados"),
    ("workspace", "elegir o crear workspace"),
    ("sessions", "sesiones del workspace"),
    ("new", "sesión nueva"),
    ("resume", "reanudar un thread"),
    ("branch", "crear o cambiar de rama"),
    ("inspect", "abrir el Inspector"),
    ("context", "contexto enviado al modelo"),
    ("memory", "memoria del workspace"),
    ("artifacts", "artefactos del run"),
    ("mch", "procesos MCH"),
    ("details", "expandir detalles"),
    ("cancel", "cancelar el run activo"),
    ("clear", "limpiar el transcript"),
    ("export", "exportar la conversación"),
    ("settings", "configuración de HLI"),
    ("config", "igual que /settings"),
    ("exit", "salir de HLI"),
];

pub fn available_commands() -> Vec<&'static str> {
    SLASH_COMMANDS.iter().map(|(name, _)| *name).collect()
}

pub fn format_providers(list: &[Value], current: &str) -> Vec<String> {
    list.iter()
        .map(|p| {
            let name = p["name"].as_str().unwrap_or("-");
            let kind = p["kind"].as_str().unwrap_or("-");
            let model = p["model"].as_str().unwrap_or("-");
            let mark = if name == current { " *" } else { "" };
            format!("/{name} · {kind} · {model}{mark}")
        })
        .collect()
}

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

fn event_detail(e: &Value) -> Option<ActivityDetail> {
    let kind = e["type"].as_str()?.to_string();
    let p = &e["payload"];
    let error = p["error"].as_str().map(str::to_string);
    let status = if error.is_some() || kind.ends_with("failed") {
        "failed"
    } else {
        "ok"
    };
    let output = if let Some(usage) = p["usage"].as_object() {
        format!(
            "{} in / {} out",
            usage
                .get("input_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            usage
                .get("output_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        )
    } else {
        p["result"]
            .as_str()
            .or_else(|| p["matches"].as_i64().map(|_| ""))
            .unwrap_or("")
            .chars()
            .take(120)
            .collect()
    };
    Some(ActivityDetail {
        kind,
        status: status.into(),
        started: e["created_at"].as_str().unwrap_or("-").to_string(),
        duration: None,
        input: p["tool"]
            .as_str()
            .or_else(|| p["model"].as_str())
            .unwrap_or("")
            .to_string(),
        output,
        causation: e["causation_id"].as_str().map(str::to_string),
        correlation: e["correlation_id"].as_str().map(str::to_string),
        artifacts: e["artifacts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| {
                        v["artifact_id"]
                            .as_str()
                            .or_else(|| v.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        error,
    })
}

pub fn event_line(e: &Value) -> Option<String> {
    let kind = e["type"].as_str()?;
    // Agent Mode habla en bloques humanos. Los identificadores, hashes,
    // secuencias y payloads crudos viven en el detalle (Ctrl+E) y en el Inspector.
    if matches!(
        kind,
        "run.queued"
            | "run.planning"
            | "run.running"
            | "model.requested"
            | "model.responded"
            | "context.manifest.created"
    ) {
        return None;
    }
    let p = &e["payload"];
    let tool = p["tool"]
        .as_str()
        .or_else(|| p["operation"].as_str())
        .unwrap_or("");
    let target = p["target"].as_str().unwrap_or("");
    let effects = p["requested_effects"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "declared".into());
    match kind {
        "plan.created" => Some("Plan".to_string()),
        "plan.revised" => Some("Plan updated".to_string()),
        "plan.invalidated" => Some("Plan needs review".to_string()),
        "run.waiting_approval" => Some(format!("Permission required · {effects}")),
        "plan.approved" => Some("Plan approved".to_string()),
        "plan.rejected" => Some("Plan rejected".to_string()),
        "plan.execution_started" => Some("Running".to_string()),
        "tool.requested" => Some(if tool.is_empty() {
            "Running".to_string()
        } else {
            format!("Running {tool}")
        }),
        "tool.completed" => Some(if target.is_empty() {
            format!("Done · {tool}")
        } else {
            format!("Modified {target}")
        }),
        "tool.failed" => Some(format!("Failed · {tool}")),
        "tool.denied" => Some(format!("Denied · {tool}")),
        "tool.execution_resumed" => Some("Running".to_string()),
        "tool.permission_requested" | "permission.requested" => {
            Some(format!("Permission required · {tool}"))
        }
        "tool.permission_approved" | "permission.approved" => {
            Some("Permission granted".to_string())
        }
        "permission.denied" => Some("Permission denied".to_string()),
        "run.completed" => Some("Completed".to_string()),
        "run.failed" => Some("Failed".to_string()),
        "run.cancelled" => Some("Cancelled".to_string()),
        "run.interrupted" => Some("Interrupted".to_string()),
        "run.timed_out" => Some("Timed out".to_string()),
        _ if kind.starts_with("plan.") => Some("Plan".to_string()),
        _ if kind.starts_with("tool.") => Some(if tool.is_empty() {
            "Running".to_string()
        } else {
            format!("· {tool}")
        }),
        _ => None,
    };
    line
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
        let list = client::call_ui("workspace.list", json!({})).unwrap_or(Value::Null);
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
    }

    /// Indicador de trabajo: spinner animado con cuentagotas ASCII si el
    /// terminal es plano.
    pub fn spinner(&self) -> &'static str {
        if !is_active(&self.agent.run_state) {
            return "";
        }
        let idx = (self.spinner_at.elapsed().as_millis() / 100) as usize;
        if self.ascii {
            const ASCII: [&str; 4] = ["|", "/", "-", "\\"];
            ASCII[idx % ASCII.len()]
        } else {
            const BRAILLE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            BRAILLE[idx % BRAILLE.len()]
        }
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
        let base = format!(
            "{ws}/{th}/{br} · {}/{} · {}",
            self.agent.provider,
            self.agent.model,
            self.agent.run_state.replace('_', " ")
        );
        match &self.statusline_cache {
            Some(extra) => format!("{base} │ {extra}"),
            None => base,
        }
    }

    pub fn load_threads(&mut self) {
        let Some(ws) = self.agent.workspace.as_ref().and_then(|w| w["id"].as_str()) else {
            return;
        };
        self.agent.lists_loading = true;
        self.daemon_async
            .request(crate::daemon_async::Request::Threads(ws.to_string()));
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
        let list =
            client::call_ui("branch.list", json!({ "thread_id": id })).unwrap_or(Value::Null);
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
        let list =
            client::call_ui("run.list", json!({ "workspace_id": id })).unwrap_or(Value::Null);
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
                match client::call_ui("workspace.create", json!({ "name": name, "root": root })) {
                    Ok(w) => {
                        self.agent.workspace = Some(w.clone());
                        self.workspaces = vec![w.clone()];
                        self.agent.offer = Offer::None;
                        self.agent.push(
                            AgentKind::Info,
                            format!("Workspace created: {}", w["id"].as_str().unwrap_or("-")),
                        );
                        self.load_threads();
                        if let Some(p) = self.agent.pending.take()
                            && self.run_prompt(&p)
                        {
                            self.agent.push(AgentKind::User, p.clone());
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
                match client::call_ui("branch.create", json!({ "thread_id": tid, "name": name })) {
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
        let issues = self.preflight_issues();
        if !issues.is_empty() {
            self.agent.preflight = Some(Preflight {
                prompt: text,
                issues,
            });
            return false;
        }
        self.agent.history.push(text.clone());
        self.agent.history_index = None;
        let accepted = self.run_prompt(&text);
        if accepted {
            self.agent.composer.clear();
            self.agent.push(AgentKind::User, text.clone());
        }
        accepted
    }

    fn ensure_thread(&mut self, text: &str) -> Option<String> {
        let ws = self
            .agent
            .workspace
            .as_ref()?
            .get("id")?
            .as_str()?
            .to_string();
        // Un thread de otro workspace no sirve: el daemon rechaza el run y el
        // usuario ve «el thread pertenece a otro workspace». Se descarta.
        if let Some(t) = self.agent.thread.as_ref() {
            // Sin `workspace_id` se acepta (thread antiguo o de test); con
            // uno distinto, se descarta.
            let same = match t["workspace_id"].as_str() {
                None => true,
                Some(tws) => tws == ws.as_str(),
            };
            if same {
                if let Some(id) = t["id"].as_str() {
                    return Some(id.to_string());
                }
            } else {
                self.agent.thread = None;
                self.agent.branch = None;
            }
        }
        let title: String = text.chars().take(48).collect();
        match client::call_ui(
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
        match client::call_ui("branch.create", json!({ "thread_id": tid, "name": "main" })) {
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

    // -----------------------------------------------------------------------
    // Transcript Mode
    // -----------------------------------------------------------------------

    pub fn open_transcript(&mut self) {
        self.transcript.saved_scroll = self.agent.scroll_top;
        self.transcript.search_open = false;
        self.transcript.query.clear();
        self.transcript.matches.clear();
        self.transcript.match_index = 0;
        self.transcript.scroll = 0;
        self.mode = TuiMode::Transcript;
    }

    /// `HLI_HOME/ui.toml` admite `statusline = "comando"`. Es el mismo
    /// mecanismo que Claude Code: un comando recibe JSON y devuelve una línea.
    pub fn statusline_command() -> Option<String> {
        let text = std::fs::read_to_string(crate::client::home().join("ui.toml")).ok()?;
        Self::parse_statusline(&text)
    }

    fn refresh_statusline(&mut self) {
        let Some(cmd) = Self::statusline_command() else {
            self.statusline_cache = None;
            return;
        };
        let payload = json!({
            "state": self.agent.run_state,
            "provider": self.agent.provider,
            "model": self.agent.model,
            "workspace": self.agent.workspace.as_ref().and_then(|w| w["name"].as_str()),
            "thread": self.agent.thread.as_ref().and_then(|t| t["title"].as_str()),
            "mode": format!("{:?}", self.mode),
        });
        let mut command;
        #[cfg(windows)]
        {
            command = std::process::Command::new("cmd");
            command.args(["/C", &cmd]);
        }
        #[cfg(not(windows))]
        {
            command = std::process::Command::new("sh");
            command.args(["-c", &cmd]);
        }
        let child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            self.statusline_cache = None;
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = writeln!(stdin, "{payload}");
            // Cerrar stdin: el comando debe terminar por sí mismo.
            drop(stdin);
        }
        // Un statusline colgado no puede colgar la TUI: techo de un segundo.
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() < std::time::Duration::from_secs(1) => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                _ => {
                    let _ = child.kill();
                    break;
                }
            }
        }
        let out = child.wait_with_output().ok();
        let text = out
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self.statusline_cache = text;
    }

    /// Texto completo del transcript para volcar o exportar.
    pub fn transcript_text(&self) -> String {
        let mut out = String::new();
        for (_, text, _) in self.transcript_lines(self.transcript_width) {
            out.push_str(&text);
            out.push('\n');
        }
        out
    }

    pub fn transcript_export_path(&self) -> std::path::PathBuf {
        let dir = crate::client::home().join("transcripts");
        let _ = std::fs::create_dir_all(&dir);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        dir.join(format!("transcript-{stamp}.txt"))
    }

    pub fn close_transcript(&mut self) {
        self.transcript.search_open = false;
        self.mode = TuiMode::Agent;
    }

    /// Líneas del transcript con su índice de origen y si son un prompt.
    pub fn transcript_lines(&self, width: usize) -> Vec<(usize, String, bool)> {
        let mut out = Vec::new();
        for (idx, line) in self.agent.lines.iter().enumerate() {
            let is_user = line.kind == AgentKind::User;
            let tag = match line.kind {
                AgentKind::User => "You",
                AgentKind::Hli => "HLI",
                AgentKind::Tool => "·",
                AgentKind::Error => "×",
                AgentKind::Info => "·",
            };
            let mut text = format!("{tag} {}", line.text.replace('\n', " "));
            if let Some(d) = &line.detail {
                text.push_str(&format!(" · {} · {}", d.kind, d.status));
            }
            // Se envuelve para que el modelo de scroll sea el mismo que el
            // del render (no hay offsets fuera de rango).
            let chunks = wrapped_lines(&text, width).max(1);
            for c in 0..chunks {
                out.push((idx, text.clone(), is_user && c == 0));
            }
        }
        if out.is_empty() {
            out.push((0, String::new(), false));
        }
        out
    }

    fn apply_transcript_search(&mut self, width: usize, height: usize) {
        let q = self.transcript.query.to_lowercase();
        self.transcript.matches = if q.is_empty() {
            Vec::new()
        } else {
            self.transcript_lines(width)
                .iter()
                .enumerate()
                .filter(|(_, (_, text, _))| text.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect()
        };
        self.transcript.match_index = 0;
        if let Some(first) = self.transcript.matches.first().copied() {
            self.transcript.scroll = first.saturating_sub(height / 3);
        }
    }

    pub fn transcript_key(&mut self, k: KeyEvent) -> bool {
        if self.transcript.search_open {
            match k.code {
                KeyCode::Enter => {
                    self.transcript.search_open = false;
                    let (w, h) = (self.transcript_width, self.transcript_height);
                    self.apply_transcript_search(w, h);
                    return false;
                }
                KeyCode::Esc => {
                    self.transcript.search_open = false;
                    self.transcript.query.clear();
                    self.transcript.matches.clear();
                    if let Some(s) = self.transcript.saved_scroll {
                        self.transcript.scroll = s;
                    }
                    return false;
                }
                KeyCode::Backspace => {
                    self.transcript.query.pop();
                }
                KeyCode::Char(c) => self.transcript.query.push(c),
                _ => {}
            }
            return false;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_transcript(),
            KeyCode::Char('o') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.close_transcript()
            }
            KeyCode::Char('/') => {
                self.transcript.saved_scroll = Some(self.transcript.scroll);
                self.transcript.search_open = true;
                self.transcript.query.clear();
            }
            KeyCode::Char('[') => {
                self.transcript_dump = true;
                self.close_transcript();
            }
            KeyCode::Char('v') => {
                self.transcript_export = Some(self.transcript_export_path());
                self.close_transcript();
            }
            KeyCode::Char('n') => {
                if !self.transcript.matches.is_empty() {
                    self.transcript.match_index =
                        (self.transcript.match_index + 1) % self.transcript.matches.len();
                    self.transcript.scroll = self.transcript.matches[self.transcript.match_index]
                        .saturating_sub(self.transcript_height / 3);
                }
            }
            KeyCode::Char('N') => {
                if !self.transcript.matches.is_empty() {
                    let len = self.transcript.matches.len();
                    self.transcript.match_index = (self.transcript.match_index + len - 1) % len;
                    self.transcript.scroll = self.transcript.matches[self.transcript.match_index]
                        .saturating_sub(self.transcript_height / 3);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.transcript.scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.transcript.scroll = self.transcript.scroll.saturating_sub(1)
            }
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.transcript.scroll += (self.transcript_height / 2).max(1)
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.transcript.scroll = self
                    .transcript
                    .scroll
                    .saturating_sub((self.transcript_height / 2).max(1))
            }
            KeyCode::Char('f') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.transcript.scroll += self.transcript_height
            }
            KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.transcript.scroll = self
                    .transcript
                    .scroll
                    .saturating_sub(self.transcript_height)
            }
            KeyCode::PageDown => self.transcript.scroll += self.transcript_height / 2,
            KeyCode::PageUp => {
                self.transcript.scroll = self
                    .transcript
                    .scroll
                    .saturating_sub(self.transcript_height / 2)
            }
            KeyCode::Char('g') => self.transcript.scroll = 0,
            KeyCode::Char('G') => {
                self.transcript.scroll = self
                    .transcript_lines(self.transcript_width)
                    .len()
                    .saturating_sub(self.transcript_height)
            }
            KeyCode::Char('}') | KeyCode::Char('{') => {
                let lines = self.transcript_lines(self.transcript_width);
                let forward = k.code == KeyCode::Char('}');
                let candidates: Vec<usize> = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, _, user))| *user)
                    .map(|(i, _)| i)
                    .collect();
                let target = if forward {
                    candidates.into_iter().find(|i| *i > self.transcript.scroll)
                } else {
                    candidates
                        .into_iter()
                        .rev()
                        .find(|i| *i < self.transcript.scroll)
                };
                if let Some(t) = target {
                    self.transcript.scroll = t;
                }
            }
            _ => {}
        }
        let total = self.transcript_lines(self.transcript_width).len();
        self.transcript.scroll = self
            .transcript
            .scroll
            .min(total.saturating_sub(self.transcript_height));
        false
    }

    /// Área aproximada del transcript: cabecera (3), compositor (3) y estado (1)
    /// quedan fuera del área desplazable.
    pub fn observe_terminal(&mut self, width: u16, height: u16) {
        let total_height = height as usize;
        let area = ratatui::layout::Rect::new(0, 0, width, height);
        let regions = crate::layout::agent_regions(area, self.agent.offer != Offer::None);
        // El cuerpo no lleva bordes laterales; se reserva una columna para la
        // scrollbar cuando aparece.
        let inner_w = width.saturating_sub(1).max(20) as usize;
        let height = crate::layout::body_inner_height(regions.body).max(1);
        self.transcript_width = inner_w;
        self.transcript_height = height;
        self.terminal_width = width as usize;
        self.terminal_height = total_height;
        self.transcript_top = crate::layout::body_content_top(regions.body);
        let mut rows: Vec<(usize, usize)> = Vec::new();
        let mut y = 0usize;
        // El tag ocupa 4 columnas: el texto no dispone del ancho completo.
        let text_w = inner_w.saturating_sub(4).max(8);
        for (idx, line) in self.agent.lines.iter().enumerate() {
            rows.push((y, idx));
            y += wrapped_lines(&line.text, text_w);
            if line.expanded
                && let Some(d) = &line.detail
            {
                y += crate::viewport::detail_rows(
                    &d.input,
                    &d.output,
                    !d.artifacts.is_empty(),
                    d.error.is_some(),
                    inner_w,
                );
            }
        }
        self.hit_rows = rows;
        let total = self.rendered_line_count(inner_w);
        self.agent.viewport.sync(total, height);
        self.sync_scroll_top();
    }

    /// Líneas renderizadas (con wrapping) del transcript actual.
    pub fn rendered_line_count(&self, width: usize) -> usize {
        let text_w = width.saturating_sub(4).max(8);
        let mut total = 0;
        for line in &self.agent.lines {
            total += wrapped_lines(&line.text, text_w);
            if line.expanded
                && let Some(d) = &line.detail
            {
                total += crate::viewport::detail_rows(
                    &d.input,
                    &d.output,
                    !d.artifacts.is_empty(),
                    d.error.is_some(),
                    width,
                );
            }
        }
        total.max(1)
    }

    fn sync_scroll_top(&mut self) {
        self.agent.scroll_top = if self.agent.viewport.at_bottom() {
            None
        } else {
            Some(self.agent.viewport.first_visible_line)
        };
    }

    pub fn viewport_scroll(&mut self, delta: isize) {
        self.agent.viewport.scroll(delta);
        self.sync_scroll_top();
    }

    pub fn viewport_page(&mut self, direction: i8) {
        self.agent.viewport.page(direction);
        self.sync_scroll_top();
    }

    pub fn viewport_start(&mut self) {
        self.agent.viewport.to_start();
        self.sync_scroll_top();
        self.agent.new_activity = 0;
    }

    pub fn viewport_bottom(&mut self) {
        self.agent.viewport.to_bottom();
        self.sync_scroll_top();
        self.agent.new_activity = 0;
    }

    /// Rueda, clic en scrollbar, clic para expandir y selección por arrastre.
    pub fn handle_mouse(&mut self, m: MouseEvent) -> bool {
        match m.kind {
            MouseEventKind::ScrollUp => {
                if self.mode == TuiMode::Agent {
                    let step = self.agent.viewport.wheel_speed as isize;
                    self.viewport_scroll(-step);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.mode == TuiMode::Agent {
                    let step = self.agent.viewport.wheel_speed as isize;
                    self.viewport_scroll(step);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Fila 1: barra de pestañas.
                if self.mode == TuiMode::Agent && m.row == 1 {
                    let col = m.column as usize;
                    self.agent.agent_tab = if col < 14 {
                        AgentTab::Workspace
                    } else if col < 26 {
                        AgentTab::Session
                    } else {
                        AgentTab::Chat
                    };
                    match self.agent.agent_tab {
                        AgentTab::Workspace => self.refresh_workspaces(),
                        AgentTab::Session => self.load_threads(),
                        AgentTab::Chat => {}
                    }
                    return true;
                }
                // Listas de workspace/sesión: clic selecciona, segundo clic entra.
                if self.mode == TuiMode::Agent && self.agent.agent_tab != AgentTab::Chat {
                    // La primera fila de contenido está en `transcript_top`,
                    // no en la 3 (cabecera + regla).
                    let idx = m.row.saturating_sub(self.transcript_top as u16) as usize;
                    match self.agent.agent_tab {
                        AgentTab::Workspace if idx < self.workspaces.len() => {
                            if self.agent.ws_sel == idx {
                                self.select_workspace();
                            } else {
                                self.agent.ws_sel = idx;
                            }
                        }
                        AgentTab::Session if idx < self.threads.len() => {
                            if self.agent.thread_sel == idx {
                                self.select_thread();
                            } else {
                                self.agent.thread_sel = idx;
                            }
                        }
                        _ => {}
                    }
                    return true;
                }
                if self.on_scrollbar(m.column) {
                    self.scrollbar_to(m.row);
                } else if let Some(idx) = self.row_to_line(m.row) {
                    self.selection_start(idx);
                    self.toggle_line(idx);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.on_scrollbar(m.column) {
                    self.scrollbar_to(m.row);
                } else if let Some(idx) = self.row_to_line(m.row) {
                    self.selection_update(idx);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.selection_copy();
            }
            _ => return false,
        }
        true
    }

    fn on_scrollbar(&self, column: u16) -> bool {
        self.mode == TuiMode::Agent
            && (column as usize) + 3 >= self.terminal_width
            && self.agent.viewport.total_rendered_lines > self.agent.viewport.viewport_height
    }

    fn scrollbar_to(&mut self, row: u16) {
        let top = self.transcript_top as u16;
        let local = row.saturating_sub(top) as usize;
        let height = self.agent.viewport.viewport_height.max(1);
        let max = self.agent.viewport.max_scroll();
        let line = (local.saturating_mul(max) / height).min(max);
        self.agent.viewport.first_visible_line = line;
        self.agent.viewport.follow_mode = line >= max;
        if self.agent.viewport.follow_mode {
            self.agent.viewport.unseen_activity_count = 0;
        }
        self.sync_scroll_top();
    }

    /// Fila del terminal → índice de línea lógica, según el mapa del render.
    fn row_to_line(&self, row: u16) -> Option<usize> {
        let local = row.saturating_sub(self.transcript_top as u16) as usize
            + self.agent.viewport.first_visible_line;
        self.hit_rows
            .iter()
            .rev()
            .find(|(y, _)| *y <= local)
            .map(|(_, idx)| *idx)
    }

    pub fn selection_start(&mut self, idx: usize) {
        self.sel = Some(idx);
        self.sel_end = Some(idx);
    }

    pub fn selection_update(&mut self, idx: usize) {
        if self.sel.is_some() {
            self.sel_end = Some(idx);
        }
    }

    pub fn selection_copy(&mut self) {
        let (Some(a), Some(b)) = (self.sel, self.sel_end) else {
            return;
        };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let text: String = self
            .agent
            .lines
            .iter()
            .skip(lo)
            .take(hi - lo + 1)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            Self::copy_to_clipboard(&text);
        }
        self.sel = None;
        self.sel_end = None;
    }

    fn toggle_line(&mut self, idx: usize) {
        if let Some(line) = self.agent.lines.get_mut(idx)
            && line.detail.is_some()
        {
            line.expanded = !line.expanded;
            // El layout se recalcula con el tamaño real, no con un +9 fijo.
            self.observe_terminal(self.terminal_width as u16, self.terminal_height as u16);
        }
    }

    /// `ui.toml` mínimo: `statusline = "comando"`. Se ignora el resto.
    pub fn parse_statusline(text: &str) -> Option<String> {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("statusline") {
                let rest = rest.trim_start().strip_prefix('=')?.trim();
                let cmd = rest.trim_matches('"').trim();
                if !cmd.is_empty() {
                    return Some(cmd.to_string());
                }
            }
        }
        None
    }

    /// Copia texto al portapapeles del sistema. Silencioso si no hay herramienta.
    fn copy_to_clipboard(text: &str) {
        use std::io::Write;
        #[cfg(windows)]
        let command = {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "clip"]);
            c
        };
        #[cfg(not(windows))]
        let command = {
            let mut c = std::process::Command::new("sh");
            c.args([
                "-c",
                "wl-copy 2>/dev/null || xclip -selection clipboard 2>/dev/null || xsel -b",
            ]);
            c
        };
        let mut command = command;
        if let Ok(mut child) = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            && let Some(stdin) = child.stdin.as_mut()
        {
            let _ = stdin.write_all(text.as_bytes());
        }
    }

    /// Regla única del compositor: sólo un `run.start` aceptado lo vacía.
    pub fn apply_submission_outcome(&mut self, accepted: bool, text: &str) {
        if accepted {
            self.agent.composer.clear();
        } else {
            self.agent.composer = text.to_string();
        }
    }

    pub fn run_prompt(&mut self, text: &str) -> bool {
        let Some(ws) = self.agent.workspace.as_ref().and_then(|w| w["id"].as_str()) else {
            return false;
        };
        let ws = ws.to_string();
        let Some(th) = self.ensure_thread(text) else {
            return false;
        };
        let Some(br) = self.ensure_branch() else {
            return false;
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
        if !self.agent.references.is_empty() {
            args["explicit_references"] = json!(self.explicit_reference_args());
        }
        if self.agent.provider == "sim"
            && let Ok(ms) = std::env::var("HLI_SIM_DELAY_MS")
            && let Ok(ms) = ms.parse::<u64>()
        {
            args["sim_delay_ms"] = json!(ms);
        }
        for warning in self.stale_references() {
            self.agent.push(AgentKind::Error, warning);
        }
        self.agent.busy = true;
        self.agent.run_state = "starting".into();
        let mut accepted = false;
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
                // Aceptado de forma durable: el compositor queda libre.
                self.apply_submission_outcome(true, text);
                accepted = true;
            }
            Err(e) => {
                self.agent.run_state = "failed".into();
                self.agent.push(AgentKind::Error, e);
                // No se creó el run: el prompt se conserva para reintentar.
                self.apply_submission_outcome(false, text);
            }
        }
        self.agent.busy = false;
        self.refresh_lists();
        self.refresh_runs();
        accepted
    }

    pub fn apply_run_status(&mut self, v: &Value) {
        let run = v.get("run").unwrap_or(v);
        let state = run["state"].as_str().unwrap_or("").to_string();
        if state.is_empty() {
            return;
        }
        let changed = state != self.agent.run_state;
        self.agent.run_state = state.clone();
        self.agent.approval_kind = run["approval_kind"].as_str().map(str::to_string);
        if changed {
            let text = if state == "planning" {
                "↻ Preparing plan".to_string()
            } else if state == "waiting_approval" {
                "Plan requires approval".to_string()
            } else {
                format!("run {state}")
            };
            self.agent.push(AgentKind::Info, text);
        }
        if is_terminal(&state) && !self.agent.receipt_shown {
            self.agent.receipt_shown = true;
            let ok = state == "completed";
            self.agent.push(
                if ok { AgentKind::Hli } else { AgentKind::Error },
                terminal_summary(run),
            );
        }
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
                let detail = event_detail(e).unwrap_or_default();
                self.agent.push_activity(line, detail);
                added += 1;
            }
            max_seq = max_seq.max(seq);
        }
        self.agent.last_seq = max_seq;
        added
    }

    /// Recarga el plan durable del run activo desde el daemon (fuente de verdad).
    pub fn load_plan(&mut self) {
        let Some(id) = self.agent.last_run.clone() else {
            self.agent.plan = None;
            return;
        };
        match client::call("run.plan", json!({ "run_id": id })) {
            Ok(v) => {
                let details = self.agent.plan.as_ref().map(|p| p.details).unwrap_or(false);
                let mut fresh = PlanView::from_value(&v);
                if let Some(p) = fresh.as_mut() {
                    p.details = details;
                }
                self.agent.plan = fresh;
            }
            Err(_) => {
                // Se conserva la última vista conocida; el overlay sólo se
                // dibuja si su estado sigue siendo waiting_approval.
            }
        }
    }

    /// Plan seleccionado según el modo activo: el overlay en Agent Mode o el
    /// run seleccionado en Inspector. No hay dos copias de lógica.
    pub fn selected_plan(&self) -> Option<PlanView> {
        if self.mode == TuiMode::Inspector {
            self.inspector_plan.clone()
        } else {
            self.agent.plan.clone()
        }
    }

    fn refresh_plan_views(&mut self) {
        if self.mode == TuiMode::Inspector {
            self.refresh_runs();
        }
        self.load_plan();
    }

    /// Recarga la solicitud de permiso pendiente del run activo.
    pub fn load_permission(&mut self) {
        let Some(id) = self.agent.last_run.clone() else {
            self.agent.perm = None;
            return;
        };
        if let Ok(v) = client::call(
            "permission.list",
            json!({"run_id": id, "status": "pending"}),
        ) {
            let details = self.agent.perm.as_ref().map(|p| p.details).unwrap_or(false);
            let mut perm = v
                .as_array()
                .and_then(|a| a.first())
                .and_then(PermissionView::from_value);
            if let Some(p) = perm.as_mut() {
                p.details = details;
            }
            self.agent.perm = perm;
        }
    }

    pub fn approve_permission(&mut self, scope: &str) {
        let Some(req) = self.agent.perm.clone() else {
            return;
        };
        match client::call(
            "permission.approve",
            json!({"request_id": req.request_id, "scope": scope}),
        ) {
            Ok(_) => {
                self.agent.push(
                    AgentKind::Info,
                    format!("permission allowed · {scope} · {}", req.tool),
                );
                self.agent.perm = None;
                self.agent.run_state = "running".into();
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent
                    .push(AgentKind::Error, format!("permission: {e}"));
                self.load_permission();
            }
        }
    }

    pub fn deny_permission(&mut self) {
        let Some(req) = self.agent.perm.clone() else {
            return;
        };
        match client::call("permission.deny", json!({"request_id": req.request_id})) {
            Ok(_) => {
                self.agent
                    .push(AgentKind::Info, format!("permission denied · {}", req.tool));
                self.agent.perm = None;
                self.agent.run_state = "failed".into();
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, format!("deny: {e}"));
                self.load_permission();
            }
        }
    }

    pub fn cancel_permission(&mut self) {
        let Some(req) = self.agent.perm.clone() else {
            return;
        };
        match client::call("permission.cancel", json!({"request_id": req.request_id})) {
            Ok(_) => {
                self.agent
                    .push(AgentKind::Info, "permission cancelled".to_string());
                self.agent.perm = None;
                self.agent.run_state = "cancelled".into();
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, format!("cancel: {e}"));
                self.load_permission();
            }
        }
    }

    pub fn approve_plan(&mut self, scope: &str) {
        let Some(plan) = self.selected_plan() else {
            return;
        };
        match client::call(
            "run.approve",
            json!({ "run_id": plan.run_id, "scope": scope, "plan_id": plan.plan_id }),
        ) {
            Ok(_) => {
                self.agent.push(
                    AgentKind::Info,
                    format!(
                        "approval requested · scope {scope} · revision {}",
                        plan.revision
                    ),
                );
                self.agent.plan = None;
                self.agent.run_state = "running".into();
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, format!("approve: {e}"));
                self.refresh_plan_views();
            }
        }
    }

    pub fn reject_plan(&mut self, reason: &str) {
        let Some(plan) = self.selected_plan() else {
            return;
        };
        let reason = if reason.trim().is_empty() {
            "rejected_by_user"
        } else {
            reason.trim()
        };
        match client::call(
            "run.reject",
            json!({ "run_id": plan.run_id, "plan_id": plan.plan_id, "reason": reason }),
        ) {
            Ok(_) => {
                self.agent
                    .push(AgentKind::Info, format!("approval rejected · {reason}"));
                self.agent.plan = None;
                self.agent.run_state = "cancelled".into();
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, format!("reject: {e}"));
                self.refresh_plan_views();
            }
        }
    }

    pub fn replan_plan(&mut self) {
        let Some(plan) = self.selected_plan() else {
            return;
        };
        match client::call("run.replan", json!({ "run_id": plan.run_id })) {
            Ok(v) => {
                let revision = v["plan"]["revision"].as_i64().unwrap_or(plan.revision + 1);
                self.agent.push(
                    AgentKind::Info,
                    format!("plan replanned · revision {revision}"),
                );
                self.refresh_plan_views();
            }
            Err(e) => {
                self.agent.push(AgentKind::Error, format!("replan: {e}"));
                self.refresh_plan_views();
            }
        }
    }

    pub fn cancel_approval(&mut self) {
        let reason = self
            .agent
            .plan
            .as_ref()
            .map(|p| format!("approval_cancelled_at_revision_{}", p.revision))
            .unwrap_or_else(|| "approval_cancelled".into());
        self.reject_plan(&reason);
    }

    fn poll_agent(&mut self) {
        let Some(id) = self.agent.last_run.clone() else {
            return;
        };
        if self.agent.poll_at.elapsed() < Duration::from_millis(350) {
            return;
        }
        self.agent.poll_at = Instant::now();

        // 1) Primero los eventos: el estado terminal nunca debe adelantarse a
        // los eventos que lo causaron.
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
            if let Ok(list) = client::call_ui("event.list", args)
                && let Some(arr) = list.as_array()
            {
                self.ingest_events(arr);
            }
        }
        // 2) Después el estado del run.
        if let Ok(v) = client::call_ui("run.status", json!({ "run_id": id })) {
            self.apply_run_status(&v);
        }

        // 3) En estado terminal se reconstruye la proyección durable: mismo
        // orden causal que el Event Store, sin duplicar ni adelantar líneas.
        if is_terminal(&self.agent.run_state) {
            if !self.agent.receipt_shown {
                self.agent.receipt_shown = true;
            }
            self.load_transcript();
            return;
        }

        if matches!(
            self.agent.run_state.as_str(),
            "planning" | "waiting_approval"
        ) {
            self.load_plan();
        }
        if self.agent.run_state == "waiting_approval" {
            self.load_permission();
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
            let _ = client::call_ui("run.cancel", json!({ "run_id": id }));
        }
    }

    /// `Ctrl+O` abre el Transcript Mode; en Inspector vuelve a Agent.
    pub fn toggle_mode(&mut self) {
        match self.mode {
            TuiMode::Agent => {
                self.agent.palette = false;
                self.help = false;
                self.open_transcript();
            }
            TuiMode::Transcript => self.close_transcript(),
            TuiMode::Inspector => self.mode = TuiMode::Agent,
            TuiMode::Settings => self.mode = TuiMode::Agent,
        }
    }

    /// Inspector: administración de runs, contexto, memoria y artefactos.
    pub fn open_inspector(&mut self) {
        self.agent.palette = false;
        self.help = false;
        self.mode = TuiMode::Inspector;
    }

    pub fn palette_items(&self) -> Vec<(&'static str, &'static str)> {
        let needle = self.agent.composer.trim_start_matches('/').to_lowercase();
        SLASH_COMMANDS
            .iter()
            .copied()
            .filter(|(name, _)| needle.is_empty() || name.starts_with(&needle))
            .collect()
    }

    // --- Pestañas: workspace → sesión → chat -------------------------------

    // --- Vista de configuración -------------------------------------------

    pub const SETTINGS_CATEGORIES: [&'static str; 6] = [
        "Sesión",
        "Proveedores",
        "Apariencia",
        "Módulos",
        "MCH",
        "Datos",
    ];

    pub fn open_settings(&mut self) {
        self.agent.palette = false;
        self.agent.composer.clear();
        self.agent.settings_cat = 0;
        self.agent.settings_sel = 0;
        self.agent.settings_loading = true;
        self.mode = TuiMode::Settings;
        self.daemon_async
            .request(crate::daemon_async::Request::Settings);
    }

    /// Filas de la categoría activa. Todo sale del estado o del daemon.
    pub fn settings_rows(&self) -> Vec<(String, String)> {
        let ws = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["name"].as_str())
            .unwrap_or("sin vincular")
            .to_string();
        let th = self
            .agent
            .thread
            .as_ref()
            .and_then(|t| t["title"].as_str())
            .unwrap_or("—")
            .to_string();
        match self.agent.settings_cat {
            0 => vec![
                (
                    "Modelo".into(),
                    format!("{}/{}", self.agent.provider, self.agent.model),
                ),
                ("Workspace".into(), ws),
                ("Sesión".into(), th),
                (
                    "Daemon".into(),
                    if self.daemon {
                        "en marcha".into()
                    } else {
                        "detenido".into()
                    },
                ),
                ("Inspector".into(), "↵ abrir".into()),
                ("Transcript".into(), "Ctrl+O".into()),
                ("Ayuda".into(), "? y /help".into()),
            ],
            1 => {
                let mut rows: Vec<(String, String)> = self
                    .agent
                    .settings_providers
                    .iter()
                    .map(|p| {
                        let name = p["name"].as_str().unwrap_or("-").to_string();
                        let kind = p["kind"].as_str().unwrap_or("-");
                        let model = p["model"].as_str().unwrap_or("-");
                        let state = if p["has_secret"].as_bool() == Some(true) {
                            "con secreto"
                        } else if name == "sim" {
                            "local"
                        } else {
                            "sin secreto"
                        };
                        (name, format!("{kind} · {model} · {state}"))
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        "(ninguno)".into(),
                        if self.agent.settings_loading {
                            "cargando…".into()
                        } else {
                            "sin proveedores en providers.toml".into()
                        },
                    ));
                }
                rows.push(("Añadir proveedor".into(), "hli provider add".into()));
                rows
            }
            2 => vec![
                (
                    "Color".into(),
                    if self.no_color {
                        "desactivado".into()
                    } else {
                        "activo".into()
                    },
                ),
                (
                    "ASCII".into(),
                    if self.ascii {
                        "sí".into()
                    } else {
                        "no".into()
                    },
                ),
                (
                    "Ratón".into(),
                    if self.mouse_enabled {
                        "activo".into()
                    } else {
                        "desactivado".into()
                    },
                ),
                (
                    "Rueda".into(),
                    format!("{} líneas", self.agent.viewport.wheel_speed),
                ),
                (
                    "Statusline".into(),
                    if Self::statusline_command().is_some() {
                        "configurada".into()
                    } else {
                        "sin ui.toml".into()
                    },
                ),
            ],
            3 => {
                let mut rows: Vec<(String, String)> = self
                    .agent
                    .settings_modules
                    .iter()
                    .map(|m| {
                        let name = m["name"].as_str().unwrap_or("-").to_string();
                        let version = m["version"].as_str().unwrap_or("-");
                        let signed = m["signed"].as_bool().unwrap_or(false);
                        (
                            name,
                            format!(
                                "{version} · {}",
                                if signed { "firmado" } else { "sin firma" }
                            ),
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        "(ninguno)".into(),
                        if self.agent.settings_loading {
                            "cargando…".into()
                        } else {
                            "sin módulos instalados".into()
                        },
                    ));
                }
                rows.push(("Instalar módulo".into(), "hli module install".into()));
                rows
            }
            4 => {
                let mut rows: Vec<(String, String)> = self
                    .agent
                    .settings_mchs
                    .iter()
                    .map(|m| {
                        (
                            m["name"].as_str().unwrap_or("-").to_string(),
                            m["state"].as_str().unwrap_or("-").to_string(),
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        "(ninguno)".into(),
                        if self.agent.settings_loading {
                            "cargando…".into()
                        } else {
                            "sin procesos MCH".into()
                        },
                    ));
                }
                rows.push(("Abrir Inspector MCH".into(), "↵ abrir".into()));
                rows
            }
            _ => vec![
                ("HLI_HOME".into(), self.home.display().to_string()),
                ("Versión".into(), env!("CARGO_PKG_VERSION").to_string()),
                (
                    "Precios".into(),
                    if self.agent.settings_prices == 0 {
                        "sin prices.yaml".into()
                    } else {
                        format!("{} entradas", self.agent.settings_prices)
                    },
                ),
                (
                    "Base de datos".into(),
                    self.home.join("hli.db").display().to_string(),
                ),
            ],
        }
    }

    pub fn settings_key(&mut self, k: KeyEvent) -> bool {
        let rows = self.settings_rows().len();
        match k.code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.agent.settings_cat = self.agent.settings_cat.saturating_sub(1);
                self.agent.settings_sel = 0;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                let last = Self::SETTINGS_CATEGORIES.len() - 1;
                self.agent.settings_cat = (self.agent.settings_cat + 1).min(last);
                self.agent.settings_sel = 0;
            }
            KeyCode::Up => {
                self.agent.settings_sel = self.agent.settings_sel.saturating_sub(1);
            }
            KeyCode::Down if self.agent.settings_sel + 1 < rows => {
                self.agent.settings_sel += 1;
            }
            KeyCode::Enter => {
                let row = self.settings_rows().get(self.agent.settings_sel).cloned();
                let Some((label, _)) = row else { return false };
                match (self.agent.settings_cat, label.as_str()) {
                    (0, "Modelo") => {
                        self.mode = TuiMode::Agent;
                        self.open_model_picker();
                    }
                    (0, "Inspector") => self.mode = TuiMode::Inspector,
                    (0, "Transcript") => self.open_transcript(),
                    (0, "Ayuda") => {
                        self.mode = TuiMode::Agent;
                        self.help = true;
                    }
                    (1, "Añadir proveedor") => {
                        self.agent.push(
                            AgentKind::Info,
                            "hli provider add <nombre> --kind openai --base-url URL --model M"
                                .to_string(),
                        );
                    }
                    (1, name) => {
                        if let Some(p) = self
                            .agent
                            .settings_providers
                            .iter()
                            .find(|p| p["name"].as_str() == Some(name))
                        {
                            self.agent.provider = name.to_string();
                            self.agent.model =
                                p["model"].as_str().unwrap_or(&self.agent.model).to_string();
                        }
                    }
                    (3, "Instalar módulo") => {
                        self.agent.push(
                            AgentKind::Info,
                            "hli module install <archivo.hlimod>".to_string(),
                        );
                    }
                    (4, _) => {
                        self.mode = TuiMode::Inspector;
                        self.tab = 5;
                    }
                    _ => {}
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.mode = TuiMode::Agent,
            _ => {}
        }
        false
    }

    pub fn cycle_tab(&mut self, forward: bool) {
        self.agent.palette = false;
        self.agent.agent_tab = match (self.agent.agent_tab, forward) {
            (AgentTab::Workspace, true) => AgentTab::Session,
            (AgentTab::Session, true) => AgentTab::Chat,
            (AgentTab::Chat, true) => AgentTab::Workspace,
            (AgentTab::Workspace, false) => AgentTab::Chat,
            (AgentTab::Session, false) => AgentTab::Workspace,
            (AgentTab::Chat, false) => AgentTab::Session,
        };
        if self.agent.agent_tab == AgentTab::Workspace {
            self.refresh_workspaces();
        } else if self.agent.agent_tab == AgentTab::Session {
            self.load_threads();
        }
    }

    pub fn refresh_workspaces(&mut self) {
        self.agent.lists_loading = true;
        self.daemon_async
            .request(crate::daemon_async::Request::Workspaces);
    }

    /// Enter en la pestaña Workspace: vincula y pasa a sesiones.
    pub fn select_workspace(&mut self) {
        let Some(ws) = self.workspaces.get(self.agent.ws_sel).cloned() else {
            return;
        };
        self.agent.workspace = Some(ws);
        self.agent.thread = None;
        self.agent.branch = None;
        // Las sesiones llegan por el worker; se eligen cuando estén.
        self.agent.lists_loading = true;
        self.load_threads();
        self.agent.agent_tab = AgentTab::Session;
    }

    /// Enter en la pestaña Sesión: entra al thread y abre el chat.
    pub fn select_thread(&mut self) {
        let Some(t) = self.threads.get(self.agent.thread_sel).cloned() else {
            return;
        };
        self.agent.thread = Some(t);
        self.agent.branch = None;
        self.load_branches();
        self.pick_last_branch();
        self.agent.lines.clear();
        self.agent.scroll_top = None;
        self.load_transcript();
        self.agent.agent_tab = AgentTab::Chat;
    }

    /// `n` en Workspace: crea el workspace del directorio actual.
    pub fn create_workspace_here(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let name = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string();
        match client::call_ui(
            "workspace.create",
            json!({ "name": name, "root": cwd.to_string_lossy() }),
        ) {
            Ok(ws) => {
                self.agent.workspace = Some(ws);
                self.refresh_workspaces();
                self.load_threads();
                self.agent.agent_tab = AgentTab::Session;
            }
            Err(e) => self.agent.push(AgentKind::Error, e),
        }
    }

    pub fn new_session(&mut self) {
        let Some(ws) = self.agent.workspace.as_ref().and_then(|w| w["id"].as_str()) else {
            return;
        };
        let ws = ws.to_string();
        match client::call_ui(
            "thread.create",
            json!({ "workspace_id": ws, "title": "session" }),
        ) {
            Ok(t) => {
                self.agent.thread = Some(t.clone());
                self.agent.branch = None;
                self.threads.push(t);
                self.agent.thread_sel = self.threads.len() - 1;
                self.load_branches();
                self.pick_last_branch();
                self.agent.lines.clear();
                self.agent.agent_tab = AgentTab::Chat;
                self.agent.push(AgentKind::Info, "Sesión nueva".to_string());
            }
            Err(e) => self.agent.push(AgentKind::Error, e),
        }
    }

    /// `/model`: proveedores reales del daemon, no una lista inventada.
    pub fn open_model_picker(&mut self) {
        // No bloquea: abre el selector vacío y pide los proveedores al worker.
        self.agent.providers.clear();
        self.agent.model_sel = 0;
        self.agent.model_loading = true;
        self.agent.model_open = true;
        self.agent.palette = false;
        self.agent.composer.clear();
        self.daemon_async
            .request(crate::daemon_async::Request::Providers);
    }

    /// Los tres que HLI sabe usar; se ofrecen aunque no estén configurados.
    fn ensure_known_providers(&mut self) {
        for (name, kind, model) in [
            ("sim", "sim", "sim-1"),
            ("ollama", "ollama", "llama3.1:8b"),
            ("openai", "openai", "gpt-4o-mini"),
        ] {
            if !self
                .agent
                .providers
                .iter()
                .any(|p| p["name"].as_str() == Some(name))
            {
                self.agent.providers.push(json!({
                    "name": name,
                    "kind": kind,
                    "model": model,
                    "available": name == "sim",
                }));
            }
        }
    }

    fn apply_daemon_reply(&mut self, reply: crate::daemon_async::Reply) {
        match reply {
            crate::daemon_async::Reply::Providers(result) => {
                let mut default = String::new();
                if let Ok(v) = result {
                    self.agent.providers = v["providers"].as_array().cloned().unwrap_or_default();
                    default = v["default_provider"].as_str().unwrap_or("").to_string();
                }
                self.ensure_known_providers();
                self.agent.model_loading = false;
                // Abre en el proveedor de la sesión; si no está, en el default
                // configurado; si tampoco, en el primero.
                self.agent.model_sel = self
                    .agent
                    .providers
                    .iter()
                    .position(|p| p["name"].as_str() == Some(self.agent.provider.as_str()))
                    .or_else(|| {
                        self.agent
                            .providers
                            .iter()
                            .position(|p| p["name"].as_str() == Some(default.as_str()))
                    })
                    .unwrap_or(0);
            }
            crate::daemon_async::Reply::Workspaces(result) => {
                if let Ok(v) = result {
                    self.workspaces = v.as_array().cloned().unwrap_or_default();
                }
                let current = self
                    .agent
                    .workspace
                    .as_ref()
                    .and_then(|w| w["id"].as_str())
                    .map(str::to_string);
                self.agent.ws_sel = self
                    .workspaces
                    .iter()
                    .position(|w| w["id"].as_str().map(str::to_string) == current)
                    .unwrap_or(0);
                self.agent.lists_loading = false;
            }
            crate::daemon_async::Reply::Threads(result) => {
                if let Ok(v) = result {
                    self.threads = v.as_array().cloned().unwrap_or_default();
                }
                self.agent.thread_sel = self.threads.len().saturating_sub(1);
                self.agent.lists_loading = false;
            }
            crate::daemon_async::Reply::Settings(result) => {
                if let Ok(v) = result {
                    self.agent.settings_providers = v["providers"]["providers"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    self.agent.settings_modules =
                        v["modules"].as_array().cloned().unwrap_or_default();
                    self.agent.settings_mchs = v["mchs"].as_array().cloned().unwrap_or_default();
                    self.agent.settings_prices = v["prices"]
                        .as_array()
                        .map(|a| a.len())
                        .or_else(|| v["prices"].as_object().map(|o| o.len()))
                        .unwrap_or(0);
                }
                self.agent.settings_loading = false;
            }
            crate::daemon_async::Reply::Transcript(result) => match result {
                Ok(v) => self.apply_transcript(&v),
                Err(e) => {
                    self.agent.transcript_loading = false;
                    self.agent.push(AgentKind::Error, e);
                }
            },
        }
    }

    /// Aplica lo que el worker haya terminado. Sin esperas. Si el worker
    /// murió, limpia los estados de carga: nunca se queda en `cargando…`.
    pub fn drain_daemon(&mut self) {
        loop {
            match self.daemon_async.poll() {
                crate::daemon_async::Poll::Reply(reply) => self.apply_daemon_reply(reply),
                crate::daemon_async::Poll::Empty => break,
                crate::daemon_async::Poll::Closed => {
                    self.agent.model_loading = false;
                    self.agent.lists_loading = false;
                    self.agent.transcript_loading = false;
                    self.agent.settings_loading = false;
                    if !self.agent.daemon_closed {
                        self.agent.daemon_closed = true;
                        self.agent.push(
                            AgentKind::Error,
                            "el worker del daemon se detuvo; reinicia hli".to_string(),
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Solo para tests: espera el resultado con un techo.
    pub fn pump_daemon(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut got = false;
        while std::time::Instant::now() < deadline {
            if let Some(reply) = self.daemon_async.try_recv() {
                self.apply_daemon_reply(reply);
                got = true;
                continue;
            }
            if got {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if !got {
            self.ensure_known_providers();
            self.agent.model_loading = false;
            self.agent.lists_loading = false;
            self.agent.transcript_loading = false;
            self.agent.settings_loading = false;
        }
    }

    pub fn apply_model(&mut self) {
        let chosen = self.agent.providers.get(self.agent.model_sel).cloned();
        if let Some(p) = chosen {
            self.agent.provider = p["name"].as_str().unwrap_or("sim").to_string();
            self.agent.model = p["model"].as_str().unwrap_or("sim-1").to_string();
            let line = format!("Modelo: {}/{}", self.agent.provider, self.agent.model);
            self.agent.push(AgentKind::Info, line);
            if p["available"].as_bool() == Some(false) {
                self.agent.push(
                    AgentKind::Info,
                    format!(
                        "{} sin configurar · hli provider set {}",
                        self.agent.provider, self.agent.provider
                    ),
                );
            }
        }
        self.agent.model_open = false;
    }

    pub fn run_slash(&mut self, line: &str) -> bool {
        let raw = line.trim_start_matches('/');
        let mut parts = raw.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_string();
        let args: Vec<String> = parts.map(str::to_string).collect();
        self.agent.palette = false;
        self.agent.composer.clear();
        match cmd.as_str() {
            "" => {}
            "help" => {
                self.help = true;
                self.agent.push(
                    AgentKind::Info,
                    format!("disponibles: /{}", available_commands().join(" /")),
                );
            }
            "status" => {
                let s = self.agent_status_line();
                self.agent.push(AgentKind::Info, s);
            }
            "inspect" => self.open_inspector(),
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
            "clear" => {
                let armed = self
                    .agent
                    .clear_armed
                    .map(|t| t.elapsed() < Duration::from_secs(5))
                    .unwrap_or(false);
                if armed {
                    self.agent.lines.clear();
                    self.agent.activity_sel = None;
                    self.agent.clear_armed = None;
                    self.agent
                        .push(AgentKind::Info, "transcript limpiado".to_string());
                } else {
                    self.agent.clear_armed = Some(Instant::now());
                    self.agent.push(
                        AgentKind::Info,
                        "escribe /clear otra vez en 5 s para borrar el transcript".to_string(),
                    );
                }
            }
            "details" => self.toggle_all_details(),
            "cancel" => self.request_cancel(),
            "resume" => self.open_resume(),
            "workspace" => {
                self.refresh_workspaces();
                self.agent.agent_tab = AgentTab::Workspace;
            }
            "sessions" => {
                self.load_threads();
                self.agent.agent_tab = AgentTab::Session;
            }
            "settings" | "config" => self.open_settings(),
            "exit" => return true,
            "new" => {
                if let Some(ws) = self
                    .agent
                    .workspace
                    .as_ref()
                    .and_then(|w| w["id"].as_str())
                    .map(str::to_string)
                {
                    match client::call_ui(
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
            "branch" => self.slash_branch(&args),
            "provider" | "model" => self.open_model_picker(),
            "export" => self.slash_export(&args),
            other => self.agent.push(
                AgentKind::Info,
                format!(
                    "/{other} no existe. disponibles: /{}",
                    available_commands().join(" /")
                ),
            ),
        }
        // La respuesta del comando tiene que verse: seguir el final.
        self.agent.scroll_top = None;
        self.agent.viewport.follow_mode = true;
        self.agent.viewport.unseen_activity_count = 0;
        self.agent.palette_index = 0;
        false
    }

    fn slash_branch(&mut self, args: &[String]) {
        if args.first().map(String::as_str) == Some("new") {
            let name = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| format!("alternative-{}", self.branches.len() + 1));
            self.agent.offer = Offer::CreateBranch { name };
            return;
        }
        if let Some(name) = args.first() {
            if let Some(b) = self
                .branches
                .iter()
                .find(|b| b["name"].as_str() == Some(name.as_str()))
                .cloned()
            {
                self.agent.branch = Some(b);
                self.agent.push(AgentKind::Info, format!("branch: {name}"));
            } else {
                self.agent
                    .push(AgentKind::Error, format!("branch desconocida: {name}"));
            }
            return;
        }
        self.load_branches();
        let names: Vec<String> = self
            .branches
            .iter()
            .filter_map(|b| b["name"].as_str().map(str::to_string))
            .collect();
        self.agent.push(
            AgentKind::Info,
            if names.is_empty() {
                "sin branches".to_string()
            } else {
                format!("branches: {}", names.join(", "))
            },
        );
    }

    fn slash_export(&mut self, args: &[String]) {
        let Some(ws) = self
            .agent
            .workspace
            .as_ref()
            .and_then(|w| w["id"].as_str())
            .map(str::to_string)
        else {
            return;
        };
        let out = args.first().cloned().unwrap_or_else(|| {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("hli-export-{stamp}.hliar")
        });
        match client::call("export", json!({ "workspace_id": ws, "out": out })) {
            Ok(v) => self.agent.push(
                AgentKind::Info,
                format!("export: {}", v["out"].as_str().unwrap_or(&out)),
            ),
            Err(e) => self.agent.push(AgentKind::Error, e),
        }
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
        if self.agent.preflight.is_some() {
            match k.code {
                KeyCode::Char('r') => self.refresh_preflight(),
                KeyCode::Char('c') | KeyCode::Esc => self.cancel_preflight(),
                _ => {}
            }
            return false;
        }
        if self.agent.ref_open {
            return self.handle_ref_key(k);
        }
        if self.agent.resume_open {
            return self.handle_resume_key(k);
        }
        if self.agent.history_open {
            return self.handle_history_key(k);
        }
        if self.agent.model_open {
            match k.code {
                KeyCode::Esc => self.agent.model_open = false,
                KeyCode::Up => self.agent.model_sel = self.agent.model_sel.saturating_sub(1),
                KeyCode::Down if self.agent.model_sel + 1 < self.agent.providers.len() => {
                    self.agent.model_sel += 1;
                }
                KeyCode::Enter => self.apply_model(),
                _ => {}
            }
            return false;
        }
        // Pestañas: workspace → sesión → chat. `Tab` cicla; `Alt+1/2/3` salta.
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Tab => {
                self.cycle_tab(true);
                return false;
            }
            KeyCode::BackTab => {
                self.cycle_tab(false);
                return false;
            }
            KeyCode::Char('1') if alt => {
                self.refresh_workspaces();
                self.agent.agent_tab = AgentTab::Workspace;
                return false;
            }
            KeyCode::Char('2') if alt => {
                self.load_threads();
                self.agent.agent_tab = AgentTab::Session;
                return false;
            }
            KeyCode::Char('3') if alt => {
                self.agent.agent_tab = AgentTab::Chat;
                return false;
            }
            _ => {}
        }
        if self.agent.agent_tab != AgentTab::Chat {
            let n = match self.agent.agent_tab {
                AgentTab::Workspace => self.workspaces.len(),
                AgentTab::Session => self.threads.len(),
                AgentTab::Chat => 0,
            };
            let sel = match self.agent.agent_tab {
                AgentTab::Workspace => self.agent.ws_sel,
                AgentTab::Session => self.agent.thread_sel,
                AgentTab::Chat => 0,
            };
            let next = match k.code {
                KeyCode::Up => Some(sel.saturating_sub(1)),
                KeyCode::Down if sel + 1 < n => Some(sel + 1),
                _ => None,
            };
            if let Some(v) = next {
                match self.agent.agent_tab {
                    AgentTab::Workspace => self.agent.ws_sel = v,
                    AgentTab::Session => self.agent.thread_sel = v,
                    AgentTab::Chat => {}
                }
            }
            match k.code {
                KeyCode::Up | KeyCode::Down => {}
                KeyCode::Enter => match self.agent.agent_tab {
                    AgentTab::Workspace => self.select_workspace(),
                    AgentTab::Session => self.select_thread(),
                    AgentTab::Chat => {}
                },
                KeyCode::Char('n') => match self.agent.agent_tab {
                    AgentTab::Workspace => self.create_workspace_here(),
                    AgentTab::Session => self.new_session(),
                    AgentTab::Chat => {}
                },
                KeyCode::Esc => self.agent.agent_tab = AgentTab::Chat,
                _ => {}
            }
            return false;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        if self.agent.reject_open {
            match k.code {
                KeyCode::Enter => {
                    let reason = self.agent.reject_reason.clone();
                    self.agent.reject_open = false;
                    self.agent.reject_reason.clear();
                    self.reject_plan(&reason);
                }
                KeyCode::Esc => {
                    self.agent.reject_open = false;
                    self.agent.reject_reason.clear();
                }
                KeyCode::Backspace => {
                    self.agent.reject_reason.pop();
                }
                KeyCode::Char(c) => self.agent.reject_reason.push(c),
                _ => {}
            }
            return false;
        }
        if let Some(perm) = self.agent.perm.clone()
            && perm.pending()
        {
            match k.code {
                KeyCode::Char('o') if ctrl => self.open_transcript(),
                KeyCode::Char('i') if ctrl => self.open_inspector(),
                KeyCode::Char('y') => self.approve_permission("once"),
                KeyCode::Char('a') => self.approve_permission("run"),
                KeyCode::Char('w') => self.approve_permission("workspace"),
                KeyCode::Char('n') => self.deny_permission(),
                KeyCode::Char('v') => {
                    if let Some(mut p) = self.agent.perm.clone() {
                        p.details = !p.details;
                        self.agent.perm = Some(p);
                    }
                }
                KeyCode::Esc => self.cancel_permission(),
                _ => {}
            }
            return false;
        }
        if let Some(plan) = self.agent.plan.clone()
            && plan.waiting()
        {
            // El overlay consume el teclado: el compositor no envía prompts.
            match k.code {
                KeyCode::Char('o') if ctrl => self.open_transcript(),
                KeyCode::Char('i') if ctrl => self.open_inspector(),
                KeyCode::Char('y') => self.approve_plan("once"),
                KeyCode::Char('a') => self.approve_plan("run"),
                KeyCode::Char('w') => self.approve_plan("workspace"),
                KeyCode::Char('n') => {
                    self.agent.reject_open = true;
                    self.agent.reject_reason.clear();
                }
                KeyCode::Char('v') => {
                    if let Some(mut p) = self.agent.plan.clone() {
                        p.details = !p.details;
                        self.agent.plan = Some(p);
                    }
                }
                KeyCode::Char('r') => self.replan_plan(),
                KeyCode::Esc => self.cancel_approval(),
                _ => {}
            }
            return false;
        }
        // Desplazamiento del transcript. Home/End simples pertenecen al
        // compositor; sólo con Ctrl mueven el transcript.
        match k.code {
            KeyCode::PageUp => {
                self.viewport_page(-1);
                return false;
            }
            KeyCode::PageDown => {
                self.viewport_page(1);
                return false;
            }
            KeyCode::Home if ctrl => {
                self.viewport_start();
                return false;
            }
            KeyCode::End if ctrl => {
                self.viewport_bottom();
                return false;
            }
            _ => {}
        }
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
            KeyCode::Char('i') if ctrl => self.open_inspector(),
            KeyCode::Char('j') if ctrl => self.agent.composer.push('\n'),
            KeyCode::Enter if shift => self.agent.composer.push('\n'),
            KeyCode::Enter => {
                if self.agent.offer != Offer::None {
                    self.accept_offer();
                } else if self.agent.palette {
                    // Enter ejecuta la opción seleccionada del menú, no solo
                    // lo que se haya escrito.
                    let items = self.palette_items();
                    let line = match items.get(self.agent.palette_index) {
                        Some((name, _)) => format!("/{name}"),
                        None => self.agent.composer.clone(),
                    };
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
            KeyCode::Char('e') if ctrl => self.toggle_selected_activity(),
            KeyCode::PageUp => self.scroll_page_up(),
            KeyCode::PageDown => self.scroll_page_down(),
            KeyCode::Home => self.scroll_home(),
            KeyCode::End => self.scroll_end(),
            KeyCode::Up if ctrl => self.scroll_lines(-3),
            KeyCode::Down if ctrl => self.scroll_lines(3),
            KeyCode::Up if self.agent.palette => {
                self.agent.palette_index = self.agent.palette_index.saturating_sub(1);
            }
            KeyCode::Down if self.agent.palette => {
                let n = self.palette_items().len();
                if n > 0 && self.agent.palette_index + 1 < n {
                    self.agent.palette_index += 1;
                }
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
            KeyCode::Char('@') => {
                let at = self.agent.composer.len();
                self.agent.composer.push('@');
                self.open_reference_selector(at);
            }
            KeyCode::Backspace => {
                self.agent.composer.pop();
                if !self.agent.composer.starts_with('/') {
                    self.agent.palette = false;
                }
            }
            KeyCode::Char(c) => {
                self.agent.composer.push(c);
                if self.agent.palette {
                    self.agent.palette_index = 0;
                }
            }
            _ => {}
        }
        false
    }
}
