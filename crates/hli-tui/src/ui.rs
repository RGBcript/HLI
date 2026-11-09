use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use serde_json::Value;

use crate::app::{
    AgentKind, App, FeedbackKind, FocusArea, LayoutMode, Offer, TuiMode, layout_mode,
};

pub const TABS: [&str; 6] = ["Run", "Events", "Context", "Memory", "Artifacts", "MCH"];

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
                text: Color::Reset,
                primary: Color::Reset,
                success: Color::Reset,
                warning: Color::Reset,
                error: Color::Reset,
                mch: Color::Reset,
                memory: Color::Reset,
                artifact: Color::Reset,
                mono: true,
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
    fn s(&self, c: Color) -> Style {
        if self.mono {
            Style::default()
        } else {
            Style::default().fg(c)
        }
    }
    fn b(&self, c: Color) -> Style {
        if self.mono {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c).add_modifier(Modifier::BOLD)
        }
    }
}

fn block(title: &str, focused: bool, t: &Theme) -> Block<'static> {
    let b = Block::default().borders(Borders::ALL);
    if focused {
        b.border_type(BorderType::Double)
            .border_style(t.b(t.primary))
            .title(Span::styled(format!("▸ {title} "), t.b(t.primary)))
    } else {
        b.border_style(t.s(t.artifact))
            .title(Span::styled(format!(" {title} "), t.s(t.artifact)))
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if app.mode == TuiMode::Agent {
        draw_agent(f, app);
        if app.help {
            draw_help(f, area, app);
        }
        return;
    }
    if app.mode == TuiMode::Transcript {
        draw_transcript(f, app);
        return;
    }
    match layout_mode(area.width, area.height) {
        LayoutMode::TooSmall => draw_too_small(f, area),
        LayoutMode::Reduced => draw_reduced(f, area, app),
        LayoutMode::Compact => draw_compact(f, area, app),
        LayoutMode::Medium => draw_medium(f, area, app),
        LayoutMode::Full => draw_full(f, area, app),
    }
    if app.help {
        draw_help(f, area, app);
    }
}

/// Transcript Mode: revisión y búsqueda de la conversación a pantalla casi
/// completa. No administra runs ni contexto (eso es el Inspector).
fn draw_transcript(f: &mut Frame, app: &App) {
    let area = f.area();
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    let t = Theme::for_app(app);
    let head = format!(
        "HLI · transcript · {} líneas · Ctrl+O/Esc/q volver",
        app.transcript_lines(app.transcript_width).len()
    );
    f.render_widget(
        Paragraph::new(head).block(Block::default().borders(Borders::TOP).title(" transcript ")),
        parts[0],
    );

    let height = parts[1].height.saturating_sub(2) as usize;
    let lines = app.transcript_lines(app.transcript_width);
    let start = app
        .transcript
        .scroll
        .min(lines.len().saturating_sub(height));
    let query = app.transcript.query.to_lowercase();
    let active = app
        .transcript
        .matches
        .get(app.transcript.match_index)
        .copied();
    let mut body: Vec<Line> = Vec::new();
    for (i, (_, text, _)) in lines.iter().enumerate().skip(start).take(height) {
        let hit = !query.is_empty() && text.to_lowercase().contains(&query);
        let is_active = active == Some(i);
        let style = if is_active {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else if hit {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let prefix = if hit { "» " } else { "  " };
        body.push(Line::from(Span::styled(format!("{prefix}{text}"), style)));
    }
    if body.is_empty() {
        body.push(Line::from("(transcript vacío)"));
    }
    f.render_widget(
        Paragraph::new(body)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" conversation "),
            )
            .wrap(Wrap { trim: false }),
        parts[1],
    );

    let footer = if app.transcript.search_open {
        format!(
            "Buscar: {}_   [Enter] aceptar   [Esc] cancelar",
            app.transcript.query
        )
    } else if !app.transcript.matches.is_empty() {
        format!(
            " {} coincidencias · n/N · j/k · PgUp/PgDn · g/G · {{ }} prompt · / buscar · q volver",
            app.transcript.matches.len()
        )
    } else {
        " j/k · PgUp/PgDn · g/G · { } prompt · / buscar · q volver".to_string()
    };
    let style = if t.mono {
        Style::default()
    } else {
        Style::default().fg(t.text)
    };
    f.render_widget(Paragraph::new(Span::styled(footer, style)), parts[2]);
}

fn draw_too_small(f: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("Terminal too small"),
        Line::from("Required: 40x10"),
        Line::from(format!("Current: {}x{}", area.width, area.height)),
        Line::from("q quit"),
    ];
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn chrome(f: &mut Frame, area: Rect, app: &App, hint: bool) -> Rect {
    let mut c = vec![
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
    ];
    if hint {
        c.push(Constraint::Length(1));
    }
    c.push(Constraint::Length(4));
    let ch = Layout::default()
        .direction(Direction::Vertical)
        .constraints(c)
        .split(area);
    header(f, ch[0], app);
    let body = ch[1];
    tabs(f, ch[2], app, false);
    if hint {
        hint_bar(f, ch[3], app);
        command(f, ch[4], app);
    } else {
        command(f, ch[3], app);
    }
    body
}

