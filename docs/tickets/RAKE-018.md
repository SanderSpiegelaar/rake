# RAKE-018: Ad-hoc code signing

**Milestone:** M2  
**Depends on:** RAKE-016, RAKE-017

## Scope

Run `codesign --force --deep --sign -` as the last step after every bundle change. Surface codesign errors with its output.

## Acceptance criteria

- [ ] `codesign --verify --deep --strict` passes.
- [ ] The app launches on Apple Silicon after signing.
- [ ] Copying the app to a second Mac and opening it works after the expected Gatekeeper prompt.
