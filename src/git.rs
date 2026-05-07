use std::path::PathBuf;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;
use git2::{
    Delta, Diff, DiffFindOptions, DiffOptions, Oid, Repository, RevparseMode, Status,
    StatusOptions, build::CheckoutBuilder,
};

#[derive(Debug, Clone)]
pub enum DiffText {
    Highlighted(String),
    Plain(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Staged,
    Unstaged,
    Review,
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
    pub old_path: Option<PathBuf>,
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
                old_path: None,
                change: Change::Conflicted,
                section: Section::Unstaged,
            });
            continue;
        }

        if let Some(c) = index_change(s) {
            entries.push(FileEntry {
                path: path.clone(),
                old_path: None,
                change: c,
                section: Section::Staged,
            });
        }

        if let Some(c) = workdir_change(s) {
            entries.push(FileEntry {
                path,
                old_path: None,
                change: c,
                section: Section::Unstaged,
            });
        }
    }

    entries.sort_by(|a, b| (a.section as u8, &a.path).cmp(&(b.section as u8, &b.path)));
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

pub fn discard(repo: &Repository, path: &Path, change: Change) -> Result<()> {
    if matches!(change, Change::Untracked) {
        let abs = repo
            .workdir()
            .ok_or_else(|| anyhow::anyhow!("bare repo"))?
            .join(path);
        if abs.is_dir() {
            std::fs::remove_dir_all(&abs)?;
        } else if abs.exists() {
            std::fs::remove_file(&abs)?;
        }
        return Ok(());
    }
    let mut opts = CheckoutBuilder::new();
    opts.force().path(path);
    repo.checkout_index(None, Some(&mut opts))?;
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

pub fn diff_for(repo: &Repository, path: &Path, section: Section) -> Result<DiffText> {
    let mut opts = DiffOptions::new();
    opts.pathspec(path)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);

    let diff = match section {
        Section::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
        }
        Section::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts))?,
        Section::Review => return Err(anyhow::anyhow!("review section uses diff_for_review")),
    };

    let patch = render_diff(&diff);
    Ok(match highlight_with_delta(&patch) {
        Some(out) => DiffText::Highlighted(out),
        None => DiffText::Plain(patch),
    })
}

pub fn load_review(repo: &Repository, spec: &str) -> Result<(Oid, Oid, Vec<FileEntry>)> {
    let (left_tree_id, right_tree_id) = resolve_review_trees(repo, spec)?;
    let left_tree = repo.find_tree(left_tree_id)?;
    let right_tree = repo.find_tree(right_tree_id)?;

    let mut opts = DiffOptions::new();
    let mut diff =
        repo.diff_tree_to_tree(Some(&left_tree), Some(&right_tree), Some(&mut opts))?;
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true).copies(false);
    diff.find_similar(Some(&mut find_opts))?;

    let mut entries = Vec::new();
    for delta in diff.deltas() {
        let status = delta.status();
        let change = match status {
            Delta::Added => Change::Added,
            Delta::Deleted => Change::Deleted,
            Delta::Modified => Change::Modified,
            Delta::Renamed => Change::Renamed,
            Delta::Typechange => Change::Typechange,
            Delta::Copied => Change::Modified,
            _ => continue,
        };
        let new_path = delta.new_file().path().map(PathBuf::from);
        let old_path = delta.old_file().path().map(PathBuf::from);
        let path = match status {
            Delta::Deleted => match old_path.clone() {
                Some(p) => p,
                None => continue,
            },
            _ => match new_path.clone() {
                Some(p) => p,
                None => match old_path.clone() {
                    Some(p) => p,
                    None => continue,
                },
            },
        };
        let old_path = if matches!(status, Delta::Renamed | Delta::Copied) {
            old_path.filter(|p| Some(p) != new_path.as_ref())
        } else {
            None
        };
        entries.push(FileEntry {
            path,
            old_path,
            change,
            section: Section::Review,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((left_tree_id, right_tree_id, entries))
}

pub fn diff_for_review(
    repo: &Repository,
    file: &FileEntry,
    left: Oid,
    right: Oid,
) -> Result<DiffText> {
    let left_tree = repo.find_tree(left)?;
    let right_tree = repo.find_tree(right)?;

    let mut opts = DiffOptions::new();
    opts.pathspec(file.path.as_path());
    if let Some(old) = &file.old_path {
        opts.pathspec(old.as_path());
    }
    let mut diff =
        repo.diff_tree_to_tree(Some(&left_tree), Some(&right_tree), Some(&mut opts))?;
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true).copies(false);
    diff.find_similar(Some(&mut find_opts))?;

    let patch = render_diff(&diff);
    Ok(match highlight_with_delta(&patch) {
        Some(out) => DiffText::Highlighted(out),
        None => DiffText::Plain(patch),
    })
}

fn resolve_review_trees(repo: &Repository, spec: &str) -> Result<(Oid, Oid)> {
    let rs = repo.revparse(spec)?;
    let mode = rs.mode();
    if mode.contains(RevparseMode::MERGE_BASE) {
        let from = rs
            .from()
            .ok_or_else(|| anyhow::anyhow!("revspec {spec:?} missing left side"))?;
        let to = rs
            .to()
            .ok_or_else(|| anyhow::anyhow!("revspec {spec:?} missing right side"))?;
        let from_commit = from.peel_to_commit()?;
        let to_commit = to.peel_to_commit()?;
        let base = repo.merge_base(from_commit.id(), to_commit.id())?;
        let base_commit = repo.find_commit(base)?;
        Ok((base_commit.tree_id(), to_commit.tree_id()))
    } else if mode.contains(RevparseMode::RANGE) {
        let from = rs
            .from()
            .ok_or_else(|| anyhow::anyhow!("revspec {spec:?} missing left side"))?;
        let to = rs
            .to()
            .ok_or_else(|| anyhow::anyhow!("revspec {spec:?} missing right side"))?;
        Ok((from.peel_to_commit()?.tree_id(), to.peel_to_commit()?.tree_id()))
    } else {
        let only = rs
            .from()
            .ok_or_else(|| anyhow::anyhow!("could not resolve {spec:?}"))?;
        let commit = only.peel_to_commit()?;
        let right = commit.tree_id();
        let left = if commit.parent_count() > 0 {
            commit.parent(0)?.tree_id()
        } else {
            empty_tree_oid(repo)?
        };
        Ok((left, right))
    }
}

fn empty_tree_oid(repo: &Repository) -> Result<Oid> {
    Ok(repo.treebuilder(None)?.write()?)
}

fn highlight_with_delta(patch: &str) -> Option<String> {
    let mut child = Command::new("delta")
        .args([
            "--color-only",
            "--paging=never",
            "--features=",
            "--file-style=omit",
            "--hunk-header-style=cyan",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(patch.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
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
