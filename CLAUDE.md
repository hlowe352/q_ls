# Code Style

- All Rust code must be formatted with `cargo fmt`. Run before committing.
- All Rust code must be clean under `cargo clippy --workspace -- -W clippy::pedantic`. Fix warnings rather than suppressing them unless there is a documented reason.
