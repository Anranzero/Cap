# Capture implementation report

## API and ownership

`Cap.Capture.CaptureService` is a static service. Geometry, frame, request and source contracts come from `Cap.Core`.

| API | Behavior |
| --- | --- |
| `GetVirtualBounds()` | Union bounding rectangle of the desktop, in physical pixels. |
| `GetMonitors()` | Individual physical monitor rectangles; negative origins are preserved. |
| `GetWindows()` | Visible, titled top-level windows in native z-order; skips minimized, cloaked, tool and display-affinity-excluded windows. |
| `GetWindowAt(x, y)` | Native top-level hit test, with z-order rectangle fallback beneath excluded/tool overlays. |
| `Capture(bounds)` | Immediate independent GDI screenshot; tightly packed top-down opaque BGRA32. |
| `CreateSource(request)` | Disposable frame source, preferring WGC where supported. |
| `ExcludeWindow(hwnd)` | Applies `WDA_EXCLUDEFROMCAPTURE`; failure is surfaced as `Win32Exception`. Applies only to owned top-level windows. |
| `ToBitmapSource(frame)` | Validates the BGRA frame and returns a frozen WPF bitmap. |

Callers must dispose `ICaptureSource`. Each returned frame owns an independent byte array, allowing cursor/annotation composition without changing cached frames. Capture and disposal are serialized per source. The service enters/restores per-monitor-v2 thread DPI awareness around coordinate-sensitive desktop operations.

## Backends and semantics

GDI uses a top-down 32-bit DIB, `BitBlt(SRCCOPY | CAPTUREBLT)`, `GdiFlush`, deterministic bitmap/DC release and explicit opaque alpha. It never draws the native cursor. A requested rectangle extending outside the virtual desktop is black-padded; dimensions and origin remain those originally requested.

Windows Graphics Capture uses `GraphicsCaptureItem` HWND/HMONITOR interop, a free-threaded BGRA frame pool and a native D3D11 staging texture. The bridge creates the WinRT Direct3D device without third-party production NuGet packages; Windows SDK projections are supplied by the target framework. Hardware device creation falls back to WARP. The session explicitly disables native cursor capture.

WGC is selected for window capture and regions wholly contained by one monitor. Multi-monitor regions and explicit opt-out requests use GDI. Unsupported/startup-failed WGC uses GDI; runtime graphics failures switch once to GDI and log the original exception through `Trace.TraceWarning`. `BackendName` reports the active backend. Every window source, including explicit or fallback GDI, checks the selected HWND and reports closure instead of recording the desktop behind a closed window. A WGC closed-target event also stops the source rather than switching backend.

Region WGC crops the monitor texture at the requested physical offset. Window WGC follows the captured window content, anchored at its top-left: larger content is cropped and smaller content is black-padded, maintaining the initial requested output size. For WGC window sources, `Bounds.X/Y` reports the current physical window origin so recorder cursor/highlight overlays use coordinates aligned with the moving content; `Bounds.Width/Height` remains fixed. Region and GDI source bounds remain the original rectangle. The frame pool is recreated after a target size change. Pending older frames are drained, and periods without a fresh frame return a copy of the previous frame immediately. Only the first frame waits, with a 1500 ms bound; subsequent cached captures do not wait for WGC changes. The recording loop owns pacing, so independent cursor/coordinate/click composition can continue at the configured frame rate on an unchanged target.

GDI fallback captures the original desktop rectangle, so it includes occluding windows and does not follow a moved window. This is a material fallback limitation. WGC's operating-system capture border is left under the OS default; disabling it can require capabilities/consent beyond an ordinary desktop application. HDR is captured as BGRA8 and this implementation does not add HDR tone mapping. Protected/secure content remains subject to Windows capture restrictions.

## Actual verification

Commands used with .NET SDK 8.0.426:

```powershell
& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" build src\Cap.Capture\Cap.Capture.csproj -c Release --nologo
& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" run --project tests\Cap.Capture.Tests\Cap.Capture.Tests.csproj -- --native
```

Release compilation passed with zero warnings/errors. The dependency-free console verification harness checks invalid/overflowing dimensions, null/invalid bitmap inputs, frozen BGRA conversion, opaque alpha, exact requested dimensions, explicit GDI selection, source disposal, physical monitor containment, black padding outside the desktop, window enumeration/hit test and repeated GDI resource stability.

Native verification ran on this Windows desktop at 2560×1440 with one monitor and one visible titled window. Automatic region capture reported `Windows Graphics Capture` both before and after two captures; window capture also reported `Windows Graphics Capture`. A controlled native test window positioned entirely outside the desktop tested real WGC source-origin updates after movement, fixed output dimensions, and closure exceptions for both GDI and WGC window sources. The window was then hidden to exercise startup fallback and closure validation. All harness checks passed. No browser or visible test UI was opened, and no desktop image was saved by this harness.

A later pacing regression check primes a real static native-window WGC source, then performs 60 captures and requires completion within one second. It passed here in 2.6 ms, confirming cached frames do not impose the former 100 ms wait. This measures capture-call availability rather than end-to-end encoder throughput or rendered overlay playback.

## Remaining verification scope

Negative-coordinate multi-monitor desktops, mixed DPI, actual multi-monitor GDI composition, live window resizing, visual cursor/content alignment during movement, graphics-device loss, monitor removal, secure desktop, HDR and hour-long recording/resource behavior require hardware/manual scenarios and have not been exercised here. Exclusion applies the real Windows API but visual exclusion itself is not covered by the console harness. Region/window native tests validate format, sizes and backend continuity, not a deterministic rendered color reference or recording playback.

The repository initially had no committed files, so ordinary `git diff` cannot compare the new source files with a tracked baseline. Changes were made through explicit new-file/local patches; new files were reviewed directly and no existing shared contracts were edited.
