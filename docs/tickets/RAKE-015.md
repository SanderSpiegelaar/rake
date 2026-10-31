# RAKE-015: Assemble app from template

**Milestone:** M2  
**Depends on:** RAKE-014, RAKE-003

## Scope

Copy the cached template to the output path as `<Name>.app`, rename the executable, write `config.json` and inject files into `Resources`. Refuse to overwrite an existing app unless `--force`.

## Acceptance criteria

- [ ] Output app launches and loads the configured URL.
- [ ] Existing app is untouched without `--force`.
