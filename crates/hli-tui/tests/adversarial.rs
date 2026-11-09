//! Tests adversariales: entradas raras, tamaños imposibles y estados que la
//! UI no debería producir. Cada fallo aquí es un hallazgo, no un test roto.

use std::panic::{AssertUnwindSafe, catch_unwind};

use hli_tui::app::{App, TuiMode};
use hli_tui::layout::{agent_regions, body_inner_height};
use hli_tui::viewport::{detail_rows, parse_wheel_speed, wrapped_lines};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde_json::json;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn draw_no_panic(app: &App, w: u16, h: u16) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| hli_tui::ui::draw(f, app)).unwrap();
    }))
    .is_ok()
}

#[test]
fn wrapped_lines_counts_display_width_not_chars() {
    // 30 caracteres CJK ocupan 60 columnas; en 20 columnas son 3 filas.
    let cjk = "漢".repeat(30);
    assert_eq!(wrapped_lines(&cjk, 20), 3, "CJK subestimado");
    // Un emoji compuesto ocupa 2 columnas.
    let emoji = "😀".repeat(20);
    assert_eq!(wrapped_lines(&emoji, 20), 2, "emoji subestimado");
}

#[test]
fn wrapped_lines_survives_pathological_input() {
    assert_eq!(wrapped_lines("", 0), 1);
    assert_eq!(wrapped_lines("\n\n\n", 0), 4);
    assert!(wrapped_lines(&"x".repeat(100_000), 20) > 0);
}

#[test]
fn detail_rows_matches_render_with_wide_text() {
    let out = "漢".repeat(40);
    // El detalle pinta 5 líneas fijas + input + output envueltos.
    let counted = detail_rows("", &out, false, false, 40);
    assert_eq!(counted, 5 + wrapped_lines("", 36) + wrapped_lines(&out, 36));
}

#[test]
fn layout_covers_every_terminal_size() {
    for (w, h) in [(1, 1), (20, 5), (40, 10), (200, 60), (80, 2)] {
        let area = ratatui::layout::Rect::new(0, 0, w, h);
        let r = agent_regions(area, false);
        assert!(r.body.height <= area.height);
        assert_eq!(r.status.bottom(), area.bottom());
        assert_eq!(
            body_inner_height(r.body),
            r.body.height.saturating_sub(1) as usize
        );
    }
}

#[test]
fn draw_never_panics_on_tiny_or_absurd_sizes() {
    let app = App::demo();
    for (w, h) in [(1, 1), (10, 3), (39, 9), (40, 10), (200, 60), (0, 0)] {
        assert!(draw_no_panic(&app, w.max(1), h.max(1)), "panic en {w}x{h}");
    }
}

#[test]
fn settings_survives_out_of_range_category_and_selection() {
    let mut app = App::demo();
    app.open_settings();
    app.agent.settings_cat = 99;
    app.agent.settings_sel = 999;
    assert!(draw_no_panic(&app, 100, 30), "categoría fuera de rango");
    app.handle_key(key(KeyCode::Enter));
}

#[test]
fn settings_enter_with_no_rows_does_not_panic() {
    let mut app = App::demo();
    app.open_settings();
    app.agent.settings_cat = 1;
    app.agent.settings_sel = 500;
    app.handle_key(key(KeyCode::Enter));
    assert!(draw_no_panic(&app, 80, 24));
}

#[test]
fn palette_without_matches_is_safe() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.composer = "/zzzz".into();
    app.agent.palette = true;
    assert_eq!(app.palette_items().len(), 0);
    app.handle_key(key(KeyCode::Enter));
    assert!(draw_no_panic(&app, 100, 30));
}

#[test]
fn slash_commands_with_missing_arguments_do_not_panic() {
    let mut app = App::demo();
    for cmd in [
        "/branch",
        "/export",
        "/new",
        "/model",
        "/settings",
        "/unknown",
    ] {
        app.run_slash(cmd);
        assert!(draw_no_panic(&app, 100, 30), "panic con {cmd}");
    }
}

#[test]
fn transcript_with_garbage_json_does_not_panic() {
    let mut app = App::demo();
    app.agent.workspace = Some(json!({"id": "ws"}));
    app.load_transcript();
    app.pump_daemon();
    assert!(draw_no_panic(&app, 100, 30));
}

#[test]
fn mouse_events_outside_the_screen_are_ignored() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    for (x, y) in [(0, 0), (65535, 65535), (200, 1), (1, 200)] {
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
    }
    assert!(draw_no_panic(&app, 100, 30));
}

#[test]
fn click_on_first_list_row_selects_index_zero() {
    use hli_tui::app::AgentTab;
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.agent_tab = AgentTab::Workspace;
    app.workspaces = (0..3)
        .map(|i| json!({"id": format!("ws{i}"), "name": format!("w{i}")}))
        .collect();
    app.agent.ws_sel = 2;
    app.transcript_top = 4;
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.agent.ws_sel, 0, "clic en la primera fila mal indexado");
}

#[test]
fn wheel_speed_parser_rejects_garbage() {
    for raw in ["0", "21", "-5", "abc", "", " ", "3.5", "1e3"] {
        assert_eq!(parse_wheel_speed(Some(raw)), 3, "valor {raw}");
    }
    assert_eq!(parse_wheel_speed(Some("1")), 1);
    assert_eq!(parse_wheel_speed(Some("20")), 20);
}

#[test]
fn viewport_scroll_saturates_at_extremes() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.observe_terminal(100, 30);
    app.viewport_scroll(isize::MIN);
    assert_eq!(app.agent.viewport.first_visible_line, 0);
    app.viewport_scroll(isize::MAX);
    assert_eq!(
        app.agent.viewport.first_visible_line,
        app.agent.viewport.max_scroll()
    );
}

#[test]
fn text_with_control_and_ansi_characters_is_safe() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    app.agent.push(
        hli_tui::app::AgentKind::Info,
        "\u{1b}[31mrojo\u{1b}[0m\u{0}\u{7}",
    );
    assert!(draw_no_panic(&app, 100, 30));
}

#[test]
fn thousand_activity_lines_keep_the_viewport_consistent() {
    let mut app = App::demo();
    app.mode = TuiMode::Agent;
    for i in 0..1000 {
        app.agent
            .push(hli_tui::app::AgentKind::Info, format!("linea {i}"));
    }
    app.observe_terminal(100, 30);
    assert!(app.agent.viewport.max_scroll() > 0);
    app.viewport_bottom();
    assert!(app.agent.viewport.follow_mode);
    assert!(draw_no_panic(&app, 100, 30));
}

#[test]
fn empty_workspace_and_thread_selection_is_safe() {
    let mut app = App::demo();
    app.workspaces.clear();
    app.threads.clear();
    app.select_workspace();
    app.select_thread();
    assert!(draw_no_panic(&app, 100, 30));
}
