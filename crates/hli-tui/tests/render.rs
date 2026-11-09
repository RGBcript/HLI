use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

use hli_tui::app::{
    ActivityDetail, AgentKind, App, FeedbackKind, FocusArea, LayoutMode, Offer, PermissionView,
    PlanStepView, PlanView, RefItem, RefKind, Reference, TuiMode, available_commands,
    classify_artifact_hash, classify_branch_revision, classify_memory, format_providers,
    layout_mode, project_thread, reference_token, split_command, terminal_summary,
};
use hli_tui::ui;

fn key(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
    KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)
}

fn key_with(code: KeyCode, kind: KeyEventKind, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new_with_kind(code, mods, kind)
}

fn tap(app: &mut App, code: KeyCode) {
    app.handle_key(key(code, KeyEventKind::Press));
    app.handle_key(key(code, KeyEventKind::Release));
}

fn render(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn exactly_one_focus() {
    let app = App::demo();
    assert_eq!(app.focus, FocusArea::Navigation);
    let mut app = app;
    for _ in 0..5 {
        app.cycle_focus();
    }
    assert_eq!(app.focus, FocusArea::Navigation);
}

#[test]
fn tab_cycles_focus() {
    let mut app = App::demo();
    let start = app.focus;
    for _ in 0..5 {
        app.cycle_focus();
    }
    assert_eq!(app.focus, start);
}

#[test]
fn shift_tab_reverses_focus() {
    let mut app = App::demo();
    app.cycle_focus_back();
    assert_eq!(app.focus, FocusArea::CommandBar);
    app.cycle_focus();
    assert_eq!(app.focus, FocusArea::Navigation);
}

#[test]
fn focused_panel_has_noncolor_indicator() {
    let mut app = App::demo();
    app.no_color = true;
    let text = render(&app, 120, 40);
    assert!(text.contains("▸ Navigation"), "falta marcador de foco");
}

#[test]
fn empty_workspace_has_actionable_command() {
    let app = App::empty();
    let text = render(&app, 120, 40);
    assert!(text.contains("No workspaces"));
    assert!(text.contains(":workspace create"));
}

#[test]
fn duplicate_run_titles_are_removed() {
    let app = App::demo();
    let text = render(&app, 120, 40);
    assert!(!text.contains("Run Run"));
    assert!(text.contains("Runs"));
    assert!(text.contains("Run run_demo"));
}

#[test]
fn tabs_are_visible() {
    let app = App::demo();
    let text = render(&app, 120, 40);
    for t in ["[Run]", "Events", "Context", "Memory", "Artifacts", "MCH"] {
        assert!(text.contains(t), "falta {t}");
    }
}

#[test]
fn small_layout_uses_single_panel() {
    let app = App::demo();
    assert_eq!(layout_mode(70, 20), LayoutMode::Compact);
    let text = render(&app, 70, 20);
    assert!(text.contains("1/5 Navigation"));
}

#[test]
fn tiny_terminal_shows_size_warning() {
    let app = App::demo();
    let text = render(&app, 30, 8);
    assert!(text.contains("Terminal too small"));
    assert!(text.contains("40x10"));
    assert!(text.contains("30x8"));
}

#[test]
fn selection_survives_poll_refresh() {
    let mut app = App::demo();
    app.runs = vec![
        serde_json::json!({"id": "run_new", "state": "completed"}),
        serde_json::json!({"id": "run_demo", "state": "completed"}),
    ];
    app.selected_run_id = Some("run_demo".into());
    assert_eq!(app.selected_index(), 1);
}

#[test]
fn list_scrolls_without_panic() {
    let mut app = App::demo();
    app.events = (0..100)
        .map(|i| serde_json::json!({"sequence": i, "type": "event", "actor": {"kind":"agent"}, "created_at": "x"}))
        .collect();
    app.scroll = 50;
    app.tab = 1;
    app.focus = FocusArea::DetailTabs;
    let text = render(&app, 120, 40);
    assert!(text.contains("event"));
}

#[test]
fn help_overlay_opens_and_closes() {
    let mut app = App::demo();
    app.open_help();
    let text = render(&app, 120, 40);
    assert!(text.contains("HLI · ayuda"));
    app.close_help();
    let text = render(&app, 120, 40);
    assert!(!text.contains("HLI · ayuda"));
}

#[test]
fn command_bar_escape_cancels() {
    let mut app = App::demo();
    app.start_command();
    app.input = "workspace list".into();
    app.cancel_command();
    assert_eq!(app.focus, FocusArea::Navigation);
    assert!(app.input.is_empty());
}

#[test]
fn command_history_works() {
    let mut app = App::demo();
    app.history = vec!["workspace list".into(), "run get run_1".into()];
    app.history_up();
    assert_eq!(app.input, "run get run_1");
    app.history_up();
    assert_eq!(app.input, "workspace list");
    app.history_down();
    assert_eq!(app.input, "run get run_1");
}

#[test]
fn ascii_fallback_contains_no_wide_symbols() {
    let mut app = App::demo();
    app.ascii = true;
    let text = render(&app, 120, 40);
    assert!(!text.contains('●'));
    assert!(text.contains('*'));
}

#[test]
fn tui_small_terminal_does_not_panic() {
    let app = App::demo();
    let text = render(&app, 40, 12);
    assert!(!text.is_empty());
}

#[test]
fn tui_no_color_mode() {
    let mut app = App::demo();
    app.no_color = true;
    let text = render(&app, 100, 30);
    assert!(text.contains("HLI"));
}

#[test]
fn tui_renders_run_state() {
    let app = App::demo();
    let text = render(&app, 120, 40);
    assert!(text.contains("completed"));
    assert!(text.contains("run_demo"));
}

#[test]
fn tui_renders_superseded_memory() {
    let mut app = App::demo();
    app.tab = 3;
    app.focus = FocusArea::DetailTabs;
    let text = render(&app, 120, 40);
    assert!(text.contains("superseded"));
    assert!(text.contains("legacy"));
}

#[test]
fn tui_distinguishes_mch_entities() {
    let mut app = App::demo();
    app.tab = 5;
    app.focus = FocusArea::DetailTabs;
    let text = render(&app, 120, 40);
    for s in ["Profile", "Instance", "Process", "Endpoint"] {
        assert!(text.contains(s), "falta {s}");
    }
}

#[test]
fn tui_uses_daemon_api_only() {
    let cargo = include_str!("../Cargo.toml");
    assert!(!cargo.contains("rusqlite"));
    assert!(!cargo.contains("hli-store"));
}

#[test]
fn command_bar_reuses_cli_parser() {
    let args = split_command("run start --prompt \"hola mundo\" --top-k 5");
    assert_eq!(
        args,
        vec!["run", "start", "--prompt", "hola mundo", "--top-k", "5"]
    );
    let p = hli_tui::client::program();
    assert!(p.file_name().unwrap().to_string_lossy().starts_with("hli"));
}

#[test]
fn json_output_remains_unchanged() {
    let out = std::process::Command::new(hli_tui::client::program())
        .args(["daemon", "status"])
        .env("HLI_HOME", std::env::temp_dir().join("hli-tui-json-empty"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&text).expect("JSON del CLI");
    assert!(v.get("running").is_some());
}

#[test]
fn empty_state_respects_dependency_order() {
    let app = App::empty();
    let text = render(&app, 120, 40);
    assert!(text.contains("Select or create a workspace first"));
    assert!(!text.contains(":run start"));
}

#[test]
fn header_labels_all_none_values() {
    let app = App::empty();
    let text = render(&app, 120, 40);
    assert!(text.contains("workspace: none"));
    assert!(text.contains("thread: none"));
    assert!(text.contains("branch: none"));
}

#[test]
fn language_is_consistent() {
    let app = App::demo();
    let text = render(&app, 120, 40);
    for es in ["Sin selección", "Crea uno", "sugerencia", "Ayuda de HLI"] {
        assert!(!text.contains(es), "texto en español: {es}");
    }
    assert!(text.contains("suggestion"));
}

#[test]
fn inspector_title_matches_selection() {
    let app = App::demo();
    assert!(render(&app, 120, 40).contains("Run run_demo"));
    let empty = App::empty();
    assert!(render(&empty, 120, 40).contains("No selection"));
}

#[test]
fn terminal_is_restored_after_panic() {
    hli_tui::install_panic_hook();
    let _ = std::panic::catch_unwind(|| panic!("test de restauracion"));
    assert!(hli_tui::panic_restored());
}

// --- K1: eventos de teclado -------------------------------------------------

#[test]
fn key_press_is_processed_once() {
    let mut app = App::demo();
    app.start_command();
    app.handle_key(key(KeyCode::Char('a'), KeyEventKind::Press));
    assert_eq!(app.input, "a");
}

#[test]
fn key_release_is_ignored() {
    let mut app = App::demo();
    app.start_command();
    app.handle_key(key(KeyCode::Char('a'), KeyEventKind::Press));
    app.handle_key(key(KeyCode::Char('a'), KeyEventKind::Release));
    assert_eq!(app.input, "a");
}

#[test]
fn key_repeat_is_processed() {
    let mut app = App::demo();
    app.start_command();
    app.handle_key(key(KeyCode::Char('b'), KeyEventKind::Repeat));
    assert_eq!(app.input, "b");
}

#[test]
fn press_and_release_do_not_duplicate_input() {
    let mut app = App::demo();
    app.start_command();
    tap(&mut app, KeyCode::Char('a'));
    tap(&mut app, KeyCode::Char('b'));
    assert_eq!(app.input, "ab");
}

#[test]
fn question_mark_does_not_open_and_close_help() {
    let mut app = App::demo();
    tap(&mut app, KeyCode::Char('?'));
    assert!(app.help);
    assert!(render(&app, 120, 40).contains("HLI · ayuda"));
}

#[test]
fn tab_advances_exactly_one_focus_area() {
    let mut app = App::demo();
    assert_eq!(app.focus, FocusArea::Navigation);
    tap(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, FocusArea::Collection);
}

#[test]
fn command_text_is_not_duplicated() {
    let mut app = App::demo();
    app.start_command();
    for c in ['a', 'b', 'c'] {
        tap(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.input, "abc");
}

// --- K2: feedback de la barra de comandos ----------------------------------

#[test]
fn command_success_feedback_is_visible() {
    let mut app = App::demo();
    app.finish_command(true, "Memory committed: mem_018");
    let text = render(&app, 120, 40);
    assert!(text.contains("Memory committed: mem_018"));
    assert!(text.contains('✓'));
}

#[test]
fn command_error_feedback_is_visible() {
    let mut app = App::demo();
    app.finish_command(false, "workspace not found");
    let text = render(&app, 120, 40);
    assert!(text.contains("workspace not found"));
    assert!(text.contains('×'));
}

#[test]
fn empty_input_does_not_hide_feedback() {
    let mut app = App::demo();
    app.finish_command(true, "Workspace created: ws_1");
    assert!(app.input.is_empty());
    let text = render(&app, 120, 40);
    assert!(text.contains("Workspace created: ws_1"));
    assert!(!text.contains("suggestion:"));
}

#[test]
fn typing_clears_previous_feedback() {
    let mut app = App::demo();
    app.finish_command(true, "ok");
    assert!(app.feedback.is_some());
    app.start_command();
    app.handle_key(key(KeyCode::Char('x'), KeyEventKind::Press));
    assert!(app.feedback.is_none());
}

#[test]
fn escape_clears_feedback() {
    let mut app = App::demo();
    app.finish_command(true, "ok");
    tap(&mut app, KeyCode::Esc);
    assert!(app.feedback.is_none());
}

#[test]
fn feedback_expires_after_timeout() {
    let mut app = App::demo();
    app.finish_command(true, "ok");
    app.feedback.as_mut().unwrap().created_at = Instant::now() - Duration::from_secs(6);
    assert!(app.visible_feedback().is_none());
    assert!(render(&app, 120, 40).contains("suggestion:"));
}

#[test]
fn new_feedback_replaces_previous_feedback() {
    let mut app = App::demo();
    app.finish_command(true, "primero");
    app.finish_command(false, "segundo");
    let fb = app.visible_feedback().unwrap();
    assert_eq!(fb.message, "segundo");
    assert_eq!(fb.kind, FeedbackKind::Error);
}

#[test]
fn command_execution_clears_input_but_keeps_feedback() {
    let mut app = App::demo();
    app.start_command();
    app.input = "memory list".into();
    app.finish_command(true, "mem_1");
    assert!(app.input.is_empty());
    assert!(app.visible_feedback().is_some());
}

// --- Agent Mode -------------------------------------------------------------

fn agent_app() -> App {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.workspace = Some(serde_json::json!({"id": "ws_1", "name": "demo"}));
    app.agent.thread = Some(serde_json::json!({"id": "thr_1", "title": "session"}));
    app.agent.branch = Some(serde_json::json!({"id": "br_1", "name": "main"}));
    app
}

#[test]
fn hli_without_args_opens_agent_mode() {
    assert_eq!(TuiMode::from_arg(None), TuiMode::Agent);
}

#[test]
fn inspect_command_opens_inspector_mode() {
    assert_eq!(TuiMode::from_arg(Some("inspect")), TuiMode::Inspector);
}

#[test]
fn agent_mode_initial_focus_is_composer() {
    let app = agent_app();
    let text = render(&app, 120, 30);
    assert!(text.contains("compose"));
    assert!(text.contains("❯") || text.contains("> "));
}

#[test]
fn agent_mode_does_not_show_admin_panels() {
    let app = agent_app();
    let text = render(&app, 120, 30);
    for panel in ["┌ Runs ", "▸ Navigation", "┌ Context ", "┌ MCH "] {
        assert!(
            !text.contains(panel),
            "panel de administración visible: {panel}"
        );
    }
    assert!(text.contains("transcript"));
}

#[test]
fn new_workspace_requires_single_confirmation() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.composer = "analiza el repo".into();
    app.submit_prompt();
    assert!(matches!(app.agent.offer, Offer::CreateWorkspace { .. }));
    assert_eq!(app.agent.pending.as_deref(), Some("analiza el repo"));
    assert!(app.agent.last_run.is_none());
}

#[test]
fn prompt_creates_thread_and_run_implicitly() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.workspace = Some(serde_json::json!({"id": "ws_1"}));
    let plan = app.plan_prompt();
    assert!(plan.create_thread);
    assert!(plan.create_branch);
}

#[test]
fn followup_reuses_current_thread() {
    let app = agent_app();
    let plan = app.plan_prompt();
    assert!(!plan.create_thread);
    assert!(!plan.create_branch);
}

#[test]
fn ctrl_o_switches_modes_without_losing_state() {
    let mut app = agent_app();
    app.agent.composer = "hola".into();
    app.agent.push(AgentKind::User, "hola");
    app.handle_key(key_with(
        KeyCode::Char('o'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    // Ctrl+O abre el Transcript Mode; Inspector se abre con Ctrl+I.
    assert_eq!(app.mode, TuiMode::Transcript);
    app.handle_key(key_with(
        KeyCode::Char('o'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Agent);
    assert_eq!(app.agent.composer, "hola");
    assert_eq!(app.agent.lines.len(), 1);
}

#[test]
fn escape_returns_from_inspector() {
    let mut app = agent_app();
    app.mode = TuiMode::Inspector;
    app.focus = FocusArea::Navigation;
    tap(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, TuiMode::Agent);
}

#[test]
fn key_release_is_ignored_in_both_modes() {
    let mut app = agent_app();
    app.handle_key(key_with(
        KeyCode::Char('a'),
        KeyEventKind::Release,
        KeyModifiers::NONE,
    ));
    assert!(app.agent.composer.is_empty());
    app.mode = TuiMode::Inspector;
    app.start_command();
    app.handle_key(key_with(
        KeyCode::Char('a'),
        KeyEventKind::Release,
        KeyModifiers::NONE,
    ));
    assert!(app.input.is_empty());
}

#[test]
fn small_terminal_does_not_panic() {
    let app = agent_app();
    let text = render(&app, 39, 9);
    assert!(text.contains("Terminal too small"));
}

#[test]
fn slash_command_reuses_existing_contract() {
    let mut app = agent_app();
    app.run_slash("/inspect");
    assert_eq!(app.mode, TuiMode::Inspector);
    app.mode = TuiMode::Agent;
    app.run_slash("/help");
    assert!(app.help);
    app.help = false;
    app.run_slash("/unknown-command");
    assert!(
        app.agent
            .lines
            .last()
            .is_some_and(|l| l.text.contains("no existe"))
    );
}

#[test]
fn at_reference_resolves_stable_id() {
    let mut app = agent_app();
    app.agent.ref_items[2] = vec![RefItem {
        kind: RefKind::Memory,
        stable_id: "mem_018".into(),
        display: "la fuente es SQLite".into(),
        subtitle: "mem_018 · user".into(),
        content_hash: None,
        revision: None,
        size: None,
    }];
    app.agent.ref_category = 2;
    app.agent.ref_open = true;
    app.agent.composer = "@".into();
    app.agent.ref_start = 0;
    app.reference_insert();
    assert_eq!(app.agent.references[0].stable_id, "mem_018");
    assert!(app.agent.composer.contains("@memory:mem_018"));
}

// --- Slice 2: ejecución observada -------------------------------------------

#[test]
fn async_run_starts_in_non_terminal_state() {
    let mut app = agent_app();
    app.apply_run_status(&serde_json::json!({"run": {"id": "run_1", "state": "queued"}}));
    assert_eq!(app.agent.run_state, "queued");
    assert!(app.agent.last_run.is_none());
}

#[test]
fn tick_ingests_only_new_events() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_1".into());
    app.agent.last_seq = 5;
    let events = serde_json::json!([
        {"run_id": "run_1", "sequence": 3, "type": "model.requested"},
        {"run_id": "run_1", "sequence": 6, "type": "model.requested"},
        {"run_id": "run_1", "sequence": 7, "type": "run.completed"},
        {"run_id": "run_2", "sequence": 9, "type": "model.requested"}
    ]);
    let added = app.ingest_events(events.as_array().unwrap());
    assert_eq!(added, 2);
    assert_eq!(app.agent.last_seq, 7);
    assert_eq!(app.ingest_events(events.as_array().unwrap()), 0);
}

#[test]
fn terminal_run_shows_receipt() {
    let mut app = agent_app();
    app.apply_run_status(&serde_json::json!({"run": {
        "id": "run_1", "state": "completed",
        "receipt": {"provider": "sim", "model": "sim-1",
                    "usage": {"input_tokens": 12, "output_tokens": 4},
                    "cost": {"total": 0.0}}
    }}));
    assert_eq!(app.agent.run_state, "completed");
    let last = &app.agent.lines.last().unwrap().text;
    assert!(last.contains("completed"), "{last}");
    assert!(last.contains("12 in / 4 out"), "{last}");
    assert!(app.agent.receipt_shown);
}

#[test]
fn cancel_marks_cancelling_only_when_active() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_1".into());
    app.agent.run_state = "running".into();
    app.mark_cancelling();
    assert_eq!(app.agent.run_state, "cancelling");
    app.agent.run_state = "completed".into();
    app.mark_cancelling();
    assert_eq!(app.agent.run_state, "completed");
}

#[test]
fn event_lines_skip_unrelated_events() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_9".into());
    let events = serde_json::json!([
        {"run_id": "run_1", "sequence": 10, "type": "model.requested"},
        {"run_id": "run_9", "sequence": 11, "type": "workspace.created"}
    ]);
    assert_eq!(app.ingest_events(events.as_array().unwrap()), 0);
    assert_eq!(app.agent.last_seq, 11);
}

// --- Slice 3a: transcript durable, resume, historial ------------------------

fn run_json(id: &str, thread: &str, prompt: &str, state: &str, start: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "thread_id": thread, "prompt": prompt, "state": state,
        "started_at": start, "ended_at": "2026-11-08T10:00:05Z",
        "result": {"text": format!("respuesta de {id}")},
        "receipt": {"provider": "sim", "model": "sim-1", "result_hash": "sha256:abcdef0123456789",
                    "context_manifest": "ctx_1", "result_artifacts": ["art_091"],
                    "usage": {"input_tokens": 10, "output_tokens": 2}, "cost": {"total": 0.0}}
    })
}

#[test]
fn transcript_projection_preserves_event_order() {
    let runs = vec![run_json(
        "run_1",
        "thr_1",
        "hola",
        "completed",
        "2026-11-08T10:00:00Z",
    )];
    let events = serde_json::json!([
        {"run_id": "run_1", "sequence": 3, "type": "model.responded"},
        {"run_id": "run_1", "sequence": 1, "type": "model.requested"},
        {"run_id": "run_1", "sequence": 2, "type": "context.manifest.created"}
    ]);
    let lines = project_thread(&runs, events.as_array().unwrap());
    let tool: Vec<&str> = lines
        .iter()
        .filter(|l| l.kind == AgentKind::Tool)
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(
        tool,
        vec![
            "model.requested",
            "context.manifest.created",
            "model.responded"
        ]
    );
}

#[test]
fn resume_rebuilds_same_transcript() {
    let runs = vec![run_json(
        "run_1",
        "thr_1",
        "hola",
        "completed",
        "2026-11-08T10:00:00Z",
    )];
    let events = serde_json::json!([{"run_id": "run_1", "sequence": 1, "type": "model.requested"}]);
    let a = project_thread(&runs, events.as_array().unwrap());
    let b = project_thread(&runs, events.as_array().unwrap());
    let ta: Vec<&str> = a.iter().map(|l| l.text.as_str()).collect();
    let tb: Vec<&str> = b.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(ta, tb);
}

#[test]
fn polling_does_not_duplicate_transcript_entries() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_1".into());
    let events = serde_json::json!([{"run_id": "run_1", "sequence": 4, "type": "model.requested"}]);
    let before = app.agent.lines.len();
    app.ingest_events(events.as_array().unwrap());
    app.ingest_events(events.as_array().unwrap());
    assert_eq!(app.agent.lines.len(), before + 1);
}

