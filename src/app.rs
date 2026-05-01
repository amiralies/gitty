use anyhow::Result;
use git2::Repository;

use crate::git::{FileEntry, Section, load_status};

pub struct App {
    pub repo: Repository,
    pub files: Vec<FileEntry>,
    pub selected: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(repo: Repository) -> Result<Self> {
        let files = load_status(&repo)?;
        Ok(Self {
            repo,
            files,
            selected: 0,
            should_quit: false,
        })
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.files = load_status(&self.repo)?;
        if self.selected >= self.files.len() {
            self.selected = self.files.len().saturating_sub(1);
        }
        Ok(())
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.files.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_top(&mut self) {
        self.selected = 0;
    }

    pub fn move_bottom(&mut self) {
        self.selected = self.files.len().saturating_sub(1);
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
