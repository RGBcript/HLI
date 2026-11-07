use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::client;

pub struct App {
    pub home: PathBuf,
    pub no_color: bool,
    pub tab: usize,
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
    pub daemon: bool,
    pub sel_run: usize,
    last_runs: Instant,
    last_events: Instant,
    last_lists: Instant,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            home: client::home(),
            no_color: std::env::var("NO_COLOR").is_ok(),
            tab: 0,
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
            status: "listo · q sale · Tab cambia pestaña · Enter ejecuta comando".into(),
            daemon: false,
            sel_run: 0,
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
            "receipt": {
                "usage": {"input_tokens": 120, "output_tokens": 8, "latency_total_ms": 540},
                "cost": {"total": 0.0001, "currency": "USD"}
            }
        });
        Self {
            home: PathBuf::from("."),
            no_color: false,
            tab: 0,
            workspaces: vec![json!({"id": "ws_demo", "name": "demo"})],
            threads: vec![json!({"id": "thr_demo", "title": "research"})],
            branches: vec![
                json!({"id": "br_main", "name": "main", "head_sequence": 5}),
                json!({"id": "br_adapt", "name": "adaptive-k", "head_sequence": 9}),
                json!({"id": "br_super", "name": "supersession", "head_sequence": 7}),
            ],
            runs: vec![run.clone()],
            events: vec![
                json!({"sequence": 4, "type": "context.manifest.created", "actor": {"kind":"agent"}, "causation_id": null, "correlation_id": "req_1", "created_at": "2026-11-07T10:00:00Z"}),
                json!({"sequence": 5, "type": "model.requested", "actor": {"kind":"agent"}, "causation_id": "evt_4", "correlation_id": "req_1", "created_at": "2026-11-07T10:00:01Z"}),
            ],
            memories: vec![
                json!({"id": "mem_1", "status": "active", "content": "SQLite es la fuente de verdad.", "source_authority": "user", "superseded_at": null}),
                json!({"id": "mem_0", "status": "superseded", "content": "PostgreSQL era la fuente.", "source_authority": "legacy", "superseded_at": "2026-11-07T09:00:00Z"}),
            ],
            artifacts: vec![json!({"id": "art_1", "mime": "text/plain", "bytes": 42, "sha256": "sha256:abc"})],
            mchs: vec![json!({"name": "memory-main", "profile": "memory", "state": "ready", "module": "hli-memory", "domain": "memory", "workspace_id": "ws_demo"})],
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
            daemon: true,
            sel_run: 0,
            last_runs: Instant::now(),
            last_events: Instant::now(),
            last_lists: Instant::now(),
        }
    }

    pub fn refresh_lists(&mut self) {
        self.daemon = client::call("daemon.ping", json!({})).is_ok();
        self.workspaces = client::call("workspace.list", json!({})).unwrap_or_default().as_array().cloned().unwrap_or_default();
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            self.threads = client::call("thread.list", json!({"workspace_id": id})).unwrap_or_default().as_array().cloned().unwrap_or_default();
            self.memories = client::call("memory.list", json!({"workspace_id": id})).unwrap_or_default().as_array().cloned().unwrap_or_default();
            self.artifacts = client::call("artifact.list", json!({"workspace_id": id})).unwrap_or_default().as_array().cloned().unwrap_or_default();
            if let Some(th) = self.threads.first() {
                self.branches = client::call("branch.list", json!({"thread_id": th["id"]})).unwrap_or_default().as_array().cloned().unwrap_or_default();
            }
        }
        self.mchs = client::call("mch.list", json!({})).unwrap_or_default().as_array().cloned().unwrap_or_default();
        self.last_lists = Instant::now();
    }

    pub fn refresh_runs(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            if let Ok(v) = client::call("run.list", json!({"workspace_id": id})) {
                self.runs = v.as_array().cloned().unwrap_or_default();
            }
        }
        if let Some(run) = self.runs.get(self.sel_run).cloned() {
            let rid = run["id"].as_str().unwrap_or("").to_string();
            if let Ok(got) = client::call("run.get", json!({"run_id": rid})) {
                self.selected_run = Some(got["run"].clone());
                self.manifest = Some(got["manifest"].clone());
            }
        }
        self.last_runs = Instant::now();
    }

    pub fn refresh_events(&mut self) {
        if let Some(ws) = self.workspaces.first() {
            let id = ws["id"].as_str().unwrap_or("");
            if let Ok(v) = client::call("event.list", json!({"workspace_id": id})) {
                self.events = v.as_array().cloned().unwrap_or_default();
            }
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
    }

    pub fn next_tab(&mut self) {
        self.tab = (self.tab + 1) % 6;
    }
    pub fn prev_tab(&mut self) {
        self.tab = (self.tab + 5) % 6;
    }

    pub fn run_command(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
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
                let text = if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&o.stderr).to_string()
                };
                self.status = text.lines().next().unwrap_or("ok").to_string();
                self.refresh_lists();
                self.refresh_runs();
            }
            Err(e) => self.status = format!("error: {e}"),
        }
        self.input.clear();
    }
}

/// Divide respetando comillas simples y dobles; el parser real es el de hli-cli.
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
