# Native scrolling capture UI

`src/ui/scrolling.rs` implements `show(bounds: Rect, settings: Rc<RefCell<Settings>>) -> Result<HWND, String>`. It returns the floating bar handle; the root application can use its lifetime to block conflicting captures. Completion emits `UiEvent::PreviewImage(Frame)`; cancellation closes without an image. The original C# workflow, outline, and input files were read and remain unchanged.

The bar provides automatic downward scrolling/pause, Undo, Finish, and Cancel. It is draggable and starts without taking the target's focus. Four separate two-physical-pixel windows sit strictly outside the capture rectangle. They are topmost, capture-excluded, input-transparent, and do not activate. The bar is also capture-excluded. Window creation explicitly uses a per-monitor physical DPI context and restores the caller's context afterward.

Manual capture polls every 400 ms and requires two identical complete BGRA viewports before calling the existing conservative `ScrollStitcher`. Animation, reversed motion, uncertain overlap, and length limits do not silently add pixels. Undo requires returning the visible page to the retained viewport before new capture can continue. Finish checks the last viewport twice, 160 ms apart. A moving or unmatched final viewport stays open with an explicit second-Finish option to keep the already confirmed content.

Automatic scrolling posts the original implementation's negative `WM_MOUSEWHEEL` step to the window at the selected region's center, after verifying that it still belongs to the original visible root window. It never moves the pointer, injects clicks, or installs a global mouse hook. It stops after three unchanged results. Unsupported or obstructed targets fall back to manual capture with a visible error.

Escape temporarily registers on the bar without replacing the user's other hotkeys. If registration is occupied, the bar advertises click-to-cancel. Escape/Enter/Ctrl+Z work while the bar is focused; native button subclasses forward the same keys. Cancellation, finish, external destruction, and partial initialization release outline windows, the timer, pending frames, and the temporary hotkey.

## Verified on Windows, 2026-10-27

```powershell
. 'D:\Desktop\项目\.build-tools\Enter-RustBuild.ps1'
$env:CARGO_TARGET_DIR = 'D:\Desktop\项目\cap\.artifacts\rust-target'
cargo test --manifest-path rust\Cargo.toml --bin cap-rust --offline ui::scrolling:: -- --test-threads=1
```

Result: **12 passed, 0 failed, 0 ignored**. Five state/geometry tests cover settled frames, every-pixel stability, retained-position undo, reverse/missing overlap, limits, negative coordinates, and overflow. Seven native tests in `src/ui/scrolling_native_tests.rs` create a private deterministic scrollable window and use actual GDI captures to verify:

- Exact stitched pixel rows after automatic/manual scrolling, pause, Undo, resumed capture, and Enter on a native button.
- Unchanged pointer position and unchanged foreground window when opening the bar.
- Exact outside-edge geometry, capture-exclusion affinity, click-through response, physical DPI awareness, and caller DPI restoration.
- Automatic stopping at the bottom, preserving all confirmed rows.
- Separate explicit retention for both a moving last viewport and a settled viewport without overlap.
- Target closure reporting an error while still allowing retained content to finish.
- Focused-button Escape, outline destruction, and temporary-hotkey release.

The first two state tests were observed failing before implementation and then passing. No browser was opened and no real user page was scrolled.

## Acceptance limits

Third-party applications may ignore posted wheel messages or block them through Windows integrity boundaries; users can still scroll manually. Interactive bar dragging and appearance across monitors at different actual scaling factors need desktop acceptance. The tests cover physical DPI context and geometry but do not claim a visual comparison on multiple scaling configurations. Root application command exclusion and preview presentation remain integration responsibilities.
