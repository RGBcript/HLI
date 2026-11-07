use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use hli_tui::app::App;
use hli_tui::ui;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let res = run(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Tab => app.next_tab(),
                    KeyCode::BackTab => app.prev_tab(),
                    KeyCode::Char(c @ '1'..='6') => app.tab = (c as usize) - ('1' as usize),
                    KeyCode::Enter => app.run_command(),
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Up => {
                        if app.sel_run > 0 {
                            app.sel_run -= 1;
                            app.refresh_runs();
                        }
                    }
                    KeyCode::Down => {
                        if app.sel_run + 1 < app.runs.len() {
                            app.sel_run += 1;
                            app.refresh_runs();
                        }
                    }
                    KeyCode::Char(c) => app.input.push(c),
                    _ => {}
                }
            }
        }
        app.tick();
    }
    Ok(())
}
