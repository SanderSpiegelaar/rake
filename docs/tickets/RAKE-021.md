# RAKE-021: JSON output and exit codes

**Milestone:** M2  
**Depends on:** RAKE-020

## Scope

`--json` prints exactly one JSON object on stdout (status, app path, duration, warnings); human output goes to stderr. Exit codes: 0 success, 1 unexpected, 2 bad input, 3 build failed, 4 missing environment or template.

## Acceptance criteria

- [ ] stdout is valid JSON for both success and failure.
- [ ] Each exit code is covered by an integration test.
