use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};
use ratatui::Frame;
use serde_json::Value;

use crate::app::App;

pub struct Theme {
    pub text: Color,
    pub primary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub mch: Color,
    pub memory: Color,
    pub artifact: Color,
    pub mono: bool,
}

impl Theme {
    pub fn for_app(app: &App) -> Self {
        if app.no_color || std::env::var("NO_COLOR").is_ok() {
            return Self {
                text: Color::Reset, primary: Color::Reset, success: Color::Reset,
                warning: Color::Reset, error: Color::Reset, mch: Color::Reset,
                memory: Color::Reset, artifact: Color::Reset, mono: true,
            };
        }
        Self {
            text: Color::Rgb(201, 209, 217),
            primary: Color::Rgb(47, 129, 247),
            success: Color::Rgb(63, 185, 80),
            warning: Color::Rgb(210, 153, 34),
            error: Color::Rgb(248, 81, 73),
            mch: Color::Rgb(163, 113, 247),
            memory: Color::Rgb(57, 197, 207),
            artifact: Color::Rgb(139, 148, 158),
            mono: false,
        }
    }
    fn style(&self, c: Color) -> Style {
        if self.mono { Style::default() } else { Style::default().fg(c) }
    }
    fn bold(&self, c: Color) -> Style {
        if self.mono { Style::default().add_modifier(Modifier::BOLD) }
        else { Style::default().fg(c).add_modifier(Modifier::BOLD) }
    }
}

pub const TABS: [&str; 6] = ["Run", "Events", "Context", "Memory", "Artifacts", "MCH"];

pub fn draw(f: &mut Frame, app: &App) {
    let t = Theme::for_app(app);
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)])
        .split(area);
    header(f, chunks[0], app, &t);
    if area.width >= 100 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(26), Constraint::Percentage(32), Constraint::Percentage(42)])
            .split(chunks[1]);
        left(f, cols[0], app, &t);
        center(f, cols[1], app, &t);
        right(f, cols[2], app, &t);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3)])
            .split(chunks[1]);
        tabs(f, rows[0], app, &t);
        right(f, rows[1], app, &t);
    }
    footer(f, chunks[2], app, &t);
}

fn header(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let ws = app.workspaces.first().and_then(|w| w["name"].as_str()).unwrap_or("-");
    let br = app.branches.first().and_then(|b| b["name"].as_str()).unwrap_or("-");
    let run = app.selected_run.as_ref();
    let provider = run.and_then(|r| r["receipt"]["provider"].as_str()).unwrap_or("-");
    let daemon = if app.daemon { "up" } else { "down" };
    let line = Line::from(vec![
        Span::styled(" HLI ", t.bold(t.primary)),
        Span::styled(format!("· {ws} · {br} · {provider} · daemon:{daemon}"), t.style(t.text)),
    ]);
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).border_style(t.style(t.primary))),
        area,
    );
}

fn tabs(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let tabs = Tabs::new(TABS.iter().map(|s| Line::from(*s)).collect::<Vec<_>>())
        .select(app.tab)
        .style(t.style(t.text))
        .highlight_style(t.bold(t.primary));
    f.render_widget(tabs, area);
}

fn left(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("Workspaces", t.bold(t.primary))));
    for w in &app.workspaces {
        lines.push(Line::from(format!("  {}  {}", w["id"].as_str().unwrap_or("-"), w["name"].as_str().unwrap_or(""))));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Threads", t.bold(t.primary))));
    for th in &app.threads {
        lines.push(Line::from(format!("  {}  {}", th["id"].as_str().unwrap_or("-"), th["title"].as_str().unwrap_or(""))));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Branches", t.bold(t.primary))));
    for (i, b) in app.branches.iter().enumerate() {
        let prefix = if i == 0 { "●" } else if i + 1 == app.branches.len() { "└─●" } else { "├─●" };
        lines.push(Line::from(format!(
            "  {prefix} {}  seq {}",
            b["name"].as_str().unwrap_or("-"),
            b["head_sequence"].as_i64().unwrap_or(0)
        )));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Navegación "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn center(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let mut lines = Vec::new();
    match &app.selected_run {
        Some(r) => {
            let state = r["state"].as_str().unwrap_or("-");
            let color = match state {
                "completed" => t.success,
                "failed" => t.error,
                "waiting_approval" => t.warning,
                _ => t.primary,
            };
            lines.push(Line::from(vec![
                Span::styled("estado ", t.style(t.text)),
                Span::styled(state.to_string(), t.bold(color)),
            ]));
            lines.push(Line::from(format!("run      {}", r["id"].as_str().unwrap_or("-"))));
            lines.push(Line::from(format!("provider {}/{}", r["receipt"]["provider"].as_str().unwrap_or("-"), r["receipt"]["model"].as_str().unwrap_or("-"))));
            lines.push(Line::from(format!("revisión {}", r["branch_revision"].as_i64().unwrap_or(0))));
            lines.push(Line::from(format!(
                "tokens   {} in / {} out",
                r["receipt"]["usage"]["input_tokens"].as_i64().unwrap_or(0),
                r["receipt"]["usage"]["output_tokens"].as_i64().unwrap_or(0)
            )));
            lines.push(Line::from(format!(
                "coste    ${:.6}",
                r["receipt"]["cost"]["total"].as_f64().unwrap_or(0.0)
            )));
            lines.push(Line::from(format!(
                "latencia {} ms",
                r["receipt"]["usage"]["latency_total_ms"].as_i64().unwrap_or(0)
            )));
        }
        None => lines.push(Line::from("sin runs")),
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Run "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn right(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let title = format!(" {} ", TABS[app.tab]);
    let lines = match app.tab {
        0 => run_lines(app, t),
        1 => event_lines(app, t),
        2 => context_lines(app, t),
        3 => memory_lines(app, t),
        4 => artifact_lines(app, t),
        _ => mch_lines(app, t),
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn run_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("Runs", t.bold(t.primary)))];
    for r in &app.runs {
        v.push(Line::from(format!(
            "  {}  {}",
            r["id"].as_str().unwrap_or("-"),
            r["state"].as_str().unwrap_or("-")
        )));
    }
    if let Some(m) = &app.manifest {
        v.push(Line::from(""));
        v.push(Line::from(Span::styled("ContextManifest", t.bold(t.primary))));
        v.push(Line::from(format!("  presupuesto {}", m["budget_tokens"].as_i64().unwrap_or(0))));
        v.push(Line::from(format!("  estimado   {}", m["estimated_tokens"].as_i64().unwrap_or(0))));
        v.push(Line::from(format!("  payload    {}", m["payload_hash"].as_str().unwrap_or("-"))));
    }
    v
}

fn event_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("Eventos", t.bold(t.primary)))];
    for e in app.events.iter().rev().take(20) {
        v.push(Line::from(format!(
            "  {:>4}  {:<28} {}  {}",
            e["sequence"].as_i64().unwrap_or(0),
            e["type"].as_str().unwrap_or("-"),
            e["actor"]["kind"].as_str().unwrap_or("-"),
            e["created_at"].as_str().unwrap_or("-")
        )));
    }
    v
}

