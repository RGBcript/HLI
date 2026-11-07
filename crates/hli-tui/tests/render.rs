use ratatui::Terminal;
use ratatui::backend::TestBackend;

use hli_tui::app::{App, FocusArea, LayoutMode, layout_mode, split_command};
use hli_tui::ui;

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
    assert!(text.contains("HLI help"));
    app.close_help();
    let text = render(&app, 120, 40);
    assert!(!text.contains("HLI help"));
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
