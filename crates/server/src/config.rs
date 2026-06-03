use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Workspace-level configuration loaded from `.q-ls.json` at the repo root.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Identifiers to skip when emitting "unresolved reference" warnings.
    /// Useful for names assigned dynamically (e.g. via `@[.proc;...]`).
    pub suppress_unresolved: HashSet<String>,
}

impl Config {
    /// Load config and return a human-readable status line suitable for logging.
    pub fn load(workspace_root: &Path) -> (Self, String) {
        let path = workspace_root.join(".q-ls.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (
                Self::default(),
                format!("q-ls: no config file at {}", path.display()),
            );
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(cfg) => {
                let status = if cfg.suppress_unresolved.is_empty() {
                    format!(
                        "q-ls: loaded {} (suppress_unresolved: none)",
                        path.display()
                    )
                } else {
                    let mut names: Vec<&str> =
                        cfg.suppress_unresolved.iter().map(String::as_str).collect();
                    names.sort_unstable();
                    format!(
                        "q-ls: loaded {} (suppress_unresolved: {})",
                        path.display(),
                        names.join(", ")
                    )
                };
                (cfg, status)
            }
            Err(e) => (
                Self::default(),
                format!(
                    "q-ls: failed to parse {}: {e} — using defaults",
                    path.display()
                ),
            ),
        }
    }
}
