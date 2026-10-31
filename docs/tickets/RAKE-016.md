# RAKE-016: Icon fetch and conversion

**Milestone:** M2  
**Depends on:** RAKE-015

## Scope

Use `--icon` (local path or URL) or fetch the site's icon (apple-touch-icon, then link rel icon, then `/favicon.ico`). Convert to `.icns` with the standard sizes and place it in `Resources`. Fall back to the default icon with a warning.

## Acceptance criteria

- [ ] Sites with PNG or ICO icons produce a valid `.icns`.
- [ ] A site with no icon still builds, with a warning.
