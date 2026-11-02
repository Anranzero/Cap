# Annotation UI update

- `EditorToolbar.cs`: a slim single icon row, thin separators, red cancel and green confirm. All ten tools, undo/redo, pin, save, copy completion and region confirmation retain their existing handlers. Color and stroke controls appear when an annotation tool is active. Wrap panels keep controls reachable on narrower displays. Local templates prevent application button padding from enlarging icons and color dots; controls have Chinese tooltips and automation names.
- `TextPrompt.cs`: a compact light/dark text dialog with rounded frame, input focus styling, vector close/check icons and clear cancel/add actions. Existing text trimming, 4096-character limit, multiline entry, Ctrl+Enter confirmation and cursor/monitor placement remain.
- `PinWindow.cs`: consistent blue edge and vector copy/close menu icons. Image geometry, dragging, wheel scaling, double-click reset, clipboard behavior and Esc close remain.

Shared dependency: `Cap.Visuals.VectorIcon.Create(string, double, Brush?)`, linked by the root task from `src/Shared/VectorIcon.cs`. Every annotation icon supplies an explicit foreground brush. No new package dependencies.

Final reference palette: graphite `#262627`, dark text `#F3F2F6`, warm light surface `#FCFAFC`, light text `#202126`, blue `#087DFF`; the default single-row toolbar is 46 DIP high with a 14 DIP capsule radius. Final palette changes were followed by another successful build, test run and diff check.

Archival originals were copied before edits to `.artifacts/ui-backup/annotation-*`. The backups were not used to overwrite project files.

Actual verification using `C:\Users\Tomoto\AppData\Local\cap-dev\dotnet\dotnet.exe`:

- `build src/Cap.Annotation/Cap.Annotation.csproj --no-restore`: passed, zero warnings/errors.
- `run --project tests/Cap.Annotation.Tests/Cap.Annotation.Tests.csproj --no-restore`: passed. Existing checks cover selection geometry, pixelate/blur, exact crop pixels, history, every annotation tool, PNG/JPEG export and locked-file preservation.
- `git diff --check -- src/Cap.Annotation/EditorToolbar.cs src/Cap.Annotation/TextPrompt.cs src/Cap.Annotation/PinWindow.cs`: passed.

No new tests were added for this presentation-only change. Live native rendering, pointer interactions, mixed DPI monitor placement and dark-mode popup hover appearance still require visual QA by the root task. Screenshot coordinate/DPI and export logic were not modified by this subtask.
