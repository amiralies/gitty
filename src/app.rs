use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use git2::Repository;

use crate::git::{FileEntry, Section, diff_for, discard, load_status, stage, unstage};

#[derive(Debug, Clone)]
pub enum Confirm {
    Discard { path: PathBuf, change: crate::git::Change },
}

pub struct App {
    pub repo: Repository,
    pub files: Vec<FileEntry>,
    pub selected: usize,
    pub should_quit: bool,
    pub diff_cache: HashMap<(PathBuf, Section), String>,
    pub diff_scroll: u16,
    pub status_msg: Option<String>,
    pub pending: Option<Confirm>,
    pub show_help: bool,
    pub edit_request: Option<PathBuf>,
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
        })
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
        self.status_msg = Some(format!(
            "discard {}? (y/n)",
            file.path.display()
        ));
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
        Ok(())
    }

    pub fn refresh_keep_selection(&mut self) -> Result<()> {
        let prev = self.files.get(self.selected).map(|f| (f.path.clone(), f.section));
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

    pub fn move_top(&mut self) {
        self.selected = 0;
        self.diff_scroll = 0;
    }

    pub fn move_bottom(&mut self) {
        self.selected = self.files.len().saturating_sub(1);
        self.diff_scroll = 0;
    }

    pub fn current(&self) -> Option<&FileEntry> {
        self.files.get(self.selected)
    }

    pub fn current_diff(&mut self) -> Option<&str> {
        let file = self.files.get(self.selected)?;
        let key = (file.path.clone(), file.section);
        if !self.diff_cache.contains_key(&key) {
            let text = diff_for(&self.repo, &file.path, file.section).unwrap_or_else(|e| {
                format!("error computing diff: {e}")
            });
            self.diff_cache.insert(key.clone(), text);
        }
        self.diff_cache.get(&key).map(String::as_str)
    }

    pub fn scroll_diff_down(&mut self, n: u16) {
        self.diff_scroll = self.diff_scroll.saturating_add(n);
    }

    pub fn scroll_diff_up(&mut self, n: u16) {
        self.diff_scroll = self.diff_scroll.saturating_sub(n);
    }

    pub fn stage_selected(&mut self) -> Result<()> {
        let Some(file) = self.files.get(self.selected).cloned() else {
            return Ok(());
        };
        if file.section != Section::Unstaged {
            return Ok(());
        }
        stage(&self.repo, &file.path, file.change)?;
        self.refresh_preserving(&file.path, Section::Staged)
    }

    pub fn unstage_selected(&mut self) -> Result<()> {
        let Some(file) = self.files.get(self.selected).cloned() else {
            return Ok(());
        };
        if file.section != Section::Staged {
            return Ok(());
        }
        unstage(&self.repo, &file.path)?;
        self.refresh_preserving(&file.path, Section::Unstaged)
    }

    fn refresh_preserving(&mut self, path: &std::path::Path, prefer: Section) -> Result<()> {
        self.files = load_status(&self.repo)?;
        self.diff_cache.clear();
        self.diff_scroll = 0;
        let new_idx = self
            .files
            .iter()
            .position(|f| f.path == path && f.section == prefer)
            .or_else(|| self.files.iter().position(|f| f.path == path))
            .unwrap_or(0);
        self.selected = new_idx.min(self.files.len().saturating_sub(1));
        Ok(())
    }

    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|f| f.section == Section::Staged).count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.files.iter().filter(|f| f.section == Section::Unstaged).count()
    }

    pub fn branch_name(&self) -> String {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "HEAD".into())
    }
}