fn draw_full(f: &mut Frame, area: Rect, app: &App) {
    let body = chrome(f, area, app, true);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(body);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(24),
            Constraint::Percentage(30),
            Constraint::Percentage(46),
        ])
        .split(rows[0]);
    navigation(f, cols[0], app);
    collection(f, cols[1], app);
    inspector(f, cols[2], app);
    detail(f, rows[1], app);
}

fn draw_medium(f: &mut Frame, area: Rect, app: &App) {
    let body = chrome(f, area, app, true);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    navigation(f, cols[0], app);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(cols[1]);
    inspector(f, right[0], app);
    detail(f, right[1], app);
}

fn draw_compact(f: &mut Frame, area: Rect, app: &App) {
    let body = chrome(f, area, app, true);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .split(body);
    let label = format!(" {}/5 {} ", focus_index(app.focus) + 1, app.focus.label());
    f.render_widget(Paragraph::new(label), rows[0]);
    match app.focus {
        FocusArea::Navigation => navigation(f, rows[1], app),
        FocusArea::Collection => collection(f, rows[1], app),
        FocusArea::Inspector | FocusArea::DetailTabs => inspector(f, rows[1], app),
        FocusArea::CommandBar => detail(f, rows[1], app),
    }
}

fn draw_reduced(f: &mut Frame, area: Rect, app: &App) {
    let body = chrome(f, area, app, false);
    inspector(f, body, app);
}