#[test]
fn mode_switch_does_not_duplicate_messages() {
    let mut app = agent_app();
    app.agent.push(AgentKind::User, "hola");
    let n = app.agent.lines.len();
    app.toggle_mode();
    app.toggle_mode();
    assert_eq!(app.agent.lines.len(), n);
}

#[test]
fn cancelled_run_is_rendered_as_cancelled() {
    let run = run_json(
        "run_1",
        "thr_1",
        "hola",
        "cancelled",
        "2026-11-08T10:00:00Z",
    );
    let text = terminal_summary(&run);
    assert!(text.starts_with("cancelled"), "{text}");
    assert!(!text.contains("completed"), "{text}");
}

#[test]
fn failed_run_is_rendered_as_failed() {
    let mut run = run_json("run_1", "thr_1", "hola", "failed", "2026-11-08T10:00:00Z");
    run["result"] = serde_json::json!({"error": "provider timeout"});
    let text = terminal_summary(&run);
    assert!(text.starts_with("failed"), "{text}");
    assert!(text.contains("Error: provider timeout"), "{text}");
    assert!(!text.contains("completed"), "{text}");
}

#[test]
fn completed_run_shows_artifacts() {
    let run = run_json(
        "run_1",
        "thr_1",
        "hola",
        "completed",
        "2026-11-08T10:00:00Z",
    );
    let text = terminal_summary(&run);
    assert!(text.contains("Artifacts: art_091"), "{text}");
    assert!(text.contains("Receipt: sha256:abcdef01"), "{text}");
    assert!(text.contains("Tokens: 10 in / 2 out"), "{text}");
}

