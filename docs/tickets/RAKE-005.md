# RAKE-005: CI pipeline

**Milestone:** M0  
**Depends on:** RAKE-001

## Scope

`.github/workflows/ci.yml` on a macOS arm64 runner: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, schema drift check. Cache the cargo registry and target dir.

## Acceptance criteria

- [ ] CI runs on every PR and push to main.
- [ ] A formatting error, clippy warning or failing test fails the run.
