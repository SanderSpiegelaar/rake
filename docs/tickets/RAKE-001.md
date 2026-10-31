# RAKE-001: Create Cargo workspace and repo skeleton

**Milestone:** M0  
**Depends on:** none

## Scope

Workspace root `Cargo.toml` with members `crates/*` and `xtask`, shared `[workspace.dependencies]` with exact versions, `rust-toolchain.toml`, MIT `LICENSE`, `.gitignore`, README stub. Empty crates: `rake-config`, `rake-runtime`, `rake-core`, `rake-cli`.

## Acceptance criteria

- [ ] `cargo build --workspace` succeeds on macOS arm64.
- [ ] Every dependency version is pinned in the workspace root, not in member crates.
- [ ] Repo tree matches docs/ROADMAP.md.