#[test]
fn history_uses_durable_prompts() {
    let mut app = agent_app();
    app.agent.prompts = vec!["primera tarea".into(), "segunda tarea".into()];
    app.agent.history_query.clear();
    assert_eq!(app.history_matches().len(), 2);
}

#[test]
fn history_search_filters_results() {
    let mut app = agent_app();
    app.agent.prompts = vec!["revisa el store".into(), "revisa la tui".into()];
    app.agent.history_query = "tui".into();
    let m = app.history_matches();
    assert_eq!(m, vec!["revisa la tui".to_string()]);
}

#[test]
fn history_selection_does_not_submit() {
    let mut app = agent_app();
    app.agent.prompts = vec!["tarea previa".into()];
    app.agent.history_open = true;
    let before = app.agent.lines.len();
    app.history_choose();
    assert_eq!(app.agent.composer, "tarea previa");
    assert!(app.agent.last_run.is_none());
    assert_eq!(app.agent.lines.len(), before);
    assert!(!app.agent.history_open);
}

#[test]
fn history_is_scoped_to_thread() {
    let runs = vec![
        run_json(
            "run_1",
            "thr_1",
            "de uno",
            "completed",
            "2026-11-08T10:00:00Z",
        ),
        run_json(
            "run_2",
            "thr_2",
            "de dos",
            "completed",
            "2026-11-08T10:01:00Z",
        ),
    ];
    let only_one: Vec<serde_json::Value> = runs
        .into_iter()
        .filter(|r| r["thread_id"] == "thr_1")
        .collect();
    let lines = project_thread(&only_one, &[]);
    let text: String = lines.iter().map(|l| l.text.clone()).collect();
    assert!(text.contains("de uno"));
    assert!(!text.contains("de dos"));
}

// --- Slice 3b: referencias @ ------------------------------------------------

