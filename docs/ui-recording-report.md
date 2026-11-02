# Recording and preview UI

Changed only `src/Cap.App/RecordingControlWindow.cs` and `src/Cap.App/PreviewWindow.cs`.

- Recording control: 350 × 56 rounded bar with a drag grip, recording dot, fixed-width timer, vector pause/resume and red stop buttons. Backend/frame/drop details are available in the timer tooltip. Existing public events and methods, physical monitor placement, capture exclusion, and close protection remain.
- Preview: custom title bar, large media area, one muted filename/size line, save and copy-file actions. The folder action is in the content context menu. Copy uses a real file-drop list for the existing MP4/GIF file.
- MP4: actual MediaElement playback and seek position, elapsed/total time, 200 ms progress refresh, and thumb-drag protection. Duration uses TotalSeconds (no 32-bit millisecond conversion). A dragged slider does not get overwritten by timer ticks. The existing loop behavior remains.
- GIF: the existing PictureBox animation continues to loop. It displays the real recorded duration and does not present a fake seek slider. Native GIF seeking is not supported by this host.
- Cleanup: stop the dispatcher timer, close MediaElement, dispose the GIF image/PictureBox and WindowsFormsHost on window close. Existing save-as semantics remain; file/folder/clipboard failures use Ui.Error.

Original source backups: `.artifacts/ui-backup/RecordingControlWindow.cs` and `.artifacts/ui-backup/PreviewWindow.cs`.

## Verification

- `git diff --check`: passed after the UI edits.
- `dotnet build ...`: PATH dotnet has no SDK. The repository's SDK at `$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe` was used for actual builds.
- After the concurrent shared Ui.cs fix landed, `& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" build src\Cap.App\Cap.App.csproj --no-restore -v:minimal` passed with zero warnings and zero errors.
- No browser was opened. Native visual QA, actual MP4 playback/seek, GIF animation, clipboard paste, minimum-size layout, and multi-monitor movement remain for the root agent's native UI verification.
