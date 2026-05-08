mod app;
mod editor;
mod git;
mod ui;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::{App, Pane, Search};

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<()> {
    let revspec = match parse_args() {
        Ok(spec) => spec,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let repo = git::open_repo()?;
    let app = match revspec {
        Some(spec) => App::new_review(repo, spec)?,
        None => App::new(repo)?,
    };
    let mut terminal = setup_terminal()?;
    let res = run(&mut terminal, app);
    restore_terminal(&mut terminal)?;
    res
}

fn parse_args() -> Result<Option<String>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => Ok(None),
        [s] if !s.starts_with('-') => Ok(Some(s.clone())),
        _ => Err("Usage: gitty [<revspec>]".into()),
    }
}

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Tui, mut app: App) -> Result<()> {
    let workdir = app
        .repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repo"))?
        .to_path_buf();

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
                app.refresh_keep_selection()?;
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
    if matches!(app.search, Search::Input(_)) {
        match key.code {
            KeyCode::Esc => app.search_cancel(),
            KeyCode::Enter => app.search_submit(),
            KeyCode::Backspace => app.search_input_pop(),
            KeyCode::Char(c) => app.search_input_push(c),
            _ => {}
        }
        return Ok(());
    }
    if app.show_help {
        if matches!(
            key.code,
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
        ) {
            app.show_help = false;
        }
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

    if matches!(key.code, KeyCode::Char('g')) && !ctrl {
        if app.pending_g {
            app.pending_g = false;
            if app.focus == Pane::Diff {
                app.scroll_diff_top();
            }
        } else {
            app.pending_g = true;
        }
        return Ok(());
    }
    app.pending_g = false;

    match key.code {
        KeyCode::Char('q') => app.request_quit(),
        KeyCode::Char('h') | KeyCode::Left if !ctrl => app.focus_status(),
        KeyCode::Char('l') | KeyCode::Right if !ctrl => app.focus_diff(),
        KeyCode::Char('d') if ctrl => app.scroll_diff_down(10),
        KeyCode::Char('u') if ctrl => app.scroll_diff_up(10),
        KeyCode::Char('j') | KeyCode::Down => match app.focus {
            Pane::Status => app.move_down(),
            Pane::Diff => app.scroll_diff_down(1),
        },
        KeyCode::Char('k') | KeyCode::Up => match app.focus {
            Pane::Status => app.move_up(),
            Pane::Diff => app.scroll_diff_up(1),
        },
        KeyCode::Char('G') if app.focus == Pane::Diff => app.scroll_diff_bottom(),
        KeyCode::Char('s') => app.stage_selected()?,
        KeyCode::Char('u') if !ctrl => app.unstage_selected()?,
        KeyCode::Char('X') => app.request_discard(),
        KeyCode::Char('e') if !ctrl => app.request_edit(),
        KeyCode::Char(' ') => app.toggle_reviewed(),
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char('/') => app.search_start(),
        KeyCode::Char('n') if !ctrl => app.search_next(),
        KeyCode::Char('N') => app.search_prev(),
        KeyCode::Esc | KeyCode::Char('\\') => app.search_cancel(),
        KeyCode::Char('r') => app.refresh()?,
        _ => {}
    }
    Ok(())
}
