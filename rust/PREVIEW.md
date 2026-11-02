# Native preview and pin implementation

The original C# files remain unchanged. The new UI uses Win32 windows and GDI drawing, GDI+ for GIF decoding, and the Rust Media Foundation decoder for MP4 playback. It adds no production dependency and starts no browser or C# process.

## Files and public entry points

- `src/ui/preview.rs`: `show(PathBuf, Rc<RefCell<Settings>>)` opens a GIF/MP4 recording; `show_image(Frame, Rc<RefCell<Settings>>)` opens a screenshot/scroll result. MP4 supports real decoded playback, pause/resume, seeking, progress, and looping. Images fit the available canvas. The window has save/copy controls, recording metadata, and a recording-folder context action.
- `src/ui/gif_player.rs`: system GIF frame selection, recorded frame delays, delta-frame composition, looping, error reporting, and image/token cleanup. The application retains a composed canvas instead of accumulating decoded frames.
- `src/ui/pin.rs`: `show(Frame, settings)` positions a pin by the pointer; `show_at(Frame, Rect, settings)` preserves a requested capture location. Pins are topmost tool windows with capture exclusion, drag, wheel zoom, reset on double click, native copy/close context menu, Ctrl+C, and Escape.

File actions emit `SaveFile` / `CopyFile` / `OpenFolder`. In-memory image actions emit `SaveImage` / `CopyImage`. The root application owns the corresponding dialogs, clipboard writes, and shell operations.

## Verification on Windows, 2026-10-27

After dot-sourcing the portable build environment:

```powershell
. 'D:\Desktop\项目\.build-tools\Enter-RustBuild.ps1'
$env:CARGO_TARGET_DIR = 'D:\Desktop\项目\cap\.artifacts\rust-target'
cargo test --bin cap-rust --offline -- --test-threads=1
```

Result: **10 passed, 0 failed, 0 ignored**. This included:

- Real GDI+ GIF decoding of a two-frame fixture with a partial second frame, exact delay checks, and wraparound to the first frame.
- A damaged GIF rejecting cleanly and releasing the file.
- A generated real MP4 decoding through pause, seek, resume, and end-of-file loop.
- Native preview windows creating controls, pausing, receiving a timeline seek, emitting the correct image/file action and pixel payloads, and closing.
- A native pin checking `WDA_EXCLUDEFROMCAPTURE`, topmost style, wheel resize, double-click reset, and Escape destruction.
- Clock, time formatting, and zoom boundary checks, including a negative-origin monitor.

The native tests exposed two shared-UI defects, fixed by the root agent: process-wide window-class reuse across test threads, and an empty `DrawTextW` call that crashed image-preview painting. The exact image-window reproducer and the subsequent complete ten-test run passed after those fixes. Temporary diagnostics were removed.

An earlier broad library test run reached 45 passed and one ignored OCR-language test, then its existing `native_owned_window_capture_smoke` stalled beyond 60 seconds; only that owned cargo session was interrupted. This is not a completed library-suite result; the backend agent owns capture verification.

## GIF timing regression, 2026-10-27

GIF frame deadlines now accumulate the recorded delays instead of starting a new delay at each timer callback. A 20 ms frame sequence therefore keeps its recorded speed with the window's 15 ms timer. Each callback decodes at most eight overdue frames and retains any remaining backlog for later callbacks. Pause/resume shifts the deadline by the paused time, preserving the remainder of the current frame. GIF loop order and the displayed total duration remain unchanged; MP4 position, seeking, and looping retain their existing implementation.

`src/ui/preview/gif_timing_tests.rs` uses real GDI+ decoding with explicit `Instant` values and checks visible pixels. The late-callback regression failed against the old implementation at 45 ms, then passed after the deadline fix. The pause/resume regression also failed before its fix at the expected 1010 ms transition, then passed. Variable frame delays, loop boundaries, and bounded recovery from a five-second stall are covered.

With the same build environment, `cargo test --bin cap-rust --offline gif_timing_tests -- --nocapture --test-threads=1` passed **4 tests**. A subsequent full `cargo test --bin cap-rust --offline -- --test-threads=1` run passed all nine preview tests and all three GIF decoder tests, but its overall result was **32 passed, 10 failed**. The seven `ui::scrolling::native_tests` failures shared the initial screenshot pixel comparison at `scrolling_native_tests.rs:83`. Three settings tests failed: extended-key capture could not find a control, modifier capture posted `a0`, and the short-window footer check found controls outside the window. Other agents were running native window tests concurrently, so desktop interference is possible but not established. These failures require a serialized full-suite rerun; they were not changed by the GIF fix.

## Remaining acceptance work

The tests verify native action routing, not interaction with the system Save As dialog, external file managers/chat applications, or actual clipboard consumption. Those are root application integration checks. Menus and drag handlers were reviewed, but interactive menu selection and pointer dragging were not automated. High-DPI appearance and a visual comparison against the original GUI still need desktop acceptance. No browser was opened.

## Final integration follow-up

The root agent found that desktop screenshots contained the Windows lock screen. After the user unlocked the desktop and settings regressions were fixed, the serialized all-target suite passed: 58 library tests, 46 binary tests and 23 portable algorithm cases (the latter repeat core algorithm coverage), with four explicitly ignored helper/optional tests. OCR was also run explicitly and passed. The earlier seven scrolling screenshot failures did not reproduce after unlocking. The standalone app smoke then passed the native selection/annotation export and both GIF/GDI and MP4/Windows Graphics Capture recording flows, including pause/resume, decoded pixel checks, preview creation and quit during recording. Dark/light settings screenshots were inspected. Final release evidence and remaining clipboard-consumer limitation are recorded in VERIFICATION.md.
