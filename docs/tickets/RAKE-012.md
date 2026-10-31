# RAKE-012: Template bundle build task

**Milestone:** M1  
**Depends on:** RAKE-007

## Scope

`cargo xtask template`: build `rake-runtime` in release mode with size-focused profile (LTO, strip, opt-level tuning), assemble a template `.app` with a placeholder `Info.plist`, empty `Resources`, and a placeholder icon, then tar and checksum it.

## Acceptance criteria

- [ ] Produces `rake-template-<version>-macos-arm64.tar.gz` and its SHA-256.
- [ ] The template `.app` launches after a config is dropped in.