fn focus_index(f: FocusArea) -> usize {
    match f {
        FocusArea::Navigation => 0,
        FocusArea::Collection => 1,
        FocusArea::Inspector => 2,
        FocusArea::DetailTabs => 3,
        FocusArea::CommandBar => 4,
    }
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let ws = app
        .workspaces
        .first()
        .and_then(|w| w["name"].as_str())
        .unwrap_or("none");
    let th = app
        .threads
        .first()
        .and_then(|w| w["title"].as_str())
        .unwrap_or("none");
    let br = app
        .branches
        .first()
        .and_then(|b| b["name"].as_str())
        .unwrap_or("none");
    let provider = app
        .selected_run
        .as_ref()
        .and_then(|r| r["receipt"]["provider"].as_str())
        .unwrap_or("none");
    let model = app
        .selected_run
        .as_ref()
        .and_then(|r| r["receipt"]["model"].as_str())
        .unwrap_or("none");
    let line = format!(
        "HLI │ workspace: {ws} │ thread: {th} │ branch: {br} │ provider: {provider}/{model} │ daemon: {}",
        if app.daemon { "up" } else { "down" }
    );
    let max = area.width.saturating_sub(2) as usize;
    let shown = if line.chars().count() > max {
        format!(
            "{}…",
            line.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    } else {
        line
    };
    f.render_widget(
        Paragraph::new(shown).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn tabs(f: &mut Frame, area: Rect, app: &App, compact: bool) {
    let text = if compact {
        format!(" {}/6 {} ", app.tab + 1, TABS[app.tab])
    } else {
        TABS.iter()
            .enumerate()
            .map(|(i, t)| {
                if i == app.tab {
                    format!("[{t}]")
                } else {
                    t.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    f.render_widget(Paragraph::new(format!(" {text}")), area);
}

fn hint_bar(f: &mut Frame, area: Rect, app: &App) {
    let text = match app.focus {
        FocusArea::Navigation => "↑↓ select · → expand · Enter open · Tab panel · ? help",
        FocusArea::Collection => "↑↓ select · Enter open · Tab panel · : command",
        FocusArea::Inspector => "↑↓ scroll · ←→ tabs · ? help",
        FocusArea::DetailTabs => "←→ tabs · 1..6 direct · ? help",
        FocusArea::CommandBar => "Enter run · Esc cancel · ↑↓ history",
    };
    f.render_widget(Paragraph::new(format!(" {text}")), area);
}

fn command(f: &mut Frame, area: Rect, app: &App) {
    let theme = Theme::for_app(app);
    let mut lines = vec![Line::from(vec![
        Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(app.input.clone()),
    ])];
    let line = if let Some(fb) = app.visible_feedback() {
        let (prefix, color) = match fb.kind {
            FeedbackKind::Success => (app.symbol("✓ ", "OK "), theme.success),
            FeedbackKind::Error => (app.symbol("× ", "X "), theme.error),
            FeedbackKind::Info => ("i ".to_string(), theme.artifact),
        };
        let style = if theme.mono {
            Style::default()
        } else {
            Style::default().fg(color)
        };
        Line::from(Span::styled(format!(" {prefix}{}", fb.message), style))
    } else if app.input.is_empty() {
        Line::from(" suggestion: :workspace create demo")
    } else {
        Line::from("")
    };
    lines.push(line);
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn navigation(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if app.workspaces.is_empty() {
        lines.push(Line::from(Span::styled(
            "No workspaces",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("Create one to start"));
        lines.push(Line::from(":workspace create <name>"));
    } else {
        for (i, w) in app.workspaces.iter().enumerate() {
            let mark = if i == app.nav_index { "▸" } else { " " };
            lines.push(Line::from(format!(
                "{mark} {}  {}",
                w["id"].as_str().unwrap_or("-"),
                w["name"].as_str().unwrap_or("")
            )));
        }
        lines.push(Line::from(""));
        for th in &app.threads {
            lines.push(Line::from(format!(
                "  thread {}",
                th["title"].as_str().unwrap_or("-")
            )));
        }
        for (i, b) in app.branches.iter().enumerate() {
            let prefix = if i == 0 {
                app.symbol("●", "*")
            } else if i + 1 == app.branches.len() {
                app.symbol("└─●", "`-*")
            } else {
                app.symbol("├─●", "|-*")
            };
            lines.push(Line::from(format!(
                "  {prefix} {}  seq {}",
                b["name"].as_str().unwrap_or("-"),
                b["head_sequence"].as_i64().unwrap_or(0)
            )));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block(
                "Navigation",
                app.focus == FocusArea::Navigation,
                &Theme::for_app(app),
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn collection(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if app.workspaces.is_empty() {
        lines.push(Line::from(Span::styled(
            "Select or create a workspace first",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(":workspace create <name>"));
    } else if app.threads.is_empty() {
        lines.push(Line::from(Span::styled(
            "No threads in this workspace",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(":thread create <title>"));
    } else if app.branches.is_empty() {
        lines.push(Line::from(Span::styled(
            "No branches in this thread",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(":branch create <name>"));
    } else if app.runs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No runs on this branch",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(":run start \"<task>\""));
    } else {
        for (i, r) in app.runs.iter().enumerate() {
            let mark = if i == app.collection_index {
                "▸"
            } else {
                " "
            };
            lines.push(Line::from(format!(
                "{mark} {}  {}",
                r["id"].as_str().unwrap_or("-"),
                r["state"].as_str().unwrap_or("-")
            )));
        }
        lines.push(Line::from(format!(
            " {}/{}",
            app.collection_index + 1,
            app.runs.len()
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(block(
            "Runs",
            app.focus == FocusArea::Collection,
            &Theme::for_app(app),
        )),
        area,
    );
}

fn inspector(f: &mut Frame, area: Rect, app: &App) {
    let t = Theme::for_app(app);
    let title = match &app.selected_run {
        Some(r) => format!("Run {}", r["id"].as_str().unwrap_or("-")),
        None => "No selection".to_string(),
    };
    let mut lines = Vec::new();
    if let Some(r) = &app.selected_run {
        lines.push(Line::from(format!(
            "estado   {}",
            r["state"].as_str().unwrap_or("-")
        )));
        lines.push(Line::from(format!(
            "provider {}/{}",
            r["receipt"]["provider"].as_str().unwrap_or("-"),
            r["receipt"]["model"].as_str().unwrap_or("-")
        )));
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
            r["receipt"]["usage"]["latency_total_ms"]
                .as_i64()
                .unwrap_or(0)
        )));
    } else {
        lines.push(Line::from("No selection"));
    }
    f.render_widget(
        Paragraph::new(lines).block(block(&title, app.focus == FocusArea::Inspector, &t)),
        area,
    );
}

fn detail(f: &mut Frame, area: Rect, app: &App) {
    let t = Theme::for_app(app);
    let lines = match app.tab {
        1 => event_lines(app),
        2 => context_lines(app),
        3 => memory_lines(app),
        4 => artifact_lines(app),
        5 => mch_lines(app),
        _ => run_lines(app),
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(block(TABS[app.tab], app.focus == FocusArea::DetailTabs, &t))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn run_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = vec![Line::from(format!(
        "payload_hash {}",
        app.manifest
            .as_ref()
            .and_then(|m| m["payload_hash"].as_str())
            .unwrap_or("-")
    ))];
    if let Some(m) = &app.manifest {
        v.push(Line::from(format!(
            "presupuesto {} · estimado {}",
            m["budget_tokens"], m["estimated_tokens"]
        )));
    }
    if let Some(p) = &app.inspector_plan {
        let dash = |s: &Option<String>| s.clone().unwrap_or_else(|| "-".into());
        v.push(Line::from(""));
        v.push(Line::from("Plan"));
        v.push(Line::from(format!("Plan ID                {}", p.plan_id)));
        v.push(Line::from(format!("Revision               {}", p.revision)));
        v.push(Line::from(format!(
            "Supersedes Plan        {}",
            dash(&p.supersedes)
        )));
        v.push(Line::from(format!("Run ID                 {}", p.run_id)));
        v.push(Line::from(format!("Origin                 {}", p.origin)));
        v.push(Line::from(format!("Status                 {}", p.status)));
        v.push(Line::from(format!("Summary                {}", p.summary)));
        for s in &p.steps {
            v.push(Line::from(format!(
                "  {}. {:<38} {}{}",
                s.order,
                s.description,
                s.effect,
                if s.requires_approval {
                    " · approval required"
                } else {
                    ""
                }
            )));
        }
        v.push(Line::from(format!(
            "Requested effects      {}",
            if p.effects.is_empty() {
                "-".into()
            } else {
                p.effects.join(", ")
            }
        )));
        v.push(Line::from(format!(
            "Approval scope         {}",
            dash(&p.approval_scope)
        )));
        v.push(Line::from(format!(
            "Approved by            {}",
            dash(&p.approved_by)
        )));
        v.push(Line::from(format!(
            "Created at             {}",
            p.created_at
        )));
        v.push(Line::from(format!(
            "Approved or rejected   {}",
            dash(&p.decided_at)
        )));
        v.push(Line::from(format!(
            "Plan content hash      {}",
            p.plan_hash
        )));
        v.push(Line::from(format!(
            "Execution snapshot     {}",
            p.execution_hash
        )));
        v.push(Line::from(format!(
            "Invalidation reasons   {}",
            if p.invalidation_reasons.is_empty() {
                "-".to_string()
            } else {
                p.invalidation_reasons.join("; ")
            }
        )));
        v.push(Line::from(format!(
            "Provider/model         {} / {}",
            p.provider, p.model
        )));
        v.push(Line::from(format!(
            "ContextManifest        {}",
            p.manifest_id
        )));
        v.push(Line::from(format!(
            "Branch revision        {}",
            p.branch_revision
        )));
        v.push(Line::from(""));
        v.push(Line::from(
            "[y] approve once  [a] run  [w] workspace  [n] reject  [r] replan",
        ));
    }
    if let Some(p) = &app.inspector_perm {
        v.push(Line::from(""));
        v.push(Line::from("Tool permission"));
        v.push(Line::from(format!(
            "Request ID             {}",
            p.request_id
        )));
        v.push(Line::from(format!("Run ID                 {}", p.run_id)));
        v.push(Line::from(format!(
            "Plan y revisión        {} · {}",
            p.plan_id, p.plan_revision
        )));
        v.push(Line::from(format!("Tool                   {}", p.tool)));
        v.push(Line::from(format!(
            "Operation              {}",
            p.operation
        )));
        v.push(Line::from(format!("Effect                 {}", p.effect)));
        v.push(Line::from(format!(
            "Target                 {}",
            p.target.clone().unwrap_or_else(|| "-".into())
        )));
        v.push(Line::from(format!(
            "Arguments hash         {}",
            p.arguments_hash
        )));
        v.push(Line::from(format!(
            "Arguments summary      {}",
            p.arguments_summary
        )));
        v.push(Line::from(format!("Status                 {}", p.status)));
        v.push(Line::from(format!(
            "Effective policy       {}",
            if p.effective_policy.is_empty() {
                "ask (sin grant aplicable)"
            } else {
                &p.effective_policy
            }
        )));
        v.push(Line::from(format!(
            "Decision               {}",
            p.decision.clone().unwrap_or_else(|| "-".into())
        )));
        v.push(Line::from(format!(
            "Decision scope         {}",
            p.decision_scope.clone().unwrap_or_else(|| "-".into())
        )));
        v.push(Line::from(format!(
            "Actor                  {}",
            p.actor.clone().unwrap_or_else(|| "-".into())
        )));
        v.push(Line::from(format!(
            "Created at             {}",
            p.created_at
        )));
        v.push(Line::from(format!(
            "Decided at             {}",
            p.decided_at.clone().unwrap_or_else(|| "-".into())
        )));
        if p.pending() {
            v.push(Line::from(""));
            v.push(Line::from(
                "[y] allow once  [a] run  [w] workspace  [n] deny  [v] details",
            ));
        }
    }
    v
}

fn event_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    let needle = app.filter.as_deref().unwrap_or("").to_lowercase();
    for e in app
        .events
        .iter()
        .rev()
        .filter(|e| {
            needle.is_empty()
                || e["type"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
        })
        .skip(app.scroll)
        .take(20)
    {
        v.push(Line::from(format!(
            "{:>4}  {:<30} {}",
            e["sequence"].as_i64().unwrap_or(0),
            e["type"].as_str().unwrap_or("-"),
            e["created_at"].as_str().unwrap_or("-")
        )));
    }
    v.push(Line::from(format!(
        " {}/{}",
        app.scroll + 1,
        app.events.len().max(1)
    )));
    v
}

fn context_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    if let Some(m) = &app.manifest {
        v.push(Line::from(format!(
            "user_input {}",
            m["user_input"]["content"].as_str().unwrap_or("-")
        )));
        v.push(Line::from(format!(
            "payload_hash {}",
            m["payload_hash"].as_str().unwrap_or("-")
        )));
        let mems = m["memories"].as_array().map(|a| a.len()).unwrap_or(0);
        let slices = m["artifact_slices"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let excl = m["excluded"].as_array().map(|a| a.len()).unwrap_or(0);
        v.push(Line::from(format!(
            "memorias {mems} · slices {slices} · excluidos {excl}"
        )));
        for s in m["artifact_slices"].as_array().cloned().unwrap_or_default() {
            v.push(Line::from(format!(
                "slice {} · {}",
                s["slice_id"].as_str().unwrap_or("-"),
                s["reason"].as_str().unwrap_or("")
            )));
        }
    }
    v
}

fn memory_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    for status in ["active", "superseded", "revoked", "proposed"] {
        let sel: Vec<&Value> = app
            .memories
            .iter()
            .filter(|m| m["status"].as_str() == Some(status))
            .collect();
        if sel.is_empty() {
            continue;
        }
        v.push(Line::from(format!("{status} ({})", sel.len())));
        for m in sel {
            let extra = if status == "superseded" {
                format!(
                    " · superseded_at {}",
                    m["superseded_at"].as_str().unwrap_or("-")
                )
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

fn artifact_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    for a in &app.artifacts {
        v.push(Line::from(format!(
            "{}  {}  {} bytes  {}",
            a["id"].as_str().unwrap_or("-"),
            a["mime"].as_str().unwrap_or("-"),
            a["bytes"].as_i64().unwrap_or(0),
            a["sha256"].as_str().unwrap_or("-")
        )));
    }
    v
}

fn mch_lines(app: &App) -> Vec<Line<'static>> {
    let mut v = Vec::new();
    for m in &app.mchs {
        v.push(Line::from(format!(
            "Profile: {}",
            m["profile"].as_str().unwrap_or("-")
        )));
        v.push(Line::from(format!(
            "  módulo {}",
            m["module"].as_str().unwrap_or("-")
        )));
        v.push(Line::from(format!(
            "Instance: {} [{}]",
            m["name"].as_str().unwrap_or("-"),
            m["state"].as_str().unwrap_or("-")
        )));
        v.push(Line::from(format!(
            "  workspace {}",
            m["workspace_id"].as_str().unwrap_or("-")
        )));
        v.push(Line::from("Process: supervisado por HLI"));
        v.push(Line::from(format!(
            "Endpoint: CLI hli mch invoke {} · MCP mch_invoke",
            m["name"].as_str().unwrap_or("-")
        )));
    }
    v
}

// ---------------------------------------------------------------------------
// Agent Mode
// ---------------------------------------------------------------------------

/// Lista navegable de workspaces o sesiones. Mismo rectángulo que el
/// transcript: la pestaña cambia el contenido, no el marco.
fn draw_agent_list(f: &mut Frame, area: Rect, app: &App, t: &Theme) {
    use crate::app::AgentTab;
    let (title, items, sel): (&str, Vec<(String, String)>, usize) = match app.agent.agent_tab {
        AgentTab::Workspace => (
            " workspace ",
            app.workspaces
                .iter()
                .map(|w| {
                    (
                        w["name"].as_str().unwrap_or("-").to_string(),
                        w["root_path"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect(),
            app.agent.ws_sel,
        ),
        AgentTab::Session => (
            " sesiones ",
            app.threads
                .iter()
                .map(|th| {
                    (
                        th["title"].as_str().unwrap_or("sesión").to_string(),
                        th["id"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect(),
            app.agent.thread_sel,
        ),
        AgentTab::Chat => (" ", Vec::new(), 0),
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        match app.agent.agent_tab {
            AgentTab::Workspace => "elige dónde trabajar",
            _ => "elige una sesión o crea otra",
        },
        Style::default().fg(t.artifact),
    )));
    lines.push(Line::from(""));
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (ninguno todavía)",
            Style::default().fg(t.artifact),
        )));
    }
    for (i, (name, meta)) in items.iter().enumerate() {
        let active = i == sel;
        let mut row = vec![
            Span::styled(
                if active { " ▶ " } else { "   " },
                Style::default().fg(t.primary),
            ),
            Span::styled(
                format!("{name:<26}"),
                if active {
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.memory)
                },
            ),
            Span::styled(meta.clone(), Style::default().fg(t.artifact)),
        ];
        if active {
            row.push(Span::styled("  ↵", Style::default().fg(t.success)));
        }
        lines.push(Line::from(row));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::TOP).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_agent(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width < 40 || area.height < 10 {
        draw_too_small(f, area);
        return;
    }
    // Una sola fuente de verdad para las alturas: el mismo cálculo lo usa
    // `App::observe_terminal` para el viewport y el hit map.
    let regions = crate::layout::agent_regions(area, app.agent.offer != Offer::None);
    let parts = [regions.header, regions.body, regions.composer];

    let t = Theme::for_app(app);
    let ws = app
        .agent
        .workspace
        .as_ref()
        .and_then(|w| w["name"].as_str())
        .unwrap_or("no-workspace");
    let th = app
        .agent
        .thread
        .as_ref()
        .and_then(|w| w["title"].as_str())
        .unwrap_or("no-thread");
    let br = app
        .agent
        .branch
        .as_ref()
        .and_then(|w| w["name"].as_str())
        .unwrap_or("no-branch");
    let spinner = app.spinner();
    let kind = match app.agent.approval_kind.as_deref() {
        Some("plan") => " · plan",
        Some("tool") => " · tool",
        _ => "",
    };
    let sp = if spinner.is_empty() {
        String::new()
    } else {
        format!("{spinner} ")
    };
    // Pestañas: las tres primeras decisiones, siempre a la vista.
    let tab_style = |active: bool| {
        if active {
            Style::default()
                .fg(t.primary)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(t.artifact)
        }
    };
    let mut tabs: Vec<Span> = Vec::new();
    for (n, label, tab) in [
        (1, "Workspace", crate::app::AgentTab::Workspace),
        (2, "Sesión", crate::app::AgentTab::Session),
        (3, "Chat", crate::app::AgentTab::Chat),
    ] {
        tabs.push(Span::styled(
            format!(" {n} {label} "),
            tab_style(app.agent.agent_tab == tab),
        ));
        tabs.push(Span::styled("│", Style::default().fg(t.artifact)));
    }
    let right = format!(
        " {ws}/{th}/{br} · {p}/{m} · {sp}{state}{kind} ",
        p = app.agent.provider,
        m = app.agent.model,
        state = app.agent.run_state.replace('_', " "),
    );
    tabs.push(Span::styled(right, Style::default().fg(t.artifact)));
    f.render_widget(
        Paragraph::new(Line::from(tabs))
            .block(Block::default().borders(Borders::ALL).title(" HLI ")),
        parts[0],
    );

    // Workspace y Sesión sustituyen el cuerpo; el compositor solo vive en Chat.
    if app.agent.agent_tab != crate::app::AgentTab::Chat {
        draw_agent_list(f, parts[1], app, &t);
        let hint = "↑↓ mover · ↵ entrar · n nuevo · Tab cambiar · Esc volver a Chat";
        f.render_widget(
            Paragraph::new(Line::from(hint))
                .block(Block::default().borders(Borders::TOP).title(" atajos ")),
            parts[2],
        );
        return;
    }

    let inner_h = parts[1].height.saturating_sub(1) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for l in &app.agent.lines {
        let (tag, color) = match l.kind {
            AgentKind::User => ("You", t.primary),
            AgentKind::Hli => ("HLI", t.memory),
            AgentKind::Tool => ("·", t.success),
            AgentKind::Error => ("×", t.error),
            AgentKind::Info => ("·", t.artifact),
        };
        let style = if t.mono {
            Style::default()
        } else {
            Style::default().fg(color)
        };
        // El tag comparte línea con la primera línea del mensaje; las
        // siguientes van indentadas. Así el transcript se lee como una
        // conversación y no como dos columnas de etiquetas.
        let mut parts = l.text.split('\n');
        let first = parts.next().unwrap_or("");
        lines.push(Line::from(vec![
            Span::styled(format!("{tag:<3} "), style.add_modifier(Modifier::BOLD)),
            Span::raw(first.to_string()),
        ]));
        for part in parts {
            lines.push(Line::from(format!("    {part}")));
        }
        if l.expanded
            && let Some(d) = &l.detail
        {
            lines.push(Line::from(format!("    type:      {}", d.kind)));
            lines.push(Line::from(format!("    status:    {}", d.status)));
            lines.push(Line::from(format!("    started:   {}", d.started)));
            lines.push(Line::from(format!(
                "    duration:  {}",
                d.duration
                    .map(|v| format!("{v:.1} s"))
                    .unwrap_or_else(|| "-".into())
            )));
            lines.push(Line::from(format!("    input:     {}", d.input)));
            lines.push(Line::from(format!("    output:    {}", d.output)));
            lines.push(Line::from(format!(
                "    causation: {} · correlation: {}",
                d.causation.as_deref().unwrap_or("-"),
                d.correlation.as_deref().unwrap_or("-")
            )));
            if !d.artifacts.is_empty() {
                lines.push(Line::from(format!(
                    "    artifacts: {}",
                    d.artifacts.join(", ")
                )));
            }
            if let Some(err) = &d.error {
                lines.push(Line::from(format!("    error:     {err}")));
            }
        }
        // Separación sólo entre turnos; los eventos de log van seguidos como
        // en Claude Code.
        if matches!(l.kind, AgentKind::User | AgentKind::Hli) {
            lines.push(Line::from(""));
        }
    }
    let total = lines.len();
    lines = match app.agent.scroll_top {
        Some(top) => {
            let top = top.min(total.saturating_sub(1));
            let end = (top + inner_h).min(total);
            lines[top..end].to_vec()
        }
        None => {
            if total > inner_h {
                lines.split_off(total - inner_h)
            } else {
                lines
            }
        }
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::TOP).title(" transcript "))
            .wrap(Wrap { trim: false }),
        parts[1],
    );
    // Scrollbar sólo cuando hay contenido fuera del viewport; su posición y
    // proporción salen del modelo, no de un valor decorativo.
    let vp = &app.agent.viewport;
    if vp.total_rendered_lines > vp.viewport_height {
        let mut state = ScrollbarState::new(vp.max_scroll()).position(vp.first_visible_line);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            parts[1],
            &mut state,
        );
    }

    match app.agent.offer.clone() {
        Offer::None => {
            let block = Block::default().borders(Borders::TOP).title(" compose ");
            let mut text: Vec<Line> = Vec::new();
            if app.agent.composer.is_empty() {
                text.push(Line::from(vec![
                    Span::styled("❯ ", Style::default().fg(t.primary)),
                    Span::styled("escribe una tarea…", Style::default().fg(t.artifact)),
                ]));
            } else {
                for (i, l) in app.agent.composer.split('\n').enumerate() {
                    let prefix = if i == 0 { "❯ " } else { "  " };
                    text.push(Line::from(format!("{prefix}{l}")));
                }
            }
            text.push(Line::from(Span::styled(
                "  ⏎ enviar · ⇧⏎ línea · / comandos · @ archivo",
                Style::default().fg(t.artifact),
            )));
            f.render_widget(Paragraph::new(text).block(block), parts[2]);
        }
        Offer::CreateWorkspace { name } => {
            let text = vec![
                Line::from("No HLI workspace is linked to this directory."),
                Line::from(format!("[Enter] Create workspace \"{name}\"")),
                Line::from("[Esc]   Cancel"),
            ];
            f.render_widget(
                Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" link ")),
                parts[2],
            );
        }
        Offer::CreateBranch { name } => {
            let text = vec![
                Line::from(format!("Create branch \"{name}\" from current revision?")),
                Line::from("[Enter] Create   [Esc] Cancel"),
            ];
            f.render_widget(
                Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title(" branch ")),
                parts[2],
            );
        }
    }

    // Línea única, sin placeholders: solo si hay actividad nueva o un aviso.
    let unseen = app
        .agent
        .viewport
        .unseen_activity_count
        .max(app.agent.new_activity);
    let status = if unseen > 0 {
        if area.width >= 60 {
            format!("↓ {unseen} new activity · Ctrl+End para volver al final")
        } else {
            format!("↓ {unseen} new")
        }
    } else {
        app.agent.status.clone()
    };
    if !status.trim().is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {status}"),
                Style::default().fg(t.primary),
            ))),
            regions.status,
        );
    }

    if app.agent.palette {
        draw_palette(f, area, app);
    }
    if app.agent.history_open {
        draw_history(f, area, app);
    }
    if app.agent.resume_open {
        draw_resume(f, area, app);
    }
    if app.agent.ref_open {
        draw_reference(f, area, app);
    }
    if let Some(pf) = &app.agent.preflight {
        draw_preflight(f, area, pf);
    }
    if app.agent.reject_open {
        draw_reject_reason(f, area, app);
    } else if let Some(perm) = &app.agent.perm
        && perm.pending()
    {
        draw_permission(f, area, perm);
    } else if let Some(plan) = &app.agent.plan
        && plan.waiting()
    {
        draw_plan(f, area, plan);
    }
    if app.agent.model_open {
        draw_model(f, area, app);
    }
}

/// `/model`: proveedores del daemon. Elegir aquí cambia el modelo de la sesión
/// y lo que se envía en `run.start`.
fn draw_model(f: &mut Frame, area: Rect, app: &App) {
    let t = Theme::for_app(app);
    let height = (app.agent.providers.len() as u16 + 8).min(area.height.saturating_sub(2));
    let rect = overlay_rect(area, 72.min(area.width), height);
    f.render_widget(Clear, rect);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "modelo para esta sesión",
        Style::default().fg(t.artifact),
    )));
    lines.push(Line::from(""));
    if app.agent.providers.is_empty() {
        lines.push(Line::from("  (ningún proveedor configurado)"));
    }
    for (i, p) in app.agent.providers.iter().enumerate() {
        let name = p["name"].as_str().unwrap_or("-");
        let model = p["model"].as_str().unwrap_or("-");
        let kind = p["kind"].as_str().unwrap_or("-");
        let active = i == app.agent.model_sel;
        let current = name == app.agent.provider;
        lines.push(Line::from(vec![
            Span::styled(
                if active { " ▶ " } else { "   " },
                Style::default().fg(t.primary),
            ),
            Span::styled(
                format!("{name:<14}"),
                if active {
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.memory)
                },
            ),
            Span::styled(format!("{kind:<10}"), Style::default().fg(t.artifact)),
            Span::styled(model.to_string(), Style::default().fg(t.memory)),
            Span::styled(
                if current { "  actual" } else { "" },
                Style::default().fg(t.success),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ elegir · ↵ aplicar · esc cerrar",
        Style::default().fg(t.artifact),
    )));
    lines.push(Line::from(Span::styled(
        "más proveedores: hli provider set",
        Style::default().fg(t.artifact),
    )));
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" model "))
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn draw_permission(f: &mut Frame, area: Rect, perm: &crate::app::PermissionView) {
    let height = (perm.overlay_lines().len() as u16 + 2).min(area.height.saturating_sub(2));
    // Ancho completo con margen: el modal no deja texto del transcript a los
    // lados, que se lee como suciedad.
    let rect = overlay_rect(area, area.width, height);
    f.render_widget(Clear, rect);
    let lines: Vec<Line> = perm.overlay_lines().into_iter().map(Line::from).collect();
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" tool permission "),
            )
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn draw_plan(f: &mut Frame, area: Rect, plan: &crate::app::PlanView) {
    let height = (plan.overlay_lines().len() as u16 + 2).min(area.height.saturating_sub(2));
    let rect = overlay_rect(area, area.width, height);
    f.render_widget(Clear, rect);
    let lines: Vec<Line> = plan.overlay_lines().into_iter().map(Line::from).collect();
    let title = if plan.invalidated() {
        " plan invalidated "
    } else {
        " plan approval "
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn draw_reject_reason(f: &mut Frame, area: Rect, app: &App) {
    let rect = overlay_rect(area, 70, 6);
    f.render_widget(Clear, rect);
    let text = vec![
        Line::from("Reason for rejection (opcional, Enter para confirmar)"),
        Line::from(format!("> {}", app.agent.reject_reason)),
        Line::from("[Enter] Reject   [Esc] Cancel"),
    ];
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" reject ")),
        rect,
    );
}

fn draw_preflight(f: &mut Frame, area: Rect, pf: &crate::app::Preflight) {
    let rect = overlay_rect(area, 90, 16);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from(format!(
        "{} reference(s) require attention",
        pf.issues.len()
    ))];
    for issue in &pf.issues {
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "{} · {}",
            issue.stable_id, issue.reason
        )));
        if let Some(sel) = &issue.selected_hash {
            lines.push(Line::from(format!("  Selected: {sel}")));
        }
        if let Some(cur) = &issue.current_hash {
            lines.push(Line::from(format!("  Current:  {cur}")));
        }
    }
    lines.push(Line::from(""));
    let any_refresh = pf.issues.iter().any(|i| i.refreshable);
    lines.push(Line::from(if any_refresh {
        "[r] Refresh   [c] Cancel"
    } else {
        "[c] Cancel"
    }));
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" references ")),
        rect,
    );
}

