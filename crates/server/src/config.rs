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
                let mut names: Vec<&str> =
                    cfg.suppress_unresolved.iter().map(String::as_str).collect();
                names.sort_unstable();
                let names = if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                };
                let status = format!(
                    "q-ls: loaded {} (suppress_unresolved: {names})",
                    path.display()
                );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_valid_and_invalid() {
        let dir = std::env::temp_dir().join(format!("q-ls-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".q-ls.json");
        let _ = std::fs::remove_file(&file);

        let (cfg, status) = Config::load(&dir);
        assert!(cfg.suppress_unresolved.is_empty() && status.contains("no config"));

        std::fs::write(&file, r#"{"suppress_unresolved": ["b", "a"]}"#).unwrap();
        let (cfg, status) = Config::load(&dir);
        assert_eq!(cfg.suppress_unresolved.len(), 2);
        assert!(status.ends_with("(suppress_unresolved: a, b)"), "{status}");

        std::fs::write(&file, "{}").unwrap();
        assert!(
            Config::load(&dir)
                .1
                .ends_with("(suppress_unresolved: none)")
        );

        std::fs::write(&file, "not json").unwrap();
        let (cfg, status) = Config::load(&dir);
        assert!(cfg.suppress_unresolved.is_empty() && status.contains("failed to parse"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