fn agent_with_root(root: &std::path::Path) -> App {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.workspace = Some(serde_json::json!({
        "id": "ws_1", "name": "demo", "root_path": root.to_string_lossy()
    }));
    app.agent.thread = Some(serde_json::json!({"id": "thr_1", "title": "session"}));
    app.agent.branch = Some(serde_json::json!({"id": "br_1", "name": "main"}));
    app
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("hli-ref-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn sha_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(std::fs::read(path).unwrap());
    format!(
        "sha256:{}",
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn file_ref(id: &str, hash: &str) -> Reference {
    Reference {
        kind: RefKind::File,
        stable_id: id.into(),
        display: id.into(),
        content_hash: Some(hash.into()),
        revision: None,
        size: None,
        selected_at: 0,
    }
}

#[test]
fn at_opens_reference_selector() {
    let mut app = agent_with_root(&tmp_dir("open"));
    app.handle_key(key_with(
        KeyCode::Char('@'),
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert!(app.agent.ref_open);
    assert_eq!(app.agent.composer, "@");
}

#[test]
fn at_file_resolves_stable_reference() {
    let r = Reference {
        kind: RefKind::File,
        stable_id: "crates/hli-store/src/lib.rs".into(),
        display: "lib.rs".into(),
        content_hash: Some("sha256:aa".into()),
        revision: None,
        size: Some(10),
        selected_at: 0,
    };
    assert_eq!(reference_token(&r), "@file:crates/hli-store/src/lib.rs");
}

#[test]
fn at_artifact_preserves_hash() {
    let mut app = agent_with_root(&tmp_dir("artifact"));
    app.agent.ref_items[1] = vec![RefItem {
        kind: RefKind::Artifact,
        stable_id: "art_091".into(),
        display: "art_091".into(),
        subtitle: "text/plain".into(),
        content_hash: Some("sha256:beef".into()),
        revision: None,
        size: Some(4),
    }];
    app.agent.ref_category = 1;
    app.agent.ref_open = true;
    app.reference_insert();
    assert_eq!(app.agent.references[0].stable_id, "art_091");
    assert_eq!(
        app.agent.references[0].content_hash.as_deref(),
        Some("sha256:beef")
    );
}

#[test]
fn at_memory_uses_memory_id() {
    let mut app = agent_with_root(&tmp_dir("memory"));
    app.agent.ref_items[2] = vec![RefItem {
        kind: RefKind::Memory,
        stable_id: "mem_018".into(),
        display: "SQLite es la fuente".into(),
        subtitle: "mem_018 · user".into(),
        content_hash: None,
        revision: None,
        size: None,
    }];
    app.agent.ref_category = 2;
    app.agent.ref_open = true;
    app.reference_insert();
    assert_eq!(app.agent.references[0].kind, RefKind::Memory);
    assert_eq!(app.agent.references[0].stable_id, "mem_018");
    assert!(app.agent.composer.contains("@memory:mem_018"));
}

#[test]
fn at_run_uses_run_id() {
    let mut app = agent_with_root(&tmp_dir("run"));
    app.agent.ref_items[3] = vec![RefItem {
        kind: RefKind::Run,
        stable_id: "run_028".into(),
        display: "revisa el store".into(),
        subtitle: "run_028 · completed".into(),
        content_hash: None,
        revision: None,
        size: None,
    }];
    app.agent.ref_category = 3;
    app.agent.ref_open = true;
    app.reference_insert();
    assert_eq!(app.agent.references[0].stable_id, "run_028");
}

#[test]
fn at_branch_records_revision() {
    let mut app = agent_with_root(&tmp_dir("branch"));
    app.agent.ref_items[4] = vec![RefItem {
        kind: RefKind::Branch,
        stable_id: "alternative".into(),
        display: "alternative".into(),
        subtitle: "seq 7".into(),
        content_hash: None,
        revision: Some(7),
        size: None,
    }];
    app.agent.ref_category = 4;
    app.agent.ref_open = true;
    app.reference_insert();
    assert_eq!(app.agent.references[0].revision, Some(7));
    assert!(app.agent.composer.contains("@branch:alternative"));
}

#[test]
fn at_selector_escape_preserves_composer() {
    let mut app = agent_with_root(&tmp_dir("escape"));
    app.agent.composer = "hola ".into();
    app.handle_key(key_with(
        KeyCode::Char('@'),
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.agent.composer, "hola @");
    tap(&mut app, KeyCode::Esc);
    assert_eq!(app.agent.composer, "hola ");
    assert!(!app.agent.ref_open);
}

#[test]
fn missing_reference_is_reported() {
    let root = tmp_dir("missing");
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref("nope.rs", "sha256:00")];
    let warnings = app.stale_references();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("missing"), "{:?}", warnings);
}

#[test]
fn renamed_file_does_not_change_reference_identity() {
    let root = tmp_dir("rename");
    let a = root.join("a.txt");
    std::fs::write(&a, "contenido").unwrap();
    let hash = sha_of(&a);
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref("a.txt", &hash)];
    std::fs::rename(&a, root.join("b.txt")).unwrap();
    let warnings = app.stale_references();
    assert!(warnings[0].contains("missing"));
    assert_eq!(app.agent.references[0].stable_id, "a.txt");
}

#[test]
fn changed_file_is_reported_without_substitution() {
    let root = tmp_dir("changed");
    let a = root.join("a.txt");
    std::fs::write(&a, "uno").unwrap();
    let hash = sha_of(&a);
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref("a.txt", &hash)];
    std::fs::write(&a, "dos").unwrap();
    let warnings = app.stale_references();
    assert!(warnings[0].contains("changed"), "{:?}", warnings);
    assert_eq!(
        app.agent.references[0].content_hash.as_deref(),
        Some(hash.as_str())
    );
}

// --- Slice 3b: actividad expandible, scroll y slash -------------------------

fn detail(kind: &str, error: Option<&str>) -> ActivityDetail {
    ActivityDetail {
        kind: kind.into(),
        status: if error.is_some() {
            "failed".into()
        } else {
            "ok".into()
        },
        started: "2026-11-08T10:00:00Z".into(),
        duration: Some(1.5),
        input: "tool".into(),
        output: "4 matches".into(),
        causation: None,
        correlation: None,
        artifacts: vec!["art_091".into()],
        error: error.map(str::to_string),
    }
}

#[test]
fn tool_activity_is_collapsed_by_default() {
    let runs = vec![run_json(
        "run_1",
        "thr_1",
        "hola",
        "completed",
        "2026-11-08T10:00:00Z",
    )];
    let events = serde_json::json!([
        {"run_id": "run_1", "sequence": 1, "type": "model.responded", "payload": {}}
    ]);
    let lines = project_thread(&runs, events.as_array().unwrap());
    let tool = lines.iter().find(|l| l.kind == AgentKind::Tool).unwrap();
    assert!(!tool.expanded);
    assert!(tool.detail.is_some());
}

#[test]
fn ctrl_e_expands_selected_activity() {
    let mut app = agent_app();
    app.agent
        .push_activity("model.responded", detail("model.responded", None));
    app.handle_key(key_with(
        KeyCode::Char('e'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    let last = app.agent.lines.last().unwrap();
    assert!(last.expanded);
}

#[test]
fn ctrl_e_collapses_selected_activity() {
    let mut app = agent_app();
    app.agent
        .push_activity("model.responded", detail("model.responded", None));
    app.handle_key(key_with(
        KeyCode::Char('e'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    app.handle_key(key_with(
        KeyCode::Char('e'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert!(!app.agent.lines.last().unwrap().expanded);
}

#[test]
fn details_command_toggles_all_activity() {
    let mut app = agent_app();
    app.agent
        .push_activity("uno", detail("model.responded", None));
    app.agent
        .push_activity("dos", detail("memory.search", None));
    app.run_slash("/details");
    assert!(
        app.agent
            .lines
            .iter()
            .filter(|l| l.detail.is_some())
            .all(|l| l.expanded)
    );
    app.run_slash("/details");
    assert!(
        app.agent
            .lines
            .iter()
            .filter(|l| l.detail.is_some())
            .all(|l| !l.expanded)
    );
}

#[test]
fn expanded_activity_survives_poll_refresh() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_1".into());
    app.agent
        .push_activity("uno", detail("model.responded", None));
    app.toggle_selected_activity();
    let events = serde_json::json!([
        {"run_id": "run_1", "sequence": 9, "type": "memory.search", "payload": {}}
    ]);
    app.ingest_events(events.as_array().unwrap());
    assert!(app.agent.lines[0].expanded);
}

#[test]
fn expanded_activity_survives_mode_switch() {
    let mut app = agent_app();
    app.agent
        .push_activity("uno", detail("model.responded", None));
    app.toggle_selected_activity();
    app.toggle_mode();
    app.toggle_mode();
    assert!(app.agent.lines[0].expanded);
}

#[test]
fn failed_activity_shows_structured_error() {
    let mut app = agent_app();
    app.agent.push_activity(
        "provider timeout",
        detail("model.responded", Some("timeout")),
    );
    app.toggle_selected_activity();
    let text = render(&app, 120, 40);
    assert!(text.contains("error:"), "{text}");
    assert!(text.contains("timeout"));
}

#[test]
fn page_up_moves_transcript_and_end_follows() {
    let mut app = agent_app();
    for i in 0..40 {
        app.agent.push(AgentKind::Info, format!("linea {i}"));
    }
    app.scroll_page_up();
    assert!(app.agent.scroll_top.is_some());
    app.scroll_page_down();
    assert!(app.agent.scroll_top.is_some() || app.agent.scroll_top.is_none());
    app.scroll_home();
    assert_eq!(app.agent.scroll_top, Some(0));
    app.scroll_end();
    assert!(app.agent.scroll_top.is_none());
}

#[test]
fn new_activity_does_not_move_manual_scroll() {
    let mut app = agent_app();
    for i in 0..40 {
        app.agent.push(AgentKind::Info, format!("linea {i}"));
    }
    app.scroll_home();
    app.agent.push(AgentKind::Info, "nueva");
    assert_eq!(app.agent.scroll_top, Some(0));
    assert_eq!(app.agent.new_activity, 1);
    let text = render(&app, 120, 40);
    assert!(text.contains("new activity"), "{text}");
}

#[test]
fn new_activity_follows_when_at_bottom() {
    let mut app = agent_app();
    app.agent.push(AgentKind::Info, "uno");
    app.agent.push(AgentKind::Info, "dos");
    assert!(app.agent.scroll_top.is_none());
    assert_eq!(app.agent.new_activity, 0);
}

#[test]
fn long_transcript_keeps_composer_visible() {
    let mut app = agent_app();
    for i in 0..80 {
        app.agent
            .push(AgentKind::Hli, format!("respuesta larga {i}"));
    }
    let text = render(&app, 120, 30);
    assert!(text.contains("compose"), "composer no visible");
    assert!(text.contains("❯") || text.contains("> "));
}

#[test]
fn slash_help_lists_only_available_commands() {
    let cmds = available_commands();
    assert!(cmds.contains(&"provider"));
    assert!(cmds.contains(&"details"));
    assert!(!cmds.contains(&"permissions"));
    assert!(!cmds.contains(&"diff"));
}

#[test]
fn slash_cancel_requests_real_cancellation() {
    let mut app = agent_app();
    app.agent.last_run = Some("run_1".into());
    app.agent.run_state = "running".into();
    app.run_slash("/cancel");
    assert_eq!(app.agent.run_state, "cancelling");
}

#[test]
fn unknown_slash_command_shows_valid_alternatives() {
    let mut app = agent_app();
    app.run_slash("/nope");
    let last = &app.agent.lines.last().unwrap().text;
    assert!(last.contains("disponibles"), "{last}");
}

#[test]
fn slash_commands_never_expose_secrets() {
    let list = vec![serde_json::json!({
        "name": "openai", "kind": "openai", "model": "gpt-4o-mini",
        "api_key": "sk-should-not-appear", "secret_handle": "openai"
    })];
    let lines = format_providers(&list, "openai");
    let joined = lines.join("\n");
    assert!(joined.contains("openai"));
    assert!(!joined.contains("sk-should-not-appear"));
    assert!(!joined.contains("secret"));
}

#[test]
fn selected_references_are_sent_structurally() {
    let mut app = agent_app();
    app.agent.references = vec![
        Reference {
            kind: RefKind::File,
            stable_id: "Cargo.toml".into(),
            display: "Cargo.toml".into(),
            content_hash: Some("sha256:aa".into()),
            revision: None,
            size: Some(10),
            selected_at: 1700000000,
        },
        Reference {
            kind: RefKind::Memory,
            stable_id: "mem_018".into(),
            display: "la fuente".into(),
            content_hash: None,
            revision: None,
            size: None,
            selected_at: 1700000001,
        },
    ];
    let args = app.explicit_reference_args();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0]["reference_type"], "file");
    assert_eq!(args[0]["stable_id"], "Cargo.toml");
    assert_eq!(args[0]["content_hash"], "sha256:aa");
    assert_eq!(args[1]["reference_type"], "memory");
    assert_eq!(args[1]["stable_id"], "mem_018");
    assert_eq!(args[0]["selection_reason"], "user_selected");
}

#[test]
fn visible_reference_token_is_not_used_as_identity() {
    let mut app = agent_app();
    app.agent.references = vec![Reference {
        kind: RefKind::File,
        stable_id: "crates/x.rs".into(),
        display: "archivo renombrado.rs".into(),
        content_hash: Some("sha256:bb".into()),
        revision: None,
        size: None,
        selected_at: 1,
    }];
    let args = app.explicit_reference_args();
    assert_eq!(args[0]["stable_id"], "crates/x.rs");
    assert_eq!(args[0]["display_text"], "archivo renombrado.rs");
}

// --- Preflight de referencias cambiadas -------------------------------------

fn file_ref_at(id: &str, hash: &str) -> Reference {
    Reference {
        kind: RefKind::File,
        stable_id: id.into(),
        display: id.into(),
        content_hash: Some(hash.into()),
        revision: None,
        size: None,
        selected_at: 1,
    }
}

#[test]
fn changed_reference_opens_preflight() {
    let root = tmp_dir("preflight-changed");
    let path = root.join("a.txt");
    std::fs::write(&path, "uno").unwrap();
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref_at("a.txt", &sha_of(&path))];
    std::fs::write(&path, "dos").unwrap();
    app.agent.composer = "usa el archivo".into();
    app.submit_prompt();
    let pf = app.agent.preflight.as_ref().expect("preflight");
    assert_eq!(pf.issues.len(), 1);
    assert_eq!(pf.issues[0].reason, "changed");
    assert!(app.agent.last_run.is_none());
}

#[test]
fn cancel_preserves_composer_and_does_not_create_run() {
    let root = tmp_dir("preflight-cancel");
    let path = root.join("a.txt");
    std::fs::write(&path, "uno").unwrap();
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref_at("a.txt", &sha_of(&path))];
    std::fs::write(&path, "dos").unwrap();
    app.agent.composer = "usa el archivo".into();
    app.submit_prompt();
    app.handle_key(key(KeyCode::Char('c'), KeyEventKind::Press));
    assert!(app.agent.preflight.is_none());
    assert_eq!(app.agent.composer, "usa el archivo");
    assert!(app.agent.last_run.is_none());
}

#[test]
fn refresh_updates_hash() {
    let root = tmp_dir("preflight-refresh");
    let path = root.join("a.txt");
    std::fs::write(&path, "uno").unwrap();
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref_at("a.txt", &sha_of(&path))];
    std::fs::write(&path, "dos").unwrap();
    app.agent.composer = "usa el archivo".into();
    app.submit_prompt();
    app.handle_key(key(KeyCode::Char('r'), KeyEventKind::Press));
    assert_eq!(
        app.agent.references[0].content_hash.as_deref(),
        Some(sha_of(&path).as_str())
    );
    assert!(app.agent.preflight.is_none());
}

#[test]
fn multiple_invalid_references_are_grouped() {
    let root = tmp_dir("preflight-multi");
    for name in ["a.txt", "b.txt"] {
        std::fs::write(root.join(name), "uno").unwrap();
    }
    let mut app = agent_with_root(&root);
    app.agent.references = vec![
        file_ref_at("a.txt", &sha_of(&root.join("a.txt"))),
        file_ref_at("b.txt", &sha_of(&root.join("b.txt"))),
    ];
    for name in ["a.txt", "b.txt"] {
        std::fs::write(root.join(name), "dos").unwrap();
    }
    app.agent.composer = "usa ambos".into();
    app.submit_prompt();
    assert_eq!(app.agent.preflight.as_ref().unwrap().issues.len(), 2);
}

#[test]
fn run_starts_only_after_all_references_are_valid() {
    let root = tmp_dir("preflight-valid");
    let path = root.join("a.txt");
    std::fs::write(&path, "uno").unwrap();
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref_at("a.txt", &sha_of(&path))];
    app.agent.composer = "usa el archivo".into();
    app.submit_prompt();
    assert!(app.agent.preflight.is_none());
}

// --- Invariantes de referencias: clasificadores puros -----------------------

#[test]
fn active_memory_is_materialized() {
    assert_eq!(
        classify_memory(
            "active",
            Some("2020-01-01T00:00:00Z"),
            Some("2030-01-01T00:00:00Z"),
            1_700_000_000
        ),
        None
    );
}

#[test]
fn superseded_memory_is_rejected() {
    assert_eq!(
        classify_memory("superseded", None, None, 0),
        Some("superseded")
    );
}

#[test]
fn revoked_memory_is_rejected() {
    assert_eq!(classify_memory("revoked", None, None, 0), Some("revoked"));
}

#[test]
fn expired_memory_is_rejected() {
    assert_eq!(
        classify_memory("active", None, Some("2020-01-01T00:00:00Z"), 1_700_000_000),
        Some("expired")
    );
}

#[test]
fn future_memory_is_rejected() {
    assert_eq!(
        classify_memory("active", Some("2030-01-01T00:00:00Z"), None, 1_700_000_000),
        Some("not yet valid")
    );
}

#[test]
fn invalid_memory_timestamp_is_rejected() {
    assert_eq!(
        classify_memory("active", Some("no-es-fecha"), None, 0),
        Some("invalid timestamp")
    );
}

#[test]
fn artifact_hash_mismatch_is_rejected() {
    assert_eq!(
        classify_artifact_hash(Some("sha256:aa"), Some("sha256:bb")),
        Some("hash mismatch")
    );
    assert_eq!(
        classify_artifact_hash(Some("sha256:aa"), None),
        Some("not found")
    );
    assert_eq!(
        classify_artifact_hash(Some("sha256:aa"), Some("sha256:aa")),
        None
    );
}

#[test]
fn branch_revision_mismatch_is_detected() {
    assert_eq!(
        classify_branch_revision(Some(3), 4),
        Some("revision mismatch")
    );
    assert_eq!(classify_branch_revision(Some(4), 4), None);
    assert_eq!(classify_branch_revision(None, 9), None);
}

// ---------------------------------------------------------------------------
// Plan durable: overlay, Inspector y transcript
// ---------------------------------------------------------------------------

fn demo_plan() -> PlanView {
    PlanView {
        run_id: "run_1".into(),
        plan_id: "plan_1".into(),
        revision: 2,
        supersedes: Some("plan_0".into()),
        origin: "runtime_generated".into(),
        status: "waiting_approval".into(),
        summary: "3 pasos; efectos: read, write".into(),
        steps: vec![
            PlanStepView {
                order: 1,
                description: "Inspect the workspace".into(),
                effect: "read".into(),
                requires_approval: false,
                tool: None,
            },
            PlanStepView {
                order: 2,
                description: "Modify the selected file".into(),
                effect: "write".into(),
                requires_approval: true,
                tool: Some("filesystem.write".into()),
            },
        ],
        effects: vec!["read".into(), "write".into()],
        invalidation_reasons: vec![],
        plan_hash: "sha256:plan".into(),
        execution_hash: "sha256:exec".into(),
        provider: "local (ollama)".into(),
        model: "m2".into(),
        manifest_id: "ctx_1".into(),
        branch_revision: 3,
        tools: vec!["filesystem.write=ask".into()],
        modules: vec![],
        mch: vec![],
        secrets: vec!["api@v1".into()],
        approval_scope: None,
        approved_by: None,
        created_at: "2026-11-09T00:00:00Z".into(),
        decided_at: None,
        details: false,
    }
}

fn agent_with_plan(plan: PlanView) -> App {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.plan = Some(plan);
    app.agent.run_state = "waiting_approval".into();
    app
}

#[test]
fn plan_is_visible_in_agent_mode() {
    let app = agent_with_plan(demo_plan());
    let text = render(&app, 110, 40);
    assert!(text.contains("Plan requires approval"), "{text}");
    assert!(text.contains("Revision: 2"), "{text}");
    assert!(text.contains("Modify the selected file"), "{text}");
    assert!(text.contains("write · approval required"), "{text}");
}

#[test]
fn runtime_generated_origin_is_visible() {
    let text = render(&agent_with_plan(demo_plan()), 110, 40);
    assert!(text.contains("runtime-generated"), "{text}");
}

#[test]
fn provider_origin_is_visible() {
    let mut plan = demo_plan();
    plan.origin = "provider".into();
    let text = render(&agent_with_plan(plan), 110, 40);
    assert!(text.contains("Origin: provider"), "{text}");
}

#[test]
fn approval_overlay_blocks_prompt_submission() {
    let mut app = agent_with_plan(demo_plan());
    for c in "hola".chars() {
        tap(&mut app, KeyCode::Char(c));
    }
    tap(&mut app, KeyCode::Enter);
    assert!(
        app.agent.composer.is_empty(),
        "el compositor no debe recibir texto"
    );
    assert!(
        !app.agent.lines.iter().any(|l| l.kind == AgentKind::User),
        "no debe enviarse un prompt con la aprobación pendiente"
    );
}

#[test]
fn invalidated_plan_disables_approval() {
    let mut plan = demo_plan();
    plan.invalidation_reasons = vec!["el modelo cambió".into()];
    let text = render(&agent_with_plan(plan), 110, 40);
    assert!(text.contains("Plan invalidated"), "{text}");
    assert!(text.contains("Replan"), "{text}");
    assert!(!text.contains("Approve once"), "{text}");
}

#[test]
fn view_details_never_shows_secret_values() {
    let mut plan = demo_plan();
    plan.details = true;
    let text = render(&agent_with_plan(plan), 140, 50);
    assert!(text.contains("api@v1"), "{text}");
    assert!(!text.contains("sk-"), "{text}");
    assert!(text.contains("snapshot hash"), "{text}");
}

#[test]
fn mode_switch_preserves_plan_overlay() {
    let mut app = agent_with_plan(demo_plan());
    // El overlay consume 'o' sin modificador; el cambio de modo es Ctrl+O.
    tap(&mut app, KeyCode::Char('o'));
    assert_eq!(app.mode, TuiMode::Agent);
    app.handle_key(key_with(
        KeyCode::Char('i'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Inspector);
    app.handle_key(key_with(
        KeyCode::Char('i'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Agent);
    assert!(app.agent.plan.is_some());
    assert!(render(&app, 110, 40).contains("Plan requires approval"));
}

#[test]
fn inspector_displays_plan_and_revision() {
    let mut app = App::demo();
    app.inspector_plan = Some(demo_plan());
    app.tab = 0;
    let text = render(&app, 150, 80);
    assert!(text.contains("Plan ID"), "{text}");
    assert!(text.contains("Revision"), "{text}");
    assert!(text.contains("Execution snapshot"), "{text}");
    assert!(
        text.contains("supersedes") || text.contains("plan_0"),
        "{text}"
    );
}

#[test]
fn inspector_displays_invalidation_reasons() {
    let mut plan = demo_plan();
    plan.invalidation_reasons = vec!["secret api cambió de versión".into()];
    let mut app = App::demo();
    app.inspector_plan = Some(plan);
    let text = render(&app, 150, 80);
    assert!(text.contains("Invalidation reasons"), "{text}");
    assert!(text.contains("cambió de versión"), "{text}");
}

#[test]
fn transcript_projects_plan_events_in_order() {
    let runs = vec![json!({
        "id": "run_1", "state": "completed", "prompt": "write algo",
        "started_at": "2026-11-09T00:00:00Z",
        "receipt": {"provider": "sim", "model": "sim-1", "usage": {}, "cost": {"total": 0.0}, "result_hash": "sha256:r"}
    })];
    let events = vec![
        json!({"sequence": 2, "type": "plan.created", "run_id": "run_1", "payload": {"revision": 1, "origin": "runtime_generated"}}),
        json!({"sequence": 3, "type": "run.waiting_approval", "run_id": "run_1", "payload": {"requested_effects": ["write"]}}),
        json!({"sequence": 4, "type": "plan.approved", "run_id": "run_1", "payload": {"scope": "once", "approved_by": "cli"}}),
        json!({"sequence": 5, "type": "plan.execution_started", "run_id": "run_1", "payload": {}}),
        json!({"sequence": 6, "type": "plan.completed", "run_id": "run_1", "payload": {}}),
    ];
    let lines = project_thread(&runs, &events);
    let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    let idx = |needle: &str| texts.iter().position(|t| t.contains(needle));
    let created = idx("Plan created").expect("plan.created");
    let waiting = idx("Waiting for approval").expect("waiting");
    let approved = idx("Approval granted").expect("approved");
    let started = idx("Execution started").expect("execution");
    let completed = idx("Plan completed").expect("completed");
    assert!(created < waiting && waiting < approved && approved < started && started < completed);
}

#[test]
fn polling_does_not_duplicate_plan_events() {
    let runs = vec![
        json!({"id": "run_1", "state": "completed", "prompt": "p", "started_at": "2026-11-09T00:00:00Z"}),
    ];
    let events = vec![
        json!({"sequence": 2, "type": "plan.created", "run_id": "run_1", "payload": {"revision": 1}}),
    ];
    let a = project_thread(&runs, &events);
    let b = project_thread(&runs, &events);
    assert_eq!(a.len(), b.len());
    assert_eq!(
        a.iter().filter(|l| l.text.contains("Plan created")).count(),
        1
    );
}

#[test]
fn inspector_invalidated_plan_disables_approval() {
    let mut plan = demo_plan();
    plan.invalidation_reasons = vec!["el modelo cambió".into()];
    let mut app = App::demo();
    app.inspector_plan = Some(plan);
    tap(&mut app, KeyCode::Char('y'));
    tap(&mut app, KeyCode::Char('a'));
    tap(&mut app, KeyCode::Char('w'));
    assert!(
        !app.agent
            .lines
            .iter()
            .any(|l| l.text.starts_with("approve:")),
        "un plan invalidado no debe intentar aprobarse"
    );
    assert!(app.inspector_plan.is_some());
}

#[test]
fn inspector_approve_once_uses_daemon_operation() {
    let mut app = App::demo();
    app.inspector_plan = Some(demo_plan());
    tap(&mut app, KeyCode::Char('y'));
    // Sin daemon la operación falla, pero se invoca: misma vía que el overlay.
    assert!(
        app.agent
            .lines
            .iter()
            .any(|l| l.text.starts_with("approve:")),
        "el Inspector debe usar run.approve"
    );
}

#[test]
fn inspector_reject_uses_daemon_operation() {
    let mut app = App::demo();
    app.inspector_plan = Some(demo_plan());
    tap(&mut app, KeyCode::Char('n'));
    assert!(
        app.agent
            .lines
            .iter()
            .any(|l| l.text.starts_with("reject:"))
    );
}

#[test]
fn inspector_replan_uses_daemon_operation() {
    let mut plan = demo_plan();
    plan.invalidation_reasons = vec!["provider eliminado".into()];
    let mut app = App::demo();
    app.inspector_plan = Some(plan);
    tap(&mut app, KeyCode::Char('r'));
    assert!(
        app.agent
            .lines
            .iter()
            .any(|l| l.text.starts_with("replan:"))
    );
}

#[test]
fn inspector_view_details_toggles() {
    let mut app = App::demo();
    app.inspector_plan = Some(demo_plan());
    tap(&mut app, KeyCode::Char('v'));
    assert!(app.inspector_plan.as_ref().unwrap().details);
}

fn demo_perm() -> PermissionView {
    PermissionView {
        request_id: "perm_1".into(),
        run_id: "run_1".into(),
        plan_id: "plan_1".into(),
        plan_revision: 2,
        tool: "filesystem.write".into(),
        operation: "filesystem.write".into(),
        effect: "write".into(),
        target: Some("crates/hli-store/src/lib.rs".into()),
        arguments_hash: "sha256:args".into(),
        arguments_summary: "crates/hli-store/src/lib.rs · 12 bytes".into(),
        reason: "invocación declarada en el plan aprobado".into(),
        status: "pending".into(),
        decision: None,
        decision_scope: None,
        actor: None,
        created_at: "2026-11-09T00:00:00Z".into(),
        decided_at: None,
        effective_policy: "ask".into(),
        grants: vec![],
        details: false,
    }
}

fn agent_with_permission(perm: PermissionView) -> App {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.perm = Some(perm);
    app.agent.run_state = "waiting_approval".into();
    app
}

#[test]
fn permission_overlay_is_visible() {
    let text = render(&agent_with_permission(demo_perm()), 120, 45);
    assert!(text.contains("Permission required"), "{text}");
    assert!(text.contains("filesystem.write"), "{text}");
    assert!(text.contains("crates/hli-store/src/lib.rs"), "{text}");
    assert!(text.contains("Allow once"), "{text}");
}

#[test]
fn permission_overlay_blocks_prompt_submission() {
    let mut app = agent_with_permission(demo_perm());
    for c in "hola".chars() {
        tap(&mut app, KeyCode::Char(c));
    }
    tap(&mut app, KeyCode::Enter);
    assert!(app.agent.composer.is_empty());
    assert!(!app.agent.lines.iter().any(|l| l.kind == AgentKind::User));
}

#[test]
fn permission_details_hide_secret_values() {
    let mut perm = demo_perm();
    perm.details = true;
    perm.arguments_summary = "token=<redacted>".into();
    let text = render(&agent_with_permission(perm), 140, 50);
    assert!(text.contains("arguments_hash"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    assert!(!text.contains("sk-"), "{text}");
}

#[test]
fn mode_switch_preserves_permission_overlay() {
    let mut app = agent_with_permission(demo_perm());
    app.handle_key(key_with(
        KeyCode::Char('i'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Inspector);
    app.handle_key(key_with(
        KeyCode::Char('i'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Agent);
    assert!(app.agent.perm.is_some());
    assert!(render(&app, 120, 45).contains("Permission required"));
}

#[test]
fn inspector_displays_permission_request() {
    let mut app = App::demo();
    app.inspector_perm = Some(demo_perm());
    let text = render(&app, 150, 80);
    assert!(text.contains("Tool permission"), "{text}");
    assert!(text.contains("Arguments hash"), "{text}");
    assert!(text.contains("perm_1"), "{text}");
}

// --- K4: orden causal del transcript ---------------------------------------

fn run_with_terminal() -> Value {
    json!({
        "id": "run_k4", "state": "completed", "prompt": "tarea",
        "started_at": "2026-11-09T00:00:00Z",
        "result": {"text": "respuesta"},
        "receipt": {
            "provider": "sim", "model": "sim-1", "usage": {}, "cost": {"total": 0.0},
            "result_hash": "sha256:r", "terminal_event": "evt_9"
        }
    })
}

fn sorted_max_seq() -> i64 {
    k4_events()
        .iter()
        .filter_map(|e| e["sequence"].as_i64())
        .max()
        .unwrap_or(0)
}

fn k4_events() -> Vec<Value> {
    vec![
        json!({"sequence": 5, "type": "run.completed", "run_id": "run_k4", "payload": {"state": "completed"}}),
        json!({"sequence": 2, "type": "run.queued", "run_id": "run_k4", "payload": {}}),
        json!({"sequence": 4, "type": "tool.completed", "run_id": "run_k4", "payload": {"tool": "filesystem.write"}}),
        json!({"sequence": 1, "type": "plan.created", "run_id": "run_k4", "payload": {"revision": 1}}),
        json!({"sequence": 3, "type": "run.planning", "run_id": "run_k4", "payload": {}}),
    ]
}

#[test]
fn transcript_orders_run_events_by_sequence() {
    let lines = project_thread(&[run_with_terminal()], &k4_events());
    let kinds: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    // prompt, plan.created, run.queued, run.planning, tool.completed, respuesta,
    // run.completed, resumen.
    let pos = |needle: &str| kinds.iter().position(|t| t.contains(needle)).unwrap();
    assert!(pos("Plan created") < pos("run.queued"));
    assert!(pos("run.queued") < pos("run.planning"));
    assert!(pos("run.planning") < pos("tool.completed"));
    assert!(pos("tool.completed") < pos("run.completed"));
}

#[test]
fn terminal_summary_follows_terminal_event() {
    let lines = project_thread(&[run_with_terminal()], &k4_events());
    let last = lines.last().unwrap();
    assert!(last.text.contains("completed"), "último: {}", last.text);
    let terminal = lines
        .iter()
        .position(|l| l.text == "run.completed")
        .expect("evento terminal");
    assert_eq!(
        terminal + 1,
        lines.len() - 1,
        "resumen justo tras el terminal"
    );
}

#[test]
fn plan_is_rendered_before_execution_events() {
    let lines = project_thread(&[run_with_terminal()], &k4_events());
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    let plan = texts
        .iter()
        .position(|t| t.contains("Plan created"))
        .unwrap();
    let exec = texts
        .iter()
        .position(|t| t.contains("tool.completed"))
        .unwrap();
    assert!(plan < exec);
}

#[test]
fn polling_and_projection_produce_same_order() {
    let runs = vec![run_with_terminal()];
    let events = k4_events();
    let projected = project_thread(&runs, &events);
    let mut app = App::demo();
    app.agent.lines = projected.clone();
    app.agent.last_seq = sorted_max_seq();
    app.agent.last_run = Some("run_k4".into());
    let mut sorted = events.clone();
    sorted.sort_by_key(|e| e["sequence"].as_i64().unwrap_or(0));
    // La ingesta en vivo de los mismos eventos no altera el orden proyectado.
    app.ingest_events(&sorted);
    let a: Vec<String> = projected.iter().map(|l| l.text.clone()).collect();
    let b: Vec<String> = app.agent.lines.iter().map(|l| l.text.clone()).collect();
    assert_eq!(a, b);
}

#[test]
fn resume_preserves_identical_event_order() {
    let a = project_thread(&[run_with_terminal()], &k4_events());
    let b = project_thread(&[run_with_terminal()], &k4_events());
    assert_eq!(
        a.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
        b.iter().map(|l| l.text.clone()).collect::<Vec<_>>()
    );
}

// --- K5: compositor -------------------------------------------------------

#[test]
fn accepted_prompt_clears_composer() {
    let mut app = App::demo();
    app.agent.composer = "hola".into();
    app.apply_submission_outcome(true, "hola");
    assert!(app.agent.composer.is_empty());
}

#[test]
fn failed_prompt_submission_preserves_composer() {
    let mut app = App::demo();
    app.agent.composer = "hola".into();
    app.apply_submission_outcome(false, "hola");
    assert_eq!(app.agent.composer, "hola");
}

#[test]
fn failed_submit_without_daemon_keeps_the_prompt() {
    let mut app = App::demo();
    app.agent.workspace = Some(json!({"id": "ws_demo"}));
    app.agent.thread = Some(json!({"id": "thr_demo"}));
    app.agent.branch = Some(json!({"id": "br_main"}));
    app.agent.composer = "no se pierde".into();
    assert!(!app.submit_prompt());
    assert_eq!(app.agent.composer, "no se pierde");
}

#[test]
fn followup_starts_with_empty_composer() {
    let mut app = App::demo();
    app.agent.composer = "uno".into();
    app.apply_submission_outcome(true, "uno");
    assert!(app.agent.composer.is_empty());
    app.agent.composer.push_str("dos");
    assert_eq!(app.agent.composer, "dos");
}

#[test]
fn resume_does_not_restore_last_prompt_into_composer() {
    let mut app = App::demo();
    app.agent.prompts = vec!["prompt viejo".into()];
    app.agent.composer.clear();
    app.load_transcript();
    assert!(app.agent.composer.is_empty());
}

// --- Scroll: rueda, teclado, follow y scrollbar ---------------------------

fn mouse(kind: crossterm::event::MouseEventKind) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: 10,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }
}

fn long_transcript(app: &mut App, n: usize) {
    for i in 0..n {
        app.agent
            .push(AgentKind::Info, format!("linea historica {i}"));
    }
    app.observe_terminal(80, 40);
}

#[test]
fn mouse_wheel_up_changes_visible_lines() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_bottom();
    let before = app.agent.viewport.first_visible_line;
    assert!(app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollUp)));
    assert_eq!(app.agent.viewport.first_visible_line, before - 3);
}

#[test]
fn mouse_wheel_down_changes_visible_lines() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_scroll(-30);
    let before = app.agent.viewport.first_visible_line;
    assert!(app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown)));
    assert_eq!(app.agent.viewport.first_visible_line, before + 3);
}

#[test]
fn mouse_wheel_clamps_at_bounds() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_start();
    for _ in 0..50 {
        app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollUp));
    }
    assert_eq!(app.agent.viewport.first_visible_line, 0);
    for _ in 0..400 {
        app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown));
    }
    assert_eq!(
        app.agent.viewport.first_visible_line,
        app.agent.viewport.max_scroll()
    );
}

#[test]
fn scroll_works_with_composer_focused() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.composer = "texto en curso".into();
    app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollUp));
    assert!(!app.agent.viewport.follow_mode);
    assert_eq!(app.agent.composer, "texto en curso");
}

