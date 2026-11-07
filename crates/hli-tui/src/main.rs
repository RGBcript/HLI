use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use hli_tui::app::{App, FocusArea};
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
                let in_command = app.focus == FocusArea::CommandBar;
                match k.code {
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('q') => {
                        if app.help { app.close_help(); }
                        else if in_command { app.input.push('q'); }
                        else { break; }
                    }
                    KeyCode::Char('?') => { app.help = !app.help; }
                    KeyCode::Esc => {
                        if app.help { app.close_help(); }
                        else if in_command { app.cancel_command(); }
                        else { app.filter = None; }
                    }
                    KeyCode::Tab => { if !in_command { app.cycle_focus(); } }
                    KeyCode::BackTab => { if !in_command { app.cycle_focus_back(); } }
                    KeyCode::Char(':') => { app.start_command(); }
                    KeyCode::Char('/') => {
                        app.input = "/".into();
                        app.start_command();
                    }
                    KeyCode::Char(c @ '1'..='6') if !in_command => {
                        app.tab = (c as usize) - ('1' as usize);
                        app.focus = FocusArea::DetailTabs;
                    }
                    KeyCode::Enter => {
                        if in_command { app.run_command(); }
                        else { app.focus = FocusArea::Inspector; }
                    }
                    KeyCode::Backspace => { if in_command { app.input.pop(); } }
                    KeyCode::Up => match app.focus {
                        FocusArea::Navigation => app.nav_index = app.nav_index.saturating_sub(1),
                        FocusArea::Collection => app.collection_index = app.collection_index.saturating_sub(1),
                        FocusArea::CommandBar => app.history_up(),
                        _ => app.scroll_up(),
                    },
                    KeyCode::Down => match app.focus {
                        FocusArea::Navigation => app.nav_index += 1,
                        FocusArea::Collection => {
                            if app.collection_index + 1 < app.runs.len() { app.collection_index += 1; }
                        }
                        FocusArea::CommandBar => app.history_down(),
                        _ => app.scroll_down(app.events.len()),
                    },
                    KeyCode::Left => app.prev_tab(),
                    KeyCode::Right => app.next_tab(),
                    KeyCode::Char(c) => { if in_command { app.input.push(c); } }
                    _ => {}
                }
            }
        }
        app.tick();
    }
    Ok(())
}
