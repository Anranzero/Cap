# Native recording UI

Implemented `src/ui/record_ui.rs` and `src/ui/record_setup.rs`, with setup tests in the private child module `src/ui/record_setup_tests.rs`; no C# source, capture/encoding backend, shared runtime, or settings persistence changes. Extracting the setup test module preserves all existing assertions and makes the implementation/test responsibility boundary explicit.

## Behavior

- `record_ui::show_setup(settings)` opens the MP4-default native setup. `record_setup::show_setup_format(settings, format)` can preselect MP4/GIF. Both formats retain their original default FPS/width behavior.
- The setup enumerates real monitors and visible eligible windows. It supports selected area, one monitor, all monitors, or a specific window; a refresh action updates window choices. Window bounds are read again when confirmed. Area confirmation emits `PickRecordingRegion(config)`, other targets emit `StartRecording(config)`. Cancel/close emits no start event.
- Advanced options include FPS, maximum output width, cursor, click highlights, absolute/relative coordinates, font size, a validated hex color field, and the native Windows color chooser. Width `0` preserves source dimensions. The body scrolls on short work areas while start/cancel and save location remain accessible.
- Output paths use the configured absolute directory and unique time/process/sequence filenames. Setup never overwrites or creates output media. Backend validation/publishing remains authoritative.
- `show_controls(config, settings)` returns `(HWND, Rc<RefCell<RecordingStatus>>)`. The app writes backend status into the shared value. A native timer repaints duration, backend/frame counts, paused state, and saving state. Pause emits the toggle command. Stop and closing the bar request stop once and keep the bar showing “正在保存…” until the app destroys it after completion.
- The compact bar is draggable, topmost, does not activate on mouse click, and remains clickable. It is excluded from capture.
- GIF regions with no window handle have four capture-excluded, input-transparent, nonactivating strips exactly two physical pixels outside the selected rectangle. A scoped per-monitor DPI context protects physical placement. Paused strips turn gray. Control destruction removes every strip. MP4/WGC modes keep the system recording border.

## Actual verification

Build environment: dot-sourced `D:\Desktop\项目\.build-tools\Enter-RustBuild.ps1` (bundled SDK/LLD, static CRT).

```powershell
cargo test --bin cap-rust ui::record_ -- --test-threads=1
rustfmt --check --edition 2024 src/ui/record_ui.rs src/ui/record_setup.rs src/ui/record_setup_tests.rs
git diff --check -- src/ui/record_ui.rs src/ui/record_setup.rs src/ui/record_setup_tests.rs
```

Latest test result: **8 passed, 0 failed**. Coverage:

1. Cancel produces no recording command; GIF area setup preserves selected defaults and requests the selector.
2. Native monitor and whole-desktop choices produce matching physical bounds.
3. A test-owned target window preserves its real handle/bounds and all selected overlay options.
4. Unique output names and invalid integer input handling.
5. Advanced settings remain reachable after resizing to a short window.
6. Two-pixel border geometry, negative coordinates, and coordinate overflow rejection.
7. Native pause/continue/stop behavior; closing remains in saving state; repeated close is idempotent. Native edge HWND geometry, capture affinity, no-activate/click-through styles, paused gray pixels, and complete destruction are checked.
8. MP4 controls do not create custom region edges; hour-length timer formatting.

The initial three behavioral tests failed against the deliberately unimplemented entries, then passed after implementation. An added pixel-color assertion initially failed because the test had not driven the Win32 message queue; input-transparent windows defer painting in sibling order. With normal message pumping, the actual gray pixel assertion passes. Tests clean up their own control/target windows, including on assertion failure.

## Limits

- Tests create native test windows and recording UI only. They never start a recording, capture the user's desktop, register global hotkeys/startup, access clipboard content, or open a browser.
- No end-to-end encoder, recovery, or long-duration recording tests were run in this UI subtask; backend tests and app event integration are owned separately.
- The native color dialog's interactive selection and mixed-DPI monitor movement were not manually exercised. Physical edge placement, styles, capture affinity, and pixels were verified on the available monitor configuration.
- UI uses the shared palette, application icon/window class, and shared stock GUI font. The current settings theme determines setup appearance at opening; the recording bar reads the current theme when it paints.
- Including blank lines, `record_ui.rs` is 681 lines, `record_setup.rs` is 892 lines, and `record_setup_tests.rs` is 184 lines. No production dependencies were added.
