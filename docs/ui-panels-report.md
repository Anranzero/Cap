# Compact settings and recording panels

## Scope

- `src/Cap.App/SettingsWindow.cs`: 620 × 520 settings window with four small category tabs, one visible page, compact rows and a fixed save/cancel footer. All prior settings remain available.
- `src/Cap.App/RecordingSetupWindow.cs`: 390 × 390 recording configuration window. MP4/GIF selection, cursor and coordinate switches, and a full-width start action remain visible. Target, FPS, width, click highlight and coordinate appearance are under the initially collapsed advanced section.
- `src/Cap.App/SettingsPanel.cs`: small layout helpers shared by these panels. Colors and control styling come from the existing app resources.

The recording constructor and `Options` / `SelectRegion` contract remain unchanged. `Options.Format` follows the inline format switch. Switching format selects that format's saved FPS and width defaults, including non-preset GIF FPS. The default target represents the region selected by the app before configuration; advanced monitor/window targets remain available.

## Behavior and safety

`SettingsWindow.Save()` and `RecordingSetupWindow.Confirm()` retain their original validation, callbacks, option construction and error dialogs. Turning coordinates off disables their appearance inputs while retaining values. No dependencies or persistent settings fields were added.

Traceable source backups are in `.artifacts/ui-backup/20261024-ui-panels` and `.artifacts/ui-backup/20261024-ui-panels-compact`. Strict UTF-8 decoding found no replacement characters; original LF line endings remain unchanged. The changes were applied with patches.

## Verification

- `git diff --check -- src/Cap.App/SettingsWindow.cs src/Cap.App/RecordingSetupWindow.cs`: passed.
- `dotnet run --project tests/Cap.Tests/Cap.Tests.csproj --no-restore -c Release`: passed all nine existing checks, including settings round-trip, corrupt-file recovery, hotkeys and invalid FPS rejection.
- Exact comparison of `Save()` and `Confirm()` against the initial source backup: both unchanged.
- `dotnet build src/Cap.App/Cap.App.csproj --no-restore -c Release`: passed with zero warnings and zero errors after the shared title-bar owner fixed the `Ui.Button` name collision.
- Native rendering and interaction verification are pending integration of shared styles and the app's selection-first flow. In particular, inspect all settings categories, recording advanced expansion, both themes and MP4/GIF switching before delivery.

No layout-mirroring unit tests were added. Existing validation code was retained; runtime visual and interaction verification remains necessary.
