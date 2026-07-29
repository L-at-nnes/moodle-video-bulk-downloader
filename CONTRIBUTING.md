# Contributing

Thanks for your interest in improving this project.

## Development setup

1. Install the [Rust toolchain](https://rustup.rs) (stable).
2. Install the Tauri CLI: `cargo install tauri-cli --version "^2"`
3. Build everything: `cargo build --workspace`

Run the CLI: `cargo run -p mvbd-cli -- --help`
Run the GUI in dev mode: `cargo tauri dev --config crates/gui/src-tauri/tauri.conf.json` (or `cd crates/gui/src-tauri && cargo tauri dev`)

## Coding guidelines

- Keep changes focused and minimal.
- Business logic lives in `crates/core`; the CLI and GUI crates are thin front ends over it — don't duplicate logic between them.
- Preserve existing CLI/GUI behavior unless intentionally changed.
- Run `cargo fmt` and `cargo clippy --workspace` before opening a PR.
- Use clear names and readable logs.

## Pull requests

- Open a PR with a clear description of what changed and why.
- Include reproduction steps for bug fixes.
- If possible, include a short test command (`cargo test --workspace`).

## Issues

When opening an issue, please include:

- Command used (CLI) or steps taken (GUI)
- Full error output
- Whether cookies were valid at the time
