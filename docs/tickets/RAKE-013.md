# RAKE-013: Benchmark against Pake

**Milestone:** M1  
**Depends on:** RAKE-012

## Scope

Build the same site with Pake and with the Rake template. Measure bundle size, cold start time, and idle memory after 30 seconds. Record method and numbers in `docs/benchmarks.md`.

## Acceptance criteria

- [ ] Numbers for both apps with the exact measurement commands.
- [ ] Rake bundle is no larger than Pake's, or the PR explains why and proposes a fix.
