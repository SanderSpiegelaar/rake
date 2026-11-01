# RAKE-032: Windows template and packaging

**Milestone:** M5  
**Depends on:** RAKE-022

## Scope

WebView2 template, exe icon swap in resources after build, ZIP output.

## Acceptance criteria

- [ ] App runs on Windows 11 with the custom icon.
- [x] Icon swap tool choice documented.

The [Windows packaging reference](../cli-usage.md#windows-packaging) records the Win32 resource API choice and rejected alternatives. `tests/windows_smoke.py` verifies ZIP contents, custom icon resources, and a WebView2 JavaScript callback on Windows. CircleCI uses Windows Server 2022. It cannot prove the Windows 11 acceptance criterion.
