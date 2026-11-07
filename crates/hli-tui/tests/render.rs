use ratatui::backend::TestBackend;
use ratatui::Terminal;

use hli_tui::app::{split_command, App};
use hli_tui::ui;

fn render(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    buffer.content().iter().map(|c| c.symbol()).collect()
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
    let text = render(&app, 120, 40);
    assert!(text.contains("superseded"));
    assert!(text.contains("legacy"));
}

#[test]
fn tui_distinguishes_mch_entities() {
    let mut app = App::demo();
    app.tab = 5;
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
    assert!(cargo.contains("serde_json"));
}

#[test]
fn command_bar_reuses_cli_parser() {
    let args = split_command("run start --prompt \"hola mundo\" --top-k 5");
    assert_eq!(args, vec!["run", "start", "--prompt", "hola mundo", "--top-k", "5"]);
    let p = hli_tui::client::program();
    let name = p.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("hli"));
}

#[test]
fn json_output_remains_unchanged() {
    let out = std::process::Command::new(hli_tui::client::program())
        .arg("daemon")
        .arg("status")
        .env("HLI_HOME", std::env::temp_dir().join("hli-tui-json-empty"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&text).expect("el CLI debe seguir emitiendo JSON");
    assert!(v.get("running").is_some());
}
