# RAKE-019: DMG output and --install

**Milestone:** M2  
**Depends on:** RAKE-018

## Scope

`--target app|dmg` (default app). DMG via `hdiutil create`. `--install` copies the app into `/Applications`, replacing an older Rake-built copy with the same identifier.

## Acceptance criteria

- [ ] DMG mounts and contains the app.
- [ ] `--install` puts a launchable app in `/Applications`.
