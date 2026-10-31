# RAKE-025: Builder app skeleton with pinned GPUI

**Milestone:** M4  
**Depends on:** RAKE-021

## Scope

`crates/rake-builder`: pin GPUI to one git rev or `gpui-pre` version, open a main window with an empty form layout.

## Acceptance criteria

- [ ] App builds and opens on macOS arm64.
- [ ] GPUI source and version recorded in Cargo.toml with a comment on why.