fn draw_reference(f: &mut Frame, area: Rect, app: &App) {
    let rect = overlay_rect(area, 100, 18);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from(format!(
        "categoría {} · búsqueda: {}",
        app.ref_category_kind().title(),
        app.agent.ref_query
    ))];
    let items = app.ref_filtered();
    for (i, item) in items.iter().take(12).enumerate() {
        let mark = if i == app.agent.ref_index {
            "▸ "
        } else {
            "  "
        };
        lines.push(Line::from(format!(
            "{mark}{}  ·  {}",
            item.display, item.subtitle
        )));
    }
    if items.is_empty() {
        lines.push(Line::from("  (sin coincidencias)"));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" @ reference · Tab cambia categoría "),
        ),
        rect,
    );
}

fn overlay_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = area.width.min(w);
    let h = area.height.min(h);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn draw_history(f: &mut Frame, area: Rect, app: &App) {
    let rect = overlay_rect(area, 70, 14);
    f.render_widget(Clear, rect);
    let matches = app.history_matches();
    let mut lines = vec![Line::from(format!("> {}", app.agent.history_query))];
    for (i, m) in matches.iter().take(9).enumerate() {
        let mark = if i == app.agent.history_sel {
            "▸ "
        } else {
            "  "
        };
        lines.push(Line::from(format!("{mark}{m}")));
    }
    if matches.is_empty() {
        lines.push(Line::from("  (sin coincidencias)"));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" history · ^R "),
        ),
        rect,
    );
}

