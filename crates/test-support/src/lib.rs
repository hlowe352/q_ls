//! Running a real q for the oracle tests (`q_oracle`, `binding_oracle`).
//! Tests are skipped when no q binary is found.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The q binary: `$Q_BIN`, else `~/.kx/bin/q`, if it exists.
#[must_use]
pub fn q_bin() -> Option<PathBuf> {
    std::env::var_os("Q_BIN")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".kx/bin/q")))
        .filter(|p| p.is_file())
}

/// Run `script` as a q script and return its stdout.
///
/// # Errors
/// If q stops before the end of the script (a q error), with its output.
///
/// # Panics
/// If the temporary script can't be written or q can't be started.
pub fn run_q(q: &Path, script: &str) -> Result<String, String> {
    const DONE: &str = "q-test-support: done";
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let run = RUN.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("q_test_{}_{run}.q", std::process::id()));
    std::fs::write(&path, format!("{script}\n-1 \"{DONE}\";\nexit 0\n")).unwrap();
    let out = Command::new(q)
        .arg(&path)
        .arg("-q")
        .stdin(Stdio::null())
        .output()
        .expect("run q");
    std::fs::remove_file(&path).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    match stdout.strip_suffix(&format!("{DONE}\n")) {
        Some(body) => Ok(body.to_string()),
        None => Err(format!("{stdout}{}", String::from_utf8_lossy(&out.stderr))),
    }
}
