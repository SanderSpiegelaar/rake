# RAKE-011: Debug mode and devtools

**Milestone:** M1  
**Depends on:** RAKE-007

## Scope

When `debug` is true, enable devtools and verbose logging to stderr. When false, devtools stay off.

## Acceptance criteria

- [ ] Devtools open only in debug builds of the config.
- [ ] No log output in non-debug mode.
