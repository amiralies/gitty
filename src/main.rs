mod app;
mod editor;
mod git;
mod ui;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let res = run(&mut terminal);
    restore_terminal(&mut terminal)?;
    res
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Tui) -> Result<()> {
    let repo = git::open_repo()?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repo"))?
        .to_path_buf();
    let mut app = App::new(repo)?;

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if let Some(path) = app.edit_request.take() {
            restore_terminal(terminal)?;
            let edit_result = editor::edit_file(&workdir, &path);
            *terminal = setup_terminal()?;
            terminal.clear()?;
            if let Err(e) = edit_result {
                app.status_msg = Some(format!("editor failed: {e}"));
            } else {
                app.refresh()?;
            }
            continue;
        }

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            handle_key(&mut app, key)?;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if app.show_help {
        app.show_help = false;
        return Ok(());
    }
    if app.pending.is_some() {
        match key.code {
            KeyCode::Char('y') => app.confirm_yes()?,
            _ => app.confirm_no(),
        }
        return Ok(());
    }
    app.status_msg = None;
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('d') if ctrl => app.scroll_diff_down(10),
        KeyCode::Char('u') if ctrl => app.scroll_diff_up(10),
        KeyCode::Char('e') | KeyCode::Char('n') if ctrl => app.scroll_diff_down(1),
        KeyCode::Char('y') | KeyCode::Char('p') if ctrl => app.scroll_diff_up(1),
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('g') => app.move_top(),
        KeyCode::Char('G') => app.move_bottom(),
        KeyCode::Char('s') => app.stage_selected()?,
        KeyCode::Char('u') if !ctrl => app.unstage_selected()?,
        KeyCode::Char('X') => app.request_discard(),
        KeyCode::Char('e') if !ctrl => app.request_edit(),
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char('r') => app.refresh()?,
        _ => {}
    }
    Ok(())
}
