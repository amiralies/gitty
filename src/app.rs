use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use git2::Repository;

use crate::git::{FileEntry, Section, diff_for, load_status, stage, unstage};

pub struct App {
    pub repo: Repository,
    pub files: Vec<FileEntry>,
    pub selected: usize,
    pub should_quit: bool,
    pub diff_cache: HashMap<(PathBuf, Section), String>,
    pub diff_scroll: u16,
    pub status_msg: Option<String>,
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
        })
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
