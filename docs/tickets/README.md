# Rake tickets

| ID | Milestone | Title | Depends on |
|---|---|---|---|
| [RAKE-001](RAKE-001.md) | M0 | Create Cargo workspace and repo skeleton | none |
| [RAKE-002](RAKE-002.md) | M0 | Define config schema in rake-config | RAKE-001 |
| [RAKE-003](RAKE-003.md) | M0 | Config validation and default identifier | RAKE-002 |
| [RAKE-004](RAKE-004.md) | M0 | Generate JSON Schema | RAKE-002 |
| [RAKE-005](RAKE-005.md) | M0 | CI pipeline | RAKE-001 |
| [RAKE-006](RAKE-006.md) | M1 | Runtime window and webview | RAKE-002 |
| [RAKE-007](RAKE-007.md) | M1 | Load config.json from the app bundle | RAKE-006 |
| [RAKE-008](RAKE-008.md) | M1 | Apply window options | RAKE-007 |
| [RAKE-009](RAKE-009.md) | M1 | Inject CSS and JS | RAKE-007 |
| [RAKE-010](RAKE-010.md) | M1 | Navigation rules for safe domains | RAKE-007 |
| [RAKE-011](RAKE-011.md) | M1 | Debug mode and devtools | RAKE-007 |
| [RAKE-012](RAKE-012.md) | M1 | Template bundle build task | RAKE-007 |
| [RAKE-013](RAKE-013.md) | M1 | Benchmark against Pake | RAKE-012 |
| [RAKE-014](RAKE-014.md) | M2 | Template fetch, verify and cache | RAKE-012 |
| [RAKE-015](RAKE-015.md) | M2 | Assemble app from template | RAKE-014, RAKE-003 |
| [RAKE-016](RAKE-016.md) | M2 | Icon fetch and conversion | RAKE-015 |
| [RAKE-017](RAKE-017.md) | M2 | Patch Info.plist | RAKE-015 |
| [RAKE-018](RAKE-018.md) | M2 | Ad-hoc code signing | RAKE-016, RAKE-017 |
| [RAKE-019](RAKE-019.md) | M2 | DMG output and --install | RAKE-018 |
| [RAKE-020](RAKE-020.md) | M2 | CLI flags and config merge | RAKE-015 |
| [RAKE-021](RAKE-021.md) | M2 | JSON output and exit codes | RAKE-020 |
| [RAKE-022](RAKE-022.md) | M3 | Release workflow | RAKE-012, RAKE-021 |
| [RAKE-023](RAKE-023.md) | M3 | End-to-end test | RAKE-018 |
| [RAKE-024](RAKE-024.md) | M3 | Docs: README and cli-usage | RAKE-021 |
| [RAKE-025](RAKE-025.md) | M4 | Builder app skeleton with pinned GPUI | RAKE-021 |
| [RAKE-026](RAKE-026.md) | M4 | Builder form for P0 options | RAKE-025 |
| [RAKE-027](RAKE-027.md) | M4 | Icon picker | RAKE-026, RAKE-016 |
| [RAKE-028](RAKE-028.md) | M4 | Build action with progress | RAKE-026 |
| [RAKE-029](RAKE-029.md) | M4 | Config import and export | RAKE-026 |
| [RAKE-030](RAKE-030.md) | M5 | macOS Intel and universal templates | RAKE-022 |
| [RAKE-031](RAKE-031.md) | M5 | Linux template and packaging | RAKE-022 |
| [RAKE-032](RAKE-032.md) | M5 | Windows template and packaging | RAKE-022 |
