# RAKE-030: macOS Intel and universal templates

**Milestone:** M5  
**Depends on:** RAKE-022

## Scope

Add x86_64 and universal template targets to the release workflow; CLI picks the matching one.

## Acceptance criteria

- [ ] Intel Mac builds and runs an app.
- [ ] `--target universal` produces a universal binary (`lipo -info` shows both).
