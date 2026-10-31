# RAKE-010: Navigation rules for safe domains

**Milestone:** M1  
**Depends on:** RAKE-007

## Scope

Navigation handler: the start URL's host and configured safe domains (including subdomains) stay in-app; any other http(s) URL opens in the default browser and is blocked in the webview. New-window requests follow the same rule.

## Acceptance criteria

- [ ] Clicking an external link opens the system browser and the app stays on its page.
- [ ] Subdomains of a safe domain stay in-app.
- [ ] Unit tests for the host-matching function.
