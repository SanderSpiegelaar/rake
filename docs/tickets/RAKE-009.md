# RAKE-009: Inject CSS and JS

**Milestone:** M1  
**Depends on:** RAKE-007

## Scope

Read inject files listed in config from `Resources/inject/`. JS runs as an initialization script on every page load; CSS is injected through a small JS shim that adds a style element.

## Acceptance criteria

- [ ] Injected CSS and JS apply after in-app navigation and reloads.
- [ ] A missing inject file fails at startup with the file name in the error.
