use std::collections::HashSet;
use std::path::Path;
use serde::Deserialize;

/// Workspace-level configuration loaded from `.q-ls.json` at the repo root.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Identifiers to skip when emitting "unresolved reference" warnings.
    /// Useful for names assigned dynamically (e.g. via `@[.proc;...]`).
    pub suppress_unresolved: HashSet<String>,
}

impl Config {
    pub fn load(workspace_root: &Path) -> Self {
        let path = workspace_root.join(".q-ls.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }
}
