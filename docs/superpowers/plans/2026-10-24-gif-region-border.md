# GIF region indicator repair

> **For agentic workers:** Use superpowers:executing-plans to implement the approved scope below.

**Goal:** During a partial-screen GIF recording, show a border around the actual captured region instead of a monitor-wide WGC indicator.

**Status:** User approved this exact scope on 2026-10-24. Implemented, independently reviewed, tested and published as v1.2.1; evidence is in `docs/gif-region-report.md` and `docs/verification.md`.

**Confirmed symptom:** The user selected part of a screen; the recording indicator outlined the entire screen, while the exported GIF correctly contained only the selected part.

**Cause:** `GraphicsCaptureSource.TryCreate` creates a monitor capture item for a region. Windows therefore indicates the monitor. `Crop` subsequently produces the correct region pixels. `RecordingControlWindow` currently has no persistent region outline.

**Architecture:** Use the existing GDI source only for GIF recordings with no window handle whose bounds do not exactly match a monitor. Keep the WGC preference for MP4, selected-window GIF and whole-monitor GIF. A capture-excluded, non-activating, mouse-transparent thin outline shows the region during partial-screen GIF recording. The existing recording-control lifecycle owns and closes it.

**Tech Stack:** Existing .NET 8 / WPF / Win32 / Cap.Capture; no dependencies, package or permission changes.

## Files and scope

- `src/Cap.Recording/RecordingSession.cs`: choose the region GIF capture request; encoding, cursor overlays and output dimensions remain unchanged.
- `src/Cap.App/RecordingRegionWindow.cs` (new): narrow region outline and native physical placement / input pass-through / capture exclusion.
- `src/Cap.App/RecordingControlWindow.cs`: create, pause-style and release the outline with the controls.
- `tests/Cap.Recording.Tests/SessionTests.cs`: verify partial GIF uses GDI and still encodes the selected dimensions; preserve full-monitor/window/MP4 policy.
- `tests/Cap.Ui.Tests`: verify native outline placement, input transparency, exclusion, pause and close lifecycle.
- `README.md`, `docs/verification.md`: record the exact backend exception and actual verification; update the final EXE hash after publishing.

## Verification and risks

- [x] Obtain user approval before production logic changes, as required by the supplied AGENTS.md.
- [x] Back up sensitive source/docs under `.artifacts/ui-backup` and inspect precise diffs.
- [x] Reproduce a failing region-GIF backend assertion and a missing-outline assertion.
- [x] Implement only the approved partial-GIF exception and region outline.
- [x] Verify a real GIF decodes to the selected size and recording progress reports GDI for a partial region.
- [x] Verify capture exclusion, native click-through/noactivate styles and stop cleanup; review failure cleanup without claiming fault injection coverage.
- [x] Verify signed physical coordinates and current desktop DPI positioning; document mixed-DPI hardware not exercised.
- [x] Run relevant recording/UI tests and Release build; publish `dist/cap.exe` and smoke-test the standalone EXE.

GDI captures visible pixels and cannot record an occluded window as WGC can. The exception therefore excludes selected-window recordings. Arbitrary monitor layouts and physical mixed-DPI displays still require their own hardware coverage. Do not request borderless WGC access or remove Windows indicators globally.
