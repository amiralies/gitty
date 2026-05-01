use std::path::PathBuf;

use anyhow::Result;
use git2::{Repository, Status, StatusOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Staged,
    Unstaged,
}

#[derive(Debug, Clone, Copy)]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
    Typechange,
    Untracked,
    Conflicted,
}

impl Change {
    pub fn code(self) -> char {
        match self {
            Change::Added => 'A',
            Change::Modified => 'M',
            Change::Deleted => 'D',
            Change::Renamed => 'R',
            Change::Typechange => 'T',
            Change::Untracked => '?',
            Change::Conflicted => 'U',
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub change: Change,
    pub section: Section,
}

pub fn open_repo() -> Result<Repository> {
    Ok(Repository::discover(".")?)
}

pub fn load_status(repo: &Repository) -> Result<Vec<FileEntry>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let s = entry.status();
        let path = match entry.path() {
            Some(p) => PathBuf::from(p),
            None => continue,
        };

        if s.contains(Status::CONFLICTED) {
            entries.push(FileEntry {
                path: path.clone(),
                change: Change::Conflicted,
                section: Section::Unstaged,
            });
            continue;
        }

        if let Some(c) = index_change(s) {
            entries.push(FileEntry {
                path: path.clone(),
                change: c,
                section: Section::Staged,
            });
        }

        if let Some(c) = workdir_change(s) {
            entries.push(FileEntry {
                path,
                change: c,
                section: Section::Unstaged,
            });
        }
    }

    entries.sort_by(|a, b| {
        (a.section as u8, &a.path).cmp(&(b.section as u8, &b.path))
    });
    Ok(entries)
}

fn index_change(s: Status) -> Option<Change> {
    if s.contains(Status::INDEX_NEW) {
        Some(Change::Added)
    } else if s.contains(Status::INDEX_MODIFIED) {
        Some(Change::Modified)
    } else if s.contains(Status::INDEX_DELETED) {
        Some(Change::Deleted)
    } else if s.contains(Status::INDEX_RENAMED) {
        Some(Change::Renamed)
    } else if s.contains(Status::INDEX_TYPECHANGE) {
        Some(Change::Typechange)
    } else {
        None
    }
}

fn workdir_change(s: Status) -> Option<Change> {
    if s.contains(Status::WT_NEW) {
        Some(Change::Untracked)
    } else if s.contains(Status::WT_MODIFIED) {
        Some(Change::Modified)
    } else if s.contains(Status::WT_DELETED) {
        Some(Change::Deleted)
    } else if s.contains(Status::WT_RENAMED) {
        Some(Change::Renamed)
    } else if s.contains(Status::WT_TYPECHANGE) {
        Some(Change::Typechange)
    } else {
        None
    }
}
