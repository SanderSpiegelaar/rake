# RAKE-023: End-to-end test

**Milestone:** M3  
**Depends on:** RAKE-018

## Scope

CI job that builds an app from `tests/fixtures/basic.json` with a local template, launches it headless-safe, checks the process stays up for 5 seconds, then quits it.

## Acceptance criteria

- [ ] Job runs on every PR to main.
- [ ] A broken template or bundle fails the job.