fn context_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("ContextManifest", t.bold(t.primary)))];
    if let Some(m) = &app.manifest {
        v.push(Line::from(format!("presupuesto {} · estimado {}", m["budget_tokens"], m["estimated_tokens"])));
        v.push(Line::from(format!("user_input  {}", m["user_input"]["content"].as_str().unwrap_or("-"))));
        v.push(Line::from(format!("payload_hash {}", m["payload_hash"].as_str().unwrap_or("-"))));
        let mems = m["memories"].as_array().map(|a| a.len()).unwrap_or(0);
        let slices = m["artifact_slices"].as_array().map(|a| a.len()).unwrap_or(0);
        let excl = m["excluded"].as_array().map(|a| a.len()).unwrap_or(0);
        v.push(Line::from(format!("memorias {mems} · slices {slices} · excluidos {excl}")));
        for s in m["artifact_slices"].as_array().cloned().unwrap_or_default() {
            v.push(Line::from(format!("  slice {} · {}", s["slice_id"].as_str().unwrap_or("-"), s["reason"].as_str().unwrap_or(""))));
        }
    } else {
        v.push(Line::from("sin manifiesto"));
    }
    v
}

fn memory_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("Memorias", t.bold(t.memory)))];
    for status in ["active", "superseded", "revoked", "proposed"] {
        let sel: Vec<&Value> = app.memories.iter().filter(|m| m["status"].as_str() == Some(status)).collect();
        if sel.is_empty() {
            continue;
        }
        v.push(Line::from(Span::styled(format!("{status} ({})", sel.len()), t.bold(t.memory))));
        for m in sel {
            let extra = if status == "superseded" {
                format!(" · superseded_at {}", m["superseded_at"].as_str().unwrap_or("-"))
            } else {
                String::new()
            };
            v.push(Line::from(format!(
                "  {}  {}{}  [{}]",
                m["id"].as_str().unwrap_or("-"),
                m["content"].as_str().unwrap_or(""),
                extra,
                m["source_authority"].as_str().unwrap_or("user")
            )));
        }
    }
    v
}

fn artifact_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("Artefactos", t.bold(t.artifact)))];
    for a in &app.artifacts {
        v.push(Line::from(format!(
            "  {}  {}  {} bytes  {}",
            a["id"].as_str().unwrap_or("-"),
            a["mime"].as_str().unwrap_or("-"),
            a["bytes"].as_i64().unwrap_or(0),
            a["sha256"].as_str().unwrap_or("-")
        )));
    }
    if app.artifacts.is_empty() {
        v.push(Line::from("sin artefactos"));
    }
    v
}

fn mch_lines(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(Span::styled("MCH", t.bold(t.mch)))];
    for m in &app.mchs {
        v.push(Line::from(Span::styled(format!("Profile: {}", m["profile"].as_str().unwrap_or("-")), t.bold(t.mch))));
        v.push(Line::from(format!("  módulo {} · dominio {}", m["module"].as_str().unwrap_or("-"), m["domain"].as_str().unwrap_or("-"))));
        v.push(Line::from(Span::styled(format!("Instance: {} [{}]", m["name"].as_str().unwrap_or("-"), m["state"].as_str().unwrap_or("-")), t.bold(t.mch))));
        v.push(Line::from(format!("  workspace {}", m["workspace_id"].as_str().unwrap_or("-"))));
        v.push(Line::from("Process: supervisado por HLI (subproceso por invocación)"));
        v.push(Line::from(format!("Endpoint: CLI hli mch invoke {} · MCP mch_invoke", m["name"].as_str().unwrap_or("-"))));
    }
    if app.mchs.is_empty() {
        v.push(Line::from("sin instancias MCH"));
    }
    v
}

fn footer(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    let line = Line::from(vec![
        Span::styled("> ", t.bold(t.primary)),
        Span::styled(app.input.clone(), t.style(t.text)),
    ]);
    let status = Line::from(Span::styled(format!(" {}", app.status), t.style(t.artifact)));
    f.render_widget(
        Paragraph::new(vec![line, status]).block(Block::default().borders(Borders::ALL)),
        area,
    );
}