fn draw_resume(f: &mut Frame, area: Rect, app: &App) {
    let rect = overlay_rect(area, 96, 18);
    f.render_widget(Clear, rect);
    let mut lines = Vec::new();
    for (i, item) in app.agent.resume_items.iter().take(14).enumerate() {
        let mark = if i == app.agent.resume_index {
            "▸ "
        } else {
            "  "
        };
        let when: String = item.last_activity.chars().take(16).collect();
        lines.push(Line::from(format!(
            "{mark}{} · {}/{} · {} · {} · {}",
            item.title, item.workspace_name, item.branch, item.state, item.model, when
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from("  (sin threads)"));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" resume ")),
        rect,
    );
}

fn draw_palette(f: &mut Frame, area: Rect, app: &App) {
    let t = Theme::for_app(app);
    let items = app.palette_items();
    let w = 66.min(area.width);
    let max_h = area.height.saturating_sub(4).max(3);
    let h = ((items.len() as u16) + 3).min(max_h);
    let visible = h.saturating_sub(3) as usize;
    let sel = app.agent.palette_index.min(items.len().saturating_sub(1));
    let offset = (sel + 1).saturating_sub(visible);
    // Anclado justo encima del compositor, como el menú de Claude Code: no
    // cruza reglas ni tapa la entrada.
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + area.height.saturating_sub(5 + h),
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);

    let mut lines: Vec<Line> = Vec::new();
    for (i, (name, desc)) in items.iter().enumerate().skip(offset).take(visible) {
        let active = i == sel;
        lines.push(Line::from(vec![
            Span::styled(
                if active { " ▸ " } else { "   " },
                Style::default().fg(t.primary),
            ),
            Span::styled(
                format!("/{name:<12}"),
                if active {
                    Style::default().fg(t.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.memory)
                },
            ),
            Span::styled(*desc, Style::default().fg(t.artifact)),
        ]));
    }
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No commands match",
            Style::default().fg(t.error),
        )));
    }
    lines.push(Line::from(Span::styled(
        "↑↓ elegir · ↵ ejecutar · esc cerrar",
        Style::default().fg(t.artifact),
    )));
    let title = format!(" / · {} ", items.len());
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        rect,
    );
}

fn draw_help(f: &mut Frame, area: Rect, _app: &App) {
    let w = area.width.min(72);
    let h = area.height.min(17);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let text = vec![
        Line::from("HLI · ayuda"),
        Line::from(""),
        Line::from("Pestañas   1/2/3 · Tab / Shift+Tab · clic en la barra"),
        Line::from("Listas     ↑↓ mover · ↵ entrar · n nuevo · Esc volver al chat"),
        Line::from("Comandos   / abre el menú · ↑↓ elegir · ↵ ejecutar · Esc cerrar"),
        Line::from("Chat       ↵ enviar · ⇧↵ línea · @ archivo · Esc cancela"),
        Line::from("Vista      rueda · PgUp/PgDn · Ctrl+Home/End"),
        Line::from("Paneles    Ctrl+O transcript · Ctrl+I inspector · Ctrl+E detalles"),
        Line::from("Estados    ● activo · ✓ hecho · ◌ pausa · ! aprobación · × fallo"),
        Line::from("Modelo     /model elige proveedor y modelo de la sesión"),
        Line::from(""),
        Line::from("Esc, ? o q cierran esta ayuda"),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" ayuda "))
            .wrap(Wrap { trim: false }),
        rect,
    );
}
