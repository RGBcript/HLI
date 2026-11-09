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

    let classic = args.iter().any(|a| a == "--classic")
        || std::env::var("HLI_DISABLE_ALTERNATE_SCREEN")
            .map(|v| v != "0")
            .unwrap_or(false)
        // Modo lector de pantalla: texto plano, sin alternate screen ni cajas.
        || matches!(
            std::env::var("HLI_SCREEN_READER").as_deref(),
            Ok("1") | Ok("true")
        );
    if classic {
        return hli_tui::classic::run(root.map(std::path::PathBuf::from));
    }

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
                    // Volcado al scrollback y export: se sale del alternate
                    // screen, se escribe y se vuelve con Enter.
                    if app.transcript_dump || app.transcript_export.is_some() {
                        let text = app.transcript_text();
                        let export = app.transcript_export.take();
                        let dump = std::mem::take(&mut app.transcript_dump);
                        ratatui::restore();
                        if let Some(path) = export {
                            let _ = std::fs::write(&path, &text);
                            println!("Transcript exportado: {}", path.display());
                        }
                        if dump {
                            println!("{text}");
                        }
                        println!("--- Enter para volver a la interfaz ---");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                        *terminal = ratatui::init();
                        if app.mouse_enabled {
                            let _ = crossterm::execute!(
                                std::io::stdout(),
                                crossterm::event::EnableMouseCapture
                            );
                        }
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
