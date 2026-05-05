use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use git2::Repository;

use crate::git::{DiffText, FileEntry, Section, diff_for, discard, load_status, stage, unstage};

#[derive(Debug, Clone)]
pub enum Confirm {
    Discard {
        path: PathBuf,
        change: crate::git::Change,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Status,
    Diff,
}

#[derive(Debug, Clone)]
pub enum Search {
    Off,
    Input(String),
    Active {
        query: String,
        matches: std::collections::HashSet<PathBuf>,
        cursor: usize,
        order: Vec<usize>,
    },
}

pub struct App {
    pub repo: Repository,
    pub files: Vec<FileEntry>,
    pub selected: usize,
    pub should_quit: bool,
    pub diff_cache: HashMap<(PathBuf, Section), DiffText>,
    pub diff_scroll: u16,
    pub status_msg: Option<String>,
    pub pending: Option<Confirm>,
    pub show_help: bool,
    pub edit_request: Option<PathBuf>,
    pub focus: Pane,
    pub pending_g: bool,
    pub search: Search,
}

impl App {
    pub fn new(repo: Repository) -> Result<Self> {
        let files = load_status(&repo)?;
        Ok(Self {
            repo,
            files,
            selected: 0,
            should_quit: false,
            diff_cache: HashMap::new(),
            diff_scroll: 0,
            status_msg: None,
            pending: None,
            show_help: false,
            edit_request: None,
            focus: Pane::Status,
            pending_g: false,
            search: Search::Off,
        })
    }

    pub fn search_start(&mut self) {
        self.search = Search::Input(String::new());
    }

    pub fn search_input_push(&mut self, c: char) {
        if let Search::Input(s) = &mut self.search {
            s.push(c);
        }
    }

    pub fn search_input_pop(&mut self) {
        if let Search::Input(s) = &mut self.search {
            if s.pop().is_none() {
                self.search = Search::Off;
            }
        }
    }

    pub fn search_cancel(&mut self) {
        self.search = Search::Off;
    }

    pub fn search_submit(&mut self) {
        let query = match &self.search {
            Search::Input(s) if !s.is_empty() => s.clone(),
            _ => {
                self.search = Search::Off;
                return;
            }
        };
        let order = find_file_matches(&self.files, &query);
        if order.is_empty() {
            self.status_msg = Some(format!("no matches for {query:?}"));
            self.search = Search::Off;
            return;
        }
        let matches: std::collections::HashSet<PathBuf> =
            order.iter().map(|&i| self.files[i].path.clone()).collect();
        let next = next_match_from(&order, self.selected);
        self.selected = order[next];
        self.diff_scroll = 0;
        self.search = Search::Active {
            query,
            matches,
            cursor: next,
            order,
        };
    }

    pub fn search_next(&mut self) {
        if let Search::Active { order, cursor, .. } = &mut self.search {
            if order.is_empty() {
                return;
            }
            *cursor = (*cursor + 1) % order.len();
            self.selected = order[*cursor];
            self.diff_scroll = 0;
        }
    }

    pub fn search_prev(&mut self) {
        if let Search::Active { order, cursor, .. } = &mut self.search {
            if order.is_empty() {
                return;
            }
            *cursor = if *cursor == 0 {
                order.len() - 1
            } else {
                *cursor - 1
            };
            self.selected = order[*cursor];
            self.diff_scroll = 0;
        }
    }

    fn recompute_search(&mut self) {
        let query = match &self.search {
            Search::Active { query, .. } => query.clone(),
            _ => return,
        };
        let order = find_file_matches(&self.files, &query);
        if order.is_empty() {
            self.search = Search::Off;
            return;
        }
        let matches: std::collections::HashSet<PathBuf> =
            order.iter().map(|&i| self.files[i].path.clone()).collect();
        let cursor = order.iter().position(|&i| i == self.selected).unwrap_or(0);
        self.search = Search::Active {
            query,
            matches,
            cursor,
            order,
        };
    }

    pub fn is_match(&self, path: &std::path::Path) -> bool {
        matches!(&self.search, Search::Active { matches, .. } if matches.contains(path))
    }

    pub fn focus_status(&mut self) {
        self.focus = Pane::Status;
    }

    pub fn focus_diff(&mut self) {
        self.focus = Pane::Diff;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn request_edit(&mut self) {
        let Some(file) = self.files.get(self.selected) else {
            return;
        };
        if matches!(file.change, crate::git::Change::Deleted) {
            self.status_msg = Some("file is deleted, nothing to edit".into());
            return;
        }
        self.edit_request = Some(file.path.clone());
    }

    pub fn request_discard(&mut self) {
        let Some(file) = self.files.get(self.selected) else {
            return;
        };
        if file.section != Section::Unstaged {
            self.status_msg = Some("discard only applies to unstaged changes".into());
            return;
        }
        self.pending = Some(Confirm::Discard {
            path: file.path.clone(),
            change: file.change,
        });
        self.status_msg = Some(format!("discard {}? (y/n)", file.path.display()));
    }

    pub fn confirm_yes(&mut self) -> Result<()> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        match pending {
            Confirm::Discard { path, change } => {
                discard(&self.repo, &path, change)?;
                self.refresh()?;
                self.status_msg = Some("discarded".into());
            }
        }
        Ok(())
    }

