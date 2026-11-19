//! Quick tools of the backups repo (`python -I tools/<name>.py`, cwd = backups) with output into the window.

use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};

use dbl::error::{Error, Result};

use crate::winx::CREATE_NO_WINDOW;

pub const TOOLS: &[&str] = &["compare_heads", "probe_timeline", "auth_e2e"];

/// Splits like a shell would for simple cases: spaces separate, "double quotes" group.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

pub fn spawn(python: &str, backups: &Path, name: &str, args: &str) -> Result<tokio::process::Child> {
    if !TOOLS.contains(&name) {
        return Err(Error::new("unknown_tool", name));
    }
    let script = Path::new("tools").join(format!("{name}.py"));
    if !backups.join(&script).is_file() {
        return Err(Error::new("not_found", backups.join(&script).display().to_string()));
    }
    let mut cmd = tokio::process::Command::new(python);
    cmd.arg("-I")
        .arg(&script)
        .args(split_args(args))
        .current_dir(backups)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true);
    cmd.spawn().map_err(|e| Error::new("spawn", format!("{python}: {e}")))
}

/// Reads stdout and stderr of a tool line by line into `sink` until both close.
pub async fn pump(child: &mut tokio::process::Child, sink: impl Fn(String) + Send + Sync + 'static + Clone) {
    let mut tasks = Vec::new();
    if let Some(out) = child.stdout.take() {
        let s = sink.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                s(l);
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let s = sink.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                s(l);
            }
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn split() {
        assert_eq!(split_args(r#"--since 05:30  --grep "a b""#), vec!["--since", "05:30", "--grep", "a b"]);
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args(r#"--x """#), vec!["--x", ""]);
    }
}
