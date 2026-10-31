# RAKE-022: Release workflow

**Milestone:** M3  
**Depends on:** RAKE-012, RAKE-021

## Scope

`.github/workflows/release.yml` on a version tag: build the template, upload the tarball and `SHA256SUMS`, then build and upload the `rake` CLI binary for the same version.

## Acceptance criteria

- [ ] Tagging `v0.1.0` produces a release with template, checksums and CLI.
- [ ] The released CLI downloads the template from that same release.
