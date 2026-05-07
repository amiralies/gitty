use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Mode, Pane, Search};
use crate::git::{Change, DiffText, Section};

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

    if app.show_help {
        draw_help(frame, frame.area(), app.is_review());
    }
}

fn draw_help(frame: &mut Frame, area: Rect, review: bool) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Keybindings",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from("  h / l (←/→)   focus status / diff pane"),
        Line::from("  j / k (↓/↑)   move down / up (or scroll diff when focused)"),
        Line::from("  gg / G        top / bottom of diff (when diff focused)"),
        Line::from("  Ctrl-d / -u   half-page scroll diff"),
    ];
    if !review {
        lines.push(Line::from("  s             stage selected"));
        lines.push(Line::from("  u             unstage selected"));
        lines.push(Line::from("  X             discard (confirm with y)"));
    }
    lines.push(Line::from("  / n N         search file names / next / prev"));
    lines.push(Line::from("  Esc / \\       clear search highlight"));
    if review {
        lines.push(Line::from("  Space         toggle reviewed"));
    }
    if !review {
        lines.push(Line::from("  e             edit selected file in $EDITOR"));
    }
    lines.push(Line::from("  r             refresh"));
    lines.push(Line::from("  ?             toggle this help"));
    lines.push(Line::from("  q             quit"));
    lines.push(Line::from(""));
    if review {
        lines.push(Line::from(Span::styled(
            "review mode is read-only",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(Span::styled(
        "? / Esc / q to close",
        Style::default().fg(Color::DarkGray),
    )));

    let popup = centered_rect(60, lines.len() as u16 + 2, area);
    let para = Paragraph::new(lines).block(
        Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn draw_diff(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let title = match app.current() {
        Some(f) => format!("Diff — {}", f.path.display()),
        None => "Diff".to_string(),
    };
    let scroll = app.diff_scroll;
    let focused = app.focus == Pane::Diff;
    let body: Text = match app.current_diff() {
        Some(DiffText::Highlighted(s)) => s
            .as_bytes()
            .into_text()
            .unwrap_or_else(|_| Text::from(s.clone())),
        Some(DiffText::Plain(s)) => Text::from(s.lines().map(diff_line).collect::<Vec<_>>()),
        None => Text::from("(no selection)"),
    };
    let para = Paragraph::new(body)
        .block(focus_block(title, focused))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, area);
}

fn focus_block<'a, T: Into<ratatui::text::Line<'a>>>(title: T, focused: bool) -> Block<'a> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style)
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
    let max_path = area.width.saturating_sub(6) as usize;

    let review = app.is_review();
    let max_path = if review {
        max_path.saturating_sub(4)
    } else {
        max_path
    };

    for (i, file) in app.files.iter().enumerate() {
        if last_section != Some(file.section) {
            let header = match file.section {
                Section::Staged => Some("Staged"),
                Section::Unstaged => Some("Changes"),
                Section::Review => None,
            };
            if let Some(h) = header {
                items.push(ListItem::new(Line::from(Span::styled(
                    h,
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Cyan),
                ))));
            }
            last_section = Some(file.section);
        }

        if i == app.selected {
            visual_index_of_selected = Some(items.len());
        }

        let reviewed = review && app.is_reviewed(&file.path);
        let is_selected = i == app.selected;
        let color = change_color(file.change);
        let display = match &file.old_path {
            Some(old) => format!("{} → {}", old.display(), file.path.display()),
            None => file.path.display().to_string(),
        };
        let path_text = truncate_path_left(&display, max_path);
        let path_style = if app.is_match(&file.path) {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else if reviewed {
            let mut s = Style::default().add_modifier(Modifier::CROSSED_OUT);
            if !is_selected {
                s = s.fg(Color::DarkGray);
            }
            s
        } else {
            Style::default()
        };
        let code_style = if reviewed && !is_selected {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(color)
        };
        let mut spans = vec![Span::raw("  ")];
        if review {
            let (mark, mark_style) = if reviewed {
                ("[x] ", Style::default().fg(Color::Green))
            } else if is_selected {
                ("[ ] ", Style::default())
            } else {
                ("[ ] ", Style::default().fg(Color::DarkGray))
            };
            spans.push(Span::styled(mark, mark_style));
        }
        spans.push(Span::styled(format!("{} ", file.change.code()), code_style));
        spans.push(Span::styled(path_text, path_style));
        items.push(ListItem::new(Line::from(spans)));
    }

    let focused = app.focus == Pane::Status;
    let title = if app.is_review() { "Review" } else { "Changes" };
    let list = List::new(items)
        .block(focus_block(title, focused))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(visual_index_of_selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn truncate_path_left(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let take = max.saturating_sub(1);
    let skip = count - take;
    let tail: String = s.chars().skip(skip).collect();
    format!("…{tail}")
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
    let text = match &app.search {
        Search::Input(q) => format!("/{q}▏"),
        Search::Active {
            query,
            order,
            cursor,
            ..
        } => format!(
            " /{query}  [{}/{}]  n next  N prev  Esc clear",
            cursor + 1,
            order.len()
        ),
        Search::Off => {
            let trailer = match &app.status_msg {
                Some(m) => format!("  {m}"),
                None => "  ? help  q quit".into(),
            };
            match &app.mode {
                Mode::Review { spec, .. } => format!(
                    " review · {}  {}/{} reviewed{}",
                    spec,
                    app.reviewed_count(),
                    app.files.len(),
                    trailer,
                ),
                Mode::Status => format!(
                    " {}  {} staged  {} unstaged{}",
                    app.branch_name(),
                    app.staged_count(),
                    app.unstaged_count(),
                    trailer,
                ),
            }
        }
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}