#[test]
fn scrollbar_matches_viewport_position() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.observe_terminal(80, 40);
    assert!(app.agent.viewport.total_rendered_lines > app.agent.viewport.viewport_height);
    let text = render(&app, 80, 40);
    assert!(
        text.contains('█') || text.contains('│'),
        "sin scrollbar visible"
    );
    assert!(app.agent.viewport.thumb_ratio() < 1.0);
}

#[test]
fn scrollbar_hidden_when_content_fits() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.lines.clear();
    app.agent.push(AgentKind::Info, "una linea");
    app.observe_terminal(120, 40);
    assert!(app.agent.viewport.total_rendered_lines <= app.agent.viewport.viewport_height);
    assert_eq!(app.agent.viewport.thumb_ratio(), 1.0);
}

#[test]
fn scroll_up_disables_follow_mode() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    assert!(app.agent.viewport.follow_mode);
    tap(&mut app, KeyCode::PageUp);
    assert!(!app.agent.viewport.follow_mode);
}

#[test]
fn new_activity_preserves_historical_position() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    let before = app.agent.viewport.first_visible_line;
    app.agent.push(AgentKind::Info, "evento nuevo");
    assert_eq!(app.agent.viewport.first_visible_line, before);
}

#[test]
fn new_activity_increments_unseen_count() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    app.agent.viewport.unseen_activity_count = 0;
    app.agent.push(AgentKind::Info, "evento nuevo");
    assert_eq!(app.agent.viewport.unseen_activity_count, 1);
}

