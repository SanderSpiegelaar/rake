# RAKE-003: Config validation and default identifier

**Milestone:** M0  
**Depends on:** RAKE-002

## Scope

`validate()`: URL must be http(s); width and height not smaller than the minimums; safe domains are bare hostnames; identifier matches reverse-DNS. Default identifier derived from name and URL hash, stable across runs.

## Acceptance criteria

- [ ] Each invalid case returns a typed error naming the field.
- [ ] Same name and URL always give the same identifier.
- [ ] Unit tests cover every rule.
