# RAKE-014: Template fetch, verify and cache

**Milestone:** M2  
**Depends on:** RAKE-012

## Scope

In `rake-core`: resolve the template URL for OS, architecture and the CLI's own version from GitHub Releases; download, verify against `SHA256SUMS`, extract to the user cache dir under `templates/<version>/<target>/`. Reuse the cache on later runs; `--refresh-template` forces a re-download.

## Acceptance criteria

- [ ] Second run makes no network request.
- [ ] A checksum mismatch deletes the download and fails with exit code 4.
- [ ] Works offline when the cache exists.