#[test]
fn jump_to_bottom_clears_unseen_count() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    app.agent.push(AgentKind::Info, "nuevo");
    assert!(app.agent.viewport.unseen_activity_count > 0);
    app.viewport_bottom();
    assert_eq!(app.agent.viewport.unseen_activity_count, 0);
    assert!(app.agent.viewport.follow_mode);
}

#[test]
fn wheel_to_bottom_reenables_follow() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.viewport_scroll(-30);
    for _ in 0..100 {
        app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown));
    }
    assert!(app.agent.viewport.follow_mode);
}

#[test]
fn ctrl_end_reenables_follow() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.viewport_scroll(-30);
    assert!(!app.agent.viewport.follow_mode);
    app.handle_key(key_with(
        KeyCode::End,
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert!(app.agent.viewport.follow_mode);
    assert_eq!(app.agent.viewport.unseen_activity_count, 0);
}

#[test]
fn page_up_scrolls_half_viewport() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_bottom();
    let height = app.agent.viewport.viewport_height;
    tap(&mut app, KeyCode::PageUp);
    assert_eq!(
        app.agent.viewport.first_visible_line,
        app.agent.viewport.max_scroll() - height / 2
    );
}

#[test]
fn page_down_scrolls_half_viewport() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_start();
    let height = app.agent.viewport.viewport_height;
    tap(&mut app, KeyCode::PageDown);
    assert_eq!(app.agent.viewport.first_visible_line, height / 2);
}

