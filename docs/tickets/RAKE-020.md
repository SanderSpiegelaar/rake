# RAKE-020: CLI flags and config merge

**Milestone:** M2  
**Depends on:** RAKE-015

## Scope

`rake <url> [options]` with clap: every P0 flag, `--config <file>`, flags override file values, then validation. `--version` prints the Rake and template version.

## Acceptance criteria

- [ ] Flag-only, config-only and mixed inputs give the same result for the same values.
- [ ] Invalid input exits with code 2 and a clear message.
