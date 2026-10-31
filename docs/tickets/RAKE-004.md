# RAKE-004: Generate JSON Schema

**Milestone:** M0  
**Depends on:** RAKE-002

## Scope

Derive a JSON Schema from `RakeConfig` (e.g. schemars) and write it to `schema/rake.schema.json` through an `xtask schema` command.

## Acceptance criteria

- [ ] `cargo xtask schema` regenerates the file.
- [ ] CI fails if the committed schema differs from the generated one.