#[test]
fn ctrl_home_moves_to_start() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_bottom();
    app.handle_key(key_with(
        KeyCode::Home,
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.agent.viewport.first_visible_line, 0);
    assert!(!app.agent.viewport.follow_mode);
}

#[test]
fn resize_recalculates_wrapped_lines() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.lines.clear();
    app.agent.push(AgentKind::Info, "x".repeat(300));
    app.observe_terminal(120, 40);
    let wide = app.agent.viewport.total_rendered_lines;
    app.observe_terminal(40, 40);
    let narrow = app.agent.viewport.total_rendered_lines;
    assert!(narrow > wide, "más líneas al estrechar: {narrow} vs {wide}");
}

#[test]
fn resize_clamps_scroll_position() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.observe_terminal(40, 40);
    app.viewport_bottom();
    app.observe_terminal(120, 60);
    assert!(app.agent.viewport.first_visible_line <= app.agent.viewport.max_scroll());
}

#[test]
fn expanded_activity_changes_total_lines_safely() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.lines.clear();
    app.agent.push_activity(
        "tool.completed",
        ActivityDetail {
            kind: "tool".into(),
            status: "success".into(),
            started: "now".into(),
            duration: Some(0.1),
            input: "{}".into(),
            output: "salida larga ".repeat(50),
            causation: None,
            correlation: None,
            artifacts: vec![],
            error: None,
        },
    );
    app.observe_terminal(60, 40);
    let collapsed = app.agent.viewport.total_rendered_lines;
    app.toggle_selected_activity();
    app.observe_terminal(60, 40);
    assert!(app.agent.viewport.total_rendered_lines > collapsed);
    assert!(app.agent.viewport.first_visible_line <= app.agent.viewport.max_scroll());
}

#[test]
fn long_response_keeps_composer_visible() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 300);
    let text = render(&app, 100, 40);
    assert!(
        text.contains("compose"),
        "el compositor debe seguir visible"
    );
}

// --- Transcript Mode y búsqueda ------------------------------------------

fn agent_with_long_history() -> App {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.workspace = Some(serde_json::json!({"id": "ws", "name": "w"}));
    app.agent.thread = Some(serde_json::json!({"id": "th", "title": "t"}));
    app.agent.lines.clear();
    for i in 0..60 {
        app.agent.push(AgentKind::Info, format!("linea {i}"));
    }
    app.agent.push(AgentKind::User, "pregunta clave");
    for i in 0..60 {
        app.agent.push(AgentKind::Info, format!("respuesta {i}"));
    }
    app.observe_terminal(100, 40);
    app
}

#[test]
fn transcript_mode_opens_and_closes() {
    let mut app = agent_with_long_history();
    tap(&mut app, KeyCode::Char('q'));
    // 'q' en Agent se escribe en el compositor; el modo se abre con Ctrl+O.
    app.agent.composer.clear();
    app.handle_key(key_with(
        KeyCode::Char('o'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Transcript);
    let text = render(&app, 100, 40);
    assert!(text.contains("conversation"), "{text}");
    app.handle_key(key_with(
        KeyCode::Esc,
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.mode, TuiMode::Agent);
}

#[test]
fn transcript_search_finds_matches() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('/'));
    assert!(app.transcript.search_open);
    for c in "clave".chars() {
        tap(&mut app, KeyCode::Char(c));
    }
    app.handle_key(key_with(
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert!(!app.transcript.matches.is_empty());
    assert!(app.transcript.scroll > 0);
}

#[test]
fn transcript_search_restores_position_on_cancel() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.transcript.scroll = 0;
    tap(&mut app, KeyCode::Char('/'));
    tap(&mut app, KeyCode::Char('x'));
    app.handle_key(key_with(
        KeyCode::Esc,
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.transcript.scroll, 0);
    assert!(app.transcript.query.is_empty());
    assert!(app.transcript.matches.is_empty());
}

#[test]
fn transcript_jump_between_prompts() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.transcript.scroll = 0;
    tap(&mut app, KeyCode::Char('}'));
    let at_prompt = app.transcript.scroll;
    assert!(at_prompt > 0);
    let lines = app.transcript_lines(app.transcript_width);
    assert!(lines[at_prompt].2, "debe caer en un prompt");
}

#[test]
fn transcript_mode_preserves_composer() {
    let mut app = agent_with_long_history();
    app.agent.composer = "borrador".into();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('/'));
    tap(&mut app, KeyCode::Char('l'));
    app.close_transcript();
    assert_eq!(app.agent.composer, "borrador");
}

#[test]
fn transcript_mode_is_separate_from_inspector() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.open_inspector();
    assert_eq!(app.mode, TuiMode::Inspector);
    assert_ne!(app.mode, TuiMode::Transcript);
}

#[test]
fn mouse_guard_can_be_disabled_by_env() {
    // El flag se evalúa en el arranque; aquí se comprueba el contrato del
    // helper sin tocar el entorno global del proceso.
    let guard = hli_tui::terminal::MouseGuard::new(false).unwrap();
    assert!(!guard.enabled());
}

#[test]
fn mouse_release_is_idempotent() {
    hli_tui::terminal::release_mouse();
    hli_tui::terminal::release_mouse();
}

// --- Cobertura adicional del bloque scroll/transcript ---------------------

#[test]
fn mouse_wheel_clamps_at_start() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.agent.viewport.first_visible_line = 2;
    for _ in 0..20 {
        app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollUp));
    }
    assert_eq!(app.agent.viewport.first_visible_line, 0);
}

#[test]
fn mouse_wheel_clamps_at_end() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    for _ in 0..400 {
        app.handle_mouse(mouse(crossterm::event::MouseEventKind::ScrollDown));
    }
    assert_eq!(
        app.agent.viewport.first_visible_line,
        app.agent.viewport.max_scroll()
    );
}

#[test]
fn new_activity_indicator_is_visible() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    app.agent.push(AgentKind::Info, "evento nuevo");
    let text = render(&app, 120, 40);
    assert!(text.contains("new activity"), "{text}");
    assert!(text.contains("Ctrl+End"), "{text}");
}

#[test]
fn indicator_does_not_overlap_composer() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    app.agent.push(AgentKind::Info, "nuevo");
    let text = render(&app, 100, 40);
    let compose_at = text.find("compose").expect("compositor");
    let indicator_at = text.find("new activity").expect("indicador");
    assert!(
        indicator_at > compose_at,
        "el indicador va en la barra de estado"
    );
}

