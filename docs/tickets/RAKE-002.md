# RAKE-002: Define config schema in rake-config

**Milestone:** M0  
**Depends on:** RAKE-001

## Scope

`RakeConfig` struct covering every P0 option: url, name, identifier, icon, width, height, min_width, min_height, title, user_agent, hide_title_bar, dark_mode, inject (list of paths), safe_domains, debug. Serde with defaults (1200 x 780), `deny_unknown_fields`, and a `schema_version` field.

## Acceptance criteria

- [ ] A config with only `url` deserializes with all defaults filled.
- [ ] Unknown fields are rejected with the field name in the error.
- [ ] Round-trip test: struct to JSON to struct is equal.
