# gitty

A minimal terminal UI for reading git diffs.

Two panes: status on the left, diff on the right. Stage, unstage, discard, and edit files without leaving the terminal.

## Install

```sh
./install.sh
```

Requires a Rust toolchain. Installs the `gitty` binary via `cargo install --path .`.

## Usage

Run `gitty` inside any git repository.

### Review mode

Pass a revspec to view diffs between two revisions instead of the working tree:

```sh
gitty master...HEAD       # everything on this branch since merge base
gitty HEAD~3..HEAD        # last three commits
gitty <commit-sha>        # a single commit vs its first parent
```

Review mode is read-only — `s`, `u`, `X`, `e` are disabled. Renames are detected and shown as `R old → new`. Press `Space` to toggle a file as reviewed (in-memory only; not persisted across runs); the file list shows `[x]`/`[ ]` checkboxes and the status bar tracks `reviewed/total`.

### Keys

| Key | Action |
| --- | --- |
| `q` | quit |
| `h` / `l` | focus status / diff pane |
| `j` / `k` | move down / up |
| `Ctrl-d` / `Ctrl-u` | scroll diff half-page |
| `g g` / `G` | top / bottom of diff |
| `s` | stage selected |
| `u` | unstage selected |
| `X` | discard (with confirm) |
| `e` | edit file in `$EDITOR` |
| `/` `n` `N` | search / next / prev |
| `r` | refresh |
| `?` | help |
