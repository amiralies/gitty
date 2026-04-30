use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders};

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
    loop {
        terminal.draw(draw)?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.code == KeyCode::Char('q')
        {
            return Ok(());
        }
    }
}

fn draw(frame: &mut ratatui::Frame) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[0]);

    frame.render_widget(Block::default().title("Changes").borders(Borders::ALL), top[0]);
    frame.render_widget(Block::default().title("Diff").borders(Borders::ALL), top[1]);
    frame.render_widget(
        Block::default().title("Commit").borders(Borders::ALL),
        outer[1],
    );
    frame.render_widget(Block::default().title("NORMAL  ? help  q quit"), outer[2]);
}
