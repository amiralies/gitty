use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::git::{Change, Section};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[0]);

    draw_changes(frame, app, top[0]);
    draw_diff(frame, app, top[1]);
    draw_status_bar(frame, app, outer[1]);
}

fn draw_diff(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let title = match app.current() {
        Some(f) => format!("Diff — {}", f.path.display()),
        None => "Diff".to_string(),
    };
    let scroll = app.diff_scroll;
    let text = app
        .current_diff()
        .map(String::from)
        .unwrap_or_else(|| "(no selection)".to_string());

    let lines: Vec<Line> = text.lines().map(diff_line).collect();
    let para = Paragraph::new(lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, area);
}

fn diff_line(line: &str) -> Line<'_> {
    let style = if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else if line.starts_with("+++") || line.starts_with("---") {
        Style::default().add_modifier(Modifier::BOLD)
    } else if line.starts_with('+') {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    Line::from(Span::styled(line.to_string(), style))
}

fn draw_changes(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut items: Vec<ListItem> = Vec::with_capacity(app.files.len() + 2);
    let mut last_section: Option<Section> = None;
    let mut visual_index_of_selected: Option<usize> = None;

    for (i, file) in app.files.iter().enumerate() {
        if last_section != Some(file.section) {
            let header = match file.section {
                Section::Staged => "Staged",
                Section::Unstaged => "Changes",
            };
            items.push(ListItem::new(Line::from(Span::styled(
                header,
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
            ))));
            last_section = Some(file.section);
        }

        if i == app.selected {
            visual_index_of_selected = Some(items.len());
        }

        let color = change_color(file.change);
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{} ", file.change.code()),
                Style::default().fg(color),
            ),
            Span::raw(file.path.display().to_string()),
        ]);
        items.push(ListItem::new(line));
    }

    let list = List::new(items)
        .block(Block::default().title("Changes").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(visual_index_of_selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn change_color(c: Change) -> Color {
    match c {
        Change::Added | Change::Untracked => Color::Green,
        Change::Modified | Change::Typechange => Color::Yellow,
        Change::Deleted => Color::Red,
        Change::Renamed => Color::Blue,
        Change::Conflicted => Color::Magenta,
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let trailer = match &app.status_msg {
        Some(m) => format!("  {m}"),
        None => "  ? help  q quit".into(),
    };
    let text = format!(
        " {}  {} staged  {} unstaged{}",
        app.branch_name(),
        app.staged_count(),
        app.unstaged_count(),
        trailer,
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}
