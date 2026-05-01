use std::path::PathBuf;

use std::path::Path;

use anyhow::Result;
use git2::{Diff, DiffOptions, Repository, Status, StatusOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

pub fn stage(repo: &Repository, path: &Path, change: Change) -> Result<()> {
    let mut index = repo.index()?;
    if matches!(change, Change::Deleted) {
        index.remove_path(path)?;
    } else {
        index.add_path(path)?;
    }
    index.write()?;
    Ok(())
}

pub fn unstage(repo: &Repository, path: &Path) -> Result<()> {
    match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        Some(commit) => {
            repo.reset_default(Some(commit.as_object()), [path])?;
        }
        None => {
            let mut index = repo.index()?;
            index.remove_path(path)?;
            index.write()?;
        }
    }
    Ok(())
}

pub fn diff_for(repo: &Repository, path: &Path, section: Section) -> Result<String> {
    let mut opts = DiffOptions::new();
    opts.pathspec(path).include_untracked(true).recurse_untracked_dirs(true);

    let diff = match section {
        Section::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
        }
        Section::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts))?,
    };

    Ok(render_diff(&diff))
}

fn render_diff(diff: &Diff) -> String {
    let mut out = String::new();
    let _ = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if matches!(origin, '+' | '-' | ' ') {
            out.push(origin);
        }
        if let Ok(s) = std::str::from_utf8(line.content()) {
            out.push_str(s);
        }
        true
    });
    out
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