#[test]
fn plan_overlay_is_visible_while_scrolled_up() {
    let mut app = agent_with_plan(demo_plan());
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    assert!(!app.agent.viewport.follow_mode);
    let text = render(&app, 110, 40);
    assert!(text.contains("Plan requires approval"), "{text}");
}

#[test]
fn permission_overlay_is_visible_while_scrolled_up() {
    let mut app = agent_with_permission(demo_perm());
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    let text = render(&app, 120, 45);
    assert!(text.contains("Permission required"), "{text}");
}

#[test]
fn overlay_does_not_enable_follow_mode() {
    let mut app = agent_with_plan(demo_plan());
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    let _ = render(&app, 110, 40);
    assert!(!app.agent.viewport.follow_mode);
}

#[test]
fn closing_overlay_preserves_scroll_position() {
    let mut app = agent_with_plan(demo_plan());
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-20);
    let before = app.agent.viewport.first_visible_line;
    app.handle_key(key_with(
        KeyCode::Esc,
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert_eq!(app.agent.viewport.first_visible_line, before);
}

#[test]
fn ctrl_o_opens_transcript_mode() {
    let mut app = agent_with_long_history();
    app.handle_key(key_with(
        KeyCode::Char('o'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Transcript);
}

#[test]
fn ctrl_i_opens_inspector_not_transcript() {
    let mut app = agent_with_long_history();
    app.handle_key(key_with(
        KeyCode::Char('i'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.mode, TuiMode::Inspector);
    assert_ne!(app.mode, TuiMode::Transcript);
}

#[test]
fn transcript_scrolls_one_line() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.transcript.scroll = 5;
    tap(&mut app, KeyCode::Char('j'));
    assert_eq!(app.transcript.scroll, 6);
    tap(&mut app, KeyCode::Char('k'));
    assert_eq!(app.transcript.scroll, 5);
}

#[test]
fn transcript_scrolls_half_page() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.transcript.scroll = 10;
    let page = app.transcript_height / 2;
    app.handle_key(key_with(
        KeyCode::Char('d'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.transcript.scroll, 10 + page.max(1));
}

#[test]
fn transcript_scrolls_full_page() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    app.transcript.scroll = 10;
    let page = app.transcript_height;
    app.handle_key(key_with(
        KeyCode::Char('f'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert_eq!(app.transcript.scroll, 10 + page);
}

#[test]
fn transcript_jumps_to_start_and_end() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('G'));
    let end = app.transcript.scroll;
    assert!(end > 0);
    tap(&mut app, KeyCode::Char('g'));
    assert_eq!(app.transcript.scroll, 0);
}

#[test]
fn transcript_search_finds_tool_activity() {
    let mut app = agent_with_long_history();
    app.agent
        .push(AgentKind::Tool, "tool.completed · filesystem.write");
    app.observe_terminal(100, 40);
    app.open_transcript();
    tap(&mut app, KeyCode::Char('/'));
    for c in "filesystem".chars() {
        tap(&mut app, KeyCode::Char(c));
    }
    app.handle_key(key_with(
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::NONE,
    ));
    assert!(!app.transcript.matches.is_empty());
}

#[test]
fn transcript_exit_preserves_composer() {
    let mut app = agent_with_long_history();
    app.agent.composer = "borrador".into();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('q'));
    assert_eq!(app.mode, TuiMode::Agent);
    assert_eq!(app.agent.composer, "borrador");
}

#[test]
fn transcript_exit_preserves_agent_scroll() {
    let mut app = agent_with_long_history();
    app.agent.viewport.scroll(-15);
    let before = app.agent.viewport.first_visible_line;
    app.open_transcript();
    app.close_transcript();
    assert_eq!(app.agent.viewport.first_visible_line, before);
}

#[test]
fn resize_preserves_follow_mode() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    assert!(app.agent.viewport.follow_mode);
    app.observe_terminal(50, 30);
    assert!(app.agent.viewport.follow_mode);
}

#[test]
fn narrow_terminal_keeps_jump_indicator_readable() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.agent.viewport.scroll(-10);
    app.agent.push(AgentKind::Info, "nuevo");
    let text = render(&app, 70, 30);
    assert!(text.contains("new activity"), "{text}");
}

#[test]
fn disable_mouse_preserves_keyboard_scroll() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 100);
    app.viewport_bottom();
    tap(&mut app, KeyCode::PageUp);
    assert!(!app.agent.viewport.follow_mode);
    app.handle_key(key_with(
        KeyCode::End,
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
    ));
    assert!(app.agent.viewport.follow_mode);
}

#[test]
fn disable_mouse_preserves_transcript_mode() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    assert_eq!(app.mode, TuiMode::Transcript);
    tap(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, TuiMode::Agent);
}

#[test]
fn accepted_prompt_clears_real_composer_state() {
    let mut app = App::demo();
    app.agent.composer = "aceptado".into();
    app.apply_submission_outcome(true, "aceptado");
    assert!(app.agent.composer.is_empty());
    assert!(
        app.agent.composer.is_empty(),
        "seguir vacío en el follow-up"
    );
}

#[test]
fn failed_submission_preserves_real_composer_state() {
    let mut app = App::demo();
    app.agent.workspace = Some(serde_json::json!({"id": "ws"}));
    app.agent.thread = Some(serde_json::json!({"id": "th"}));
    app.agent.branch = Some(serde_json::json!({"id": "br"}));
    app.agent.composer = "fallo".into();
    assert!(!app.submit_prompt());
    assert_eq!(app.agent.composer, "fallo");
}

// --- Interfaz ampliada: scrollback, export, clic y statusline --------------

#[test]
fn transcript_dump_requests_scrollback() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('['));
    assert!(app.transcript_dump);
    assert_eq!(app.mode, TuiMode::Agent);
}

#[test]
fn transcript_export_requests_file() {
    let mut app = agent_with_long_history();
    app.open_transcript();
    tap(&mut app, KeyCode::Char('v'));
    assert!(app.transcript_export.is_some());
    assert_eq!(app.mode, TuiMode::Agent);
}

#[test]
fn scrollbar_click_jumps_proportionally() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    long_transcript(&mut app, 200);
    app.viewport_bottom();
    let row = 3 + (app.agent.viewport.viewport_height as u16 / 2);
    app.handle_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: app.terminal_width as u16 - 1,
        row,
        modifiers: KeyModifiers::NONE,
    });
    let line = app.agent.viewport.first_visible_line;
    assert!(line > 0 && line < app.agent.viewport.max_scroll());
}

#[test]
fn click_expands_activity() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.lines.clear();
    app.agent.push_activity(
        "tool.completed",
        ActivityDetail {
            kind: "tool".into(),
            status: "success".into(),
            started: "now".into(),
            duration: Some(0.1),
            input: "{}".into(),
            output: "salida".into(),
            causation: None,
            correlation: None,
            artifacts: vec![],
            error: None,
        },
    );
    app.observe_terminal(100, 40);
    assert!(!app.agent.lines[0].expanded);
    app.handle_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 5,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });
    assert!(app.agent.lines[0].expanded);
}

#[test]
fn selection_clears_after_copy() {
    let mut app = agent_with_long_history();
    app.selection_start(0);
    app.selection_update(3);
    app.selection_copy();
    assert!(app.sel.is_none());
    assert!(app.sel_end.is_none());
}

#[test]
fn statusline_parses_ui_toml() {
    assert_eq!(
        App::parse_statusline("[ui]\nstatusline = \"date\"\n"),
        Some("date".into())
    );
    assert_eq!(App::parse_statusline("# nada\n"), None);
}

#[test]
fn screen_reader_uses_classic_renderer() {
    // El contrato: `HLI_SCREEN_READER=1` activa el renderer de texto plano.
    let code = include_str!("../src/main.rs");
    assert!(code.contains("HLI_SCREEN_READER"));
    assert!(code.contains("classic::run"));
}

// --- Pestañas y selección de modelo ---------------------------------------

#[test]
fn tabs_cycle_workspace_session_chat() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    assert_eq!(app.agent.agent_tab, hli_tui::app::AgentTab::Chat);
    app.cycle_tab(true);
    assert_eq!(app.agent.agent_tab, hli_tui::app::AgentTab::Workspace);
    app.cycle_tab(true);
    assert_eq!(app.agent.agent_tab, hli_tui::app::AgentTab::Session);
    app.cycle_tab(true);
    assert_eq!(app.agent.agent_tab, hli_tui::app::AgentTab::Chat);
    app.cycle_tab(false);
    assert_eq!(app.agent.agent_tab, hli_tui::app::AgentTab::Session);
}

#[test]
fn workspace_tab_renders_the_list_not_the_transcript() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.agent_tab = hli_tui::app::AgentTab::Workspace;
    app.workspaces = vec![serde_json::json!({"id":"ws_1","name":"demo","root_path":"C:/tmp/demo"})];
    let text = render(&app, 100, 30);
    assert!(text.contains("Workspace"));
    assert!(text.contains("demo"));
    assert!(text.contains("elige dónde trabajar"));
}

#[test]
fn model_picker_falls_back_to_sim_without_daemon() {
    let mut app = App::demo();
    app.agent.model_open = false;
    app.open_model_picker();
    assert!(app.agent.model_open);
    assert!(!app.agent.providers.is_empty());
    assert_eq!(app.agent.providers[0]["name"], "sim");
}

#[test]
fn apply_model_sets_provider_and_model() {
    let mut app = App::demo();
    app.agent.providers = vec![serde_json::json!({
        "name": "ollama", "kind": "ollama", "model": "llama3.1:8b"
    })];
    app.agent.model_sel = 0;
    app.agent.model_open = true;
    app.apply_model();
    assert!(!app.agent.model_open);
    assert_eq!(app.agent.provider, "ollama");
    assert_eq!(app.agent.model, "llama3.1:8b");
}

#[test]
fn missing_file_requires_cancel_only() {
    let root = tmp_dir("preflight-missing");
    let mut app = agent_with_root(&root);
    app.agent.references = vec![file_ref_at("nope.txt", "sha256:00")];
    app.agent.composer = "usa".into();
    app.submit_prompt();
    let pf = app.agent.preflight.as_ref().unwrap();
    assert_eq!(pf.issues[0].reason, "missing");
    assert!(!pf.issues[0].refreshable);
}
