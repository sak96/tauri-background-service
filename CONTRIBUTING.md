# Contributing to tauri-plugin-background-service

Thank you for your interest in contributing! This guide covers everything you
need to get started.

## Code of Conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md).
By participating, you are expected to uphold this code. Report issues via
[GitHub issues](https://github.com/dardourimohamed/tauri-background-service/issues/new)
or [med@dardouri.com](mailto:med@dardouri.com).

## Issue or Discussion?

- **Bug reports and feature requests** → [Open an issue](https://github.com/dardourimohamed/tauri-background-service/issues/new)
- **Questions, ideas, or general discussion** → [Start a discussion](https://github.com/dardourimohamed/tauri-background-service/discussions)

## Development Setup

### Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | 1.77.2+ | `rustup` recommended |
| Node.js | 20+ | For guest-js bindings |
| Android SDK | 34 | For Android platform builds |
| Xcode | 15+ | For iOS platform builds (macOS only) |

### Clone and Build

```bash
git clone https://github.com/dardourimohamed/tauri-background-service.git
cd tauri-background-service
```

## Build Commands

All commands run from the repository root unless noted.

| Command | Purpose |
|---------|---------|
| `cargo build` | Compile the plugin |
| `cargo test` | Run tests |
| `cargo clippy` | Lint with Clippy |
| `cargo doc --no-deps` | Build Rust documentation (run from `tauri-plugin-background-service/`) |
| `cd tauri-plugin-background-service/guest-js && npm run build` | Build JavaScript bindings |

### Quick Verification

After making changes, run the full check:

```bash
cargo build && cargo test && cargo clippy
cd tauri-plugin-background-service/guest-js && npm run build
```

## Code Style

- **Rust:** Follow standard `rustfmt` formatting. Run `cargo fmt` before committing.
- **Public enums:** Always use `#[non_exhaustive]` to allow future expansion.
- **Async traits:** Use the `async_trait` macro for trait definitions.
- **Error handling:** Use `ServiceError` variants; do not introduce new error types without discussion.
- **JavaScript:** Follow existing patterns in `guest-js/index.ts`.

## Branch Strategy

1. Fork the repository.
2. Create a feature branch from `main`:
   ```bash
   git checkout -b feature/my-change
   ```
3. Make your changes with clear, focused commits.
4. Push to your fork and open a pull request against `main`.

## Pull Request Process

1. **One logical change per PR.** Keep PRs focused and reviewable.
2. **Update documentation** if your change affects public APIs or behavior.
3. **Add tests** for new functionality or bug fixes.
4. **Ensure CI passes:** `cargo build`, `cargo test`, `cargo clippy` must all succeed.
5. **Respond to review feedback** promptly and push updates as needed.

A maintainer will merge your PR once it is approved and all checks pass.

## Conventional Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages:

```
<type>(<scope>): <description>

[optional body]
```

Common types:

| Type | Use for |
|------|---------|
| `feat` | New features |
| `fix` | Bug fixes |
| `docs` | Documentation changes |
| `refactor` | Code restructuring without behavior change |
| `test` | Adding or updating tests |
| `chore` | Build, CI, or tooling changes |

Examples:

```
feat(android): add configurable foreground service type
fix(ios): prevent safety timer double-fire on cancel
docs(api): document StartConfig defaults
```

## Testing Requirements

- **Unit tests** for all new logic in `src/`.
- **Existing tests must pass** — run `cargo test` before pushing.
- **Platform-specific code** should include inline comments explaining platform behavior.
- **No `#[ignore]` tests** without a documented reason in the test body.

## License

By contributing to this project, you agree that your contributions will be
dual-licensed under the **MIT OR Apache-2.0** licenses, the same terms as the
project itself. See [LICENSE](LICENSE) for details.

## Security

For security vulnerabilities, please follow the
[Security Policy](SECURITY.md) instead of filing a public issue.
