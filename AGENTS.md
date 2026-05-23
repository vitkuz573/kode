# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace for a CLI + TUI coding assistant.

- `src/main.rs`: CLI entry point (`kode`) and subcommands.
- `src/tui_runner.rs`: interactive event loop and runtime wiring.
- `src/bin/mock_provider.rs`: local OpenAI/Anthropic-compatible mock server.
- `crates/kode-core`: config, types, sessions, cost/context logic.
- `crates/kode-llm`: provider clients, streaming, model routing.
- `crates/kode-agent`: tool registry and agent loop.
- `crates/kode-tui`: ratatui UI, input handling, themes, markdown rendering.
- `tests/mock_e2e.rs`: integration tests for mock server + CLI behavior.

## Build, Test, and Development Commands
- `cargo check`: fast compile validation across workspace.
- `cargo build`: build debug binaries (`target/debug/kode`).
- `cargo test`: run all tests.
- `cargo test --test mock_e2e`: run mock-provider integration checks.
- `cargo run --bin mock_provider`: start local mock server on `127.0.0.1:8787`.
- `cargo run -- --prompt "hello"`: run one-shot CLI prompt.

## Coding Style & Naming Conventions
- Follow idiomatic Rust (`rustfmt`-compatible style, 4-space indentation).
- Types/traits: `PascalCase`; functions/modules/files: `snake_case`; constants: `SCREAMING_SNAKE_CASE`.
- Prefer small focused functions; keep UI strings and keybind behavior consistent across views.
- Use `anyhow::Result` + contextual errors for fallible flows.

## Testing Guidelines
- Use `cargo test` before opening a PR.
- Add integration tests for user-visible behavior changes (CLI output, exit codes, protocol handling).
- Keep test names descriptive and behavior-oriented, e.g. `kode_returns_non_zero_on_auth_error`.
- Mock-related tests may need loopback socket bind permission (`127.0.0.1`).

## Commit & Pull Request Guidelines
- Commit messages are imperative and concise (e.g., `Stabilize TUI UX, persist theme...`).
- Group related changes in one commit; avoid mixing refactors with unrelated fixes.
- PRs should include: summary, rationale, test evidence (`cargo check`, `cargo test`), and screenshots/GIFs for TUI-visible changes.
- Link issues when applicable and call out config or migration impacts explicitly.

## Security & Configuration Tips
- Do not commit real API keys. Use env-backed values in `~/.config/kode/config.toml` (e.g., `$OPENAI_API_KEY`).
- Use `mock_provider` for offline development and error-path testing without spending provider quota.