    pub fn confirm_no(&mut self) {
        if self.pending.take().is_some() {
            self.status_msg = Some("cancelled".into());
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.files = load_status(&self.repo)?;
        if self.selected >= self.files.len() {
            self.selected = self.files.len().saturating_sub(1);
        }
        self.diff_cache.clear();
        self.diff_scroll = 0;
        self.recompute_search();
        Ok(())
    }

    pub fn refresh_keep_selection(&mut self) -> Result<()> {
        let prev = self
            .files
            .get(self.selected)
            .map(|f| (f.path.clone(), f.section));
        let prev_scroll = self.diff_scroll;
        self.files = load_status(&self.repo)?;
        self.diff_cache.clear();
        if let Some((path, section)) = prev {
            let new_idx = self
                .files
                .iter()
                .position(|f| f.path == path && f.section == section)
                .or_else(|| self.files.iter().position(|f| f.path == path));
            if let Some(i) = new_idx {
                self.selected = i;
                self.diff_scroll = prev_scroll;
                return Ok(());
            }
        }
        if self.selected >= self.files.len() {
            self.selected = self.files.len().saturating_sub(1);
        }
        self.diff_scroll = 0;
        Ok(())
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.files.len() {
            self.selected += 1;
            self.diff_scroll = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.diff_scroll = 0;
        }
    }

    pub fn current(&self) -> Option<&FileEntry> {
        self.files.get(self.selected)
    }

    pub fn current_diff(&mut self) -> Option<&DiffText> {
        let file = self.files.get(self.selected)?;
        let key = (file.path.clone(), file.section);
        if !self.diff_cache.contains_key(&key) {
            let text = diff_for(&self.repo, &file.path, file.section)
                .unwrap_or_else(|e| DiffText::Plain(format!("error computing diff: {e}")));
            self.diff_cache.insert(key.clone(), text);
        }
        self.diff_cache.get(&key)
    }

    pub fn scroll_diff_down(&mut self, n: u16) {
        self.diff_scroll = self.diff_scroll.saturating_add(n);
    }

    pub fn scroll_diff_up(&mut self, n: u16) {
        self.diff_scroll = self.diff_scroll.saturating_sub(n);
    }

    pub fn scroll_diff_top(&mut self) {
        self.diff_scroll = 0;
    }

    pub fn scroll_diff_bottom(&mut self) {
        let lines = self
            .current_diff()
            .map(|d| match d {
                DiffText::Highlighted(s) | DiffText::Plain(s) => s.lines().count(),
            })
            .unwrap_or(0);
        self.diff_scroll = lines.saturating_sub(1).min(u16::MAX as usize) as u16;
    }

    pub fn stage_selected(&mut self) -> Result<()> {
        let Some(file) = self.files.get(self.selected).cloned() else {
            return Ok(());
        };
        if file.section != Section::Unstaged {
            return Ok(());
        }
        let within = self.index_within_section();
        stage(&self.repo, &file.path, file.change)?;
        self.refresh_advancing(Section::Unstaged, within)
    }

    pub fn unstage_selected(&mut self) -> Result<()> {
        let Some(file) = self.files.get(self.selected).cloned() else {
            return Ok(());
        };
        if file.section != Section::Staged {
            return Ok(());
        }
        let within = self.index_within_section();
        unstage(&self.repo, &file.path)?;
        self.refresh_advancing(Section::Staged, within)
    }

    fn index_within_section(&self) -> usize {
        let Some(file) = self.files.get(self.selected) else {
            return 0;
        };
        self.files[..self.selected]
            .iter()
            .filter(|f| f.section == file.section)
            .count()
    }

    fn refresh_advancing(&mut self, from: Section, within: usize) -> Result<()> {
        self.files = load_status(&self.repo)?;
        self.diff_cache.clear();
        self.diff_scroll = 0;

        let in_section: Vec<usize> = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| (f.section == from).then_some(i))
            .collect();

        let new_idx = if !in_section.is_empty() {
            in_section[within.min(in_section.len() - 1)]
        } else {
            self.files
                .iter()
                .position(|f| f.section != from)
                .unwrap_or(0)
        };

        self.selected = new_idx.min(self.files.len().saturating_sub(1));
        Ok(())
    }

    pub fn staged_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.section == Section::Staged)
            .count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.section == Section::Unstaged)
            .count()
    }

    pub fn branch_name(&self) -> String {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "HEAD".into())
    }
}

fn find_file_matches(files: &[FileEntry], query: &str) -> Vec<usize> {
    let smart_lower = query.chars().all(|c| !c.is_uppercase());
    let needle = if smart_lower {
        query.to_lowercase()
    } else {
        query.to_string()
    };
    files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let path = f.path.display().to_string();
            let hay = if smart_lower {
                path.to_lowercase()
            } else {
                path
            };
            hay.contains(&needle).then_some(i)
        })
        .collect()
}

fn next_match_from(order: &[usize], current: usize) -> usize {
    order.iter().position(|&i| i >= current).unwrap_or(0)
}
