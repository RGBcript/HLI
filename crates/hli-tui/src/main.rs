use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use hli_tui::app::App;
use hli_tui::ui;

fn main() -> io::Result<()> {
    hli_tui::install_panic_hook();
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let res = run(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(k) = event::read()?
            && app.handle_key(k)
        {
            break;
        }
        app.tick();
    }
    Ok(())
}
