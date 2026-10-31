# RAKE-028: Build action with progress

**Milestone:** M4  
**Depends on:** RAKE-026

## Scope

Build button runs `rake-core` on a background thread, shows step-by-step progress, and reveals the app in Finder when done. Errors show in the window.

## Acceptance criteria

- [ ] UI stays responsive during a build.
- [ ] Successful build reveals the app in Finder.
