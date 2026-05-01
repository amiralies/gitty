use std::path::Path;
use std::process::Command;

use anyhow::Result;

pub fn edit_file(workdir: &Path, rel: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
    Command::new(editor).arg(workdir.join(rel)).status()?;
    Ok(())
}
