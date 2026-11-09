use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use hli_tui::app::{App, TuiMode};
use hli_tui::terminal::{MouseGuard, mouse_enabled_from_env};
use hli_tui::ui;

fn main() -> io::Result<()> {
    hli_tui::install_panic_hook();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = TuiMode::from_arg(args.first().map(String::as_str));
    let root = flag(&args, "--root");

    let mut terminal = ratatui::init();
    // RAII: la captura de ratón se libera al salir por cualquier vía. Con
    // HLI_DISABLE_MOUSE=1 Windows Terminal conserva su selección nativa.
    let guard = MouseGuard::new(mouse_enabled_from_env())?;
    let mut app = App::new();
    if mode == TuiMode::Agent {
        app.begin_agent(root.map(std::path::PathBuf::from));
        if args.iter().any(|a| a == "--resume") {
            app.open_resume();
        }
    }
    let res = run(&mut terminal, &mut app);
    drop(guard);
    ratatui::restore();
    res
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        let size = terminal.size()?;
        app.observe_terminal(size.width, size.height);
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) => {
                    if app.handle_key(k) {
                        break;
                    }
                }
                Event::Mouse(m) => {
                    app.handle_mouse(m);
                }
                Event::Resize(_, _) => {
                    // El siguiente ciclo recalcula el viewport con el nuevo
                    // tamaño y respeta la posición o el follow.
                }
                _ => {}
            }
        }
        app.tick();
    }
    Ok(())
}
