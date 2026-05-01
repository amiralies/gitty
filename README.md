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
