use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
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

fn draw_agent(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width < 40 || area.height < 10 {
        draw_too_small(f, area);
        return;
    }
    let heights = if app.agent.offer != Offer::None { 5 } else { 3 };
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(heights),
            Constraint::Length(1),
        ])
        .split(area);

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
    let head = format!(
        "HLI │ {ws}/{th}/{br} │ {}/{} │ {}",
        app.agent.provider, app.agent.model, app.agent.run_state
    );
    f.render_widget(
        Paragraph::new(head).block(Block::default().borders(Borders::ALL).title(" HLI ")),
        parts[0],
    );

    let inner_h = parts[1].height.saturating_sub(2) as usize;
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
        lines.push(Line::from(Span::styled(
            format!("{tag} "),
            style.add_modifier(Modifier::BOLD),
        )));
        for part in l.text.split('\n') {
            lines.push(Line::from(format!("  {part}")));
        }
        lines.push(Line::from(""));
    }
    if lines.len() > inner_h {
        lines = lines.split_off(lines.len() - inner_h);
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" transcript "))
            .wrap(Wrap { trim: false }),
        parts[1],
    );

    match app.agent.offer.clone() {
        Offer::None => {
            let block = Block::default().borders(Borders::ALL).title(" compose ");
            let mut text: Vec<Line> = Vec::new();
            for (i, l) in app.agent.composer.split('\n').enumerate() {
                let prefix = if i == 0 { "> " } else { "  " };
                text.push(Line::from(format!("{prefix}{l}")));
            }
            if text.is_empty() {
                text.push(Line::from("> "));
            }
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

    f.render_widget(Paragraph::new(format!(" {}", app.agent.status)), parts[3]);

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
    let w = area.width.min(50);
    let h = area.height.min(12);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let items = app.palette_items();
    let mut lines = Vec::new();
    for (i, cmd) in items.iter().enumerate() {
        let mark = if i == 0 { "▸ " } else { "  " };
        lines.push(Line::from(format!("{mark}/{cmd}")));
    }
    if lines.is_empty() {
        lines.push(Line::from("  (sin coincidencias)"));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" commands ")),
        rect,
    );
}

fn draw_help(f: &mut Frame, area: Rect, _app: &App) {
    let w = area.width.min(60);
    let h = area.height.min(18);
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
        Line::from("HLI help"),
        Line::from(""),
        Line::from("Navigation  Tab / Shift+Tab panel · arrows move · Enter open"),
        Line::from("Tabs        ←→ o 1..6 · [activa]"),
        Line::from("Command     : opens · Enter runs · Esc cancels · arrows history"),
        Line::from(
            "States      ● active · ✓ completed · ◌ paused · ! approval · × failed · ↻ running",
        ),
        Line::from("Symbols     * OK RUN X in ASCII mode"),
        Line::from(""),
        Line::from("Esc, ? or q closes this help"),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" ? "))
            .wrap(Wrap { trim: true }),
        rect,
    );
}
