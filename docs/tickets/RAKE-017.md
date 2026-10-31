# RAKE-017: Patch Info.plist

**Milestone:** M2  
**Depends on:** RAKE-015

## Scope

Set `CFBundleName`, `CFBundleDisplayName`, `CFBundleIdentifier`, `CFBundleShortVersionString`, `CFBundleVersion`, `CFBundleExecutable` and `CFBundleIconFile`.

## Acceptance criteria

- [ ] `plutil -lint` passes on the result.
- [ ] Finder and the Dock show the new name and icon.
