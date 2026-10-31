# RAKE-007: Load config.json from the app bundle

**Milestone:** M1  
**Depends on:** RAKE-006

## Scope

Find `config.json` in `Contents/Resources` relative to `current_exe()`. Fallback for development: a `RAKE_CONFIG` env var path. On a missing or invalid file, show a native error dialog and exit non-zero.

## Acceptance criteria

- [ ] Swapping `config.json` in a built `.app` changes the loaded site with no rebuild.
- [ ] An invalid config shows a readable error instead of a crash.
