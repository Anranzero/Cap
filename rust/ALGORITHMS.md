# Algorithm port progress

Scope: geometry, BGRA pixels, annotation, conservative scrolling stitcher, streaming GIF. Original C# sources and tests were read before implementation. No C# files are modified.

Public API: `Rect::new`; `Frame::{new,blank,crop,validate,stride}`; `AnnotationDocument::{new,add,render,export,effect,undo,redo}`; `ScrollStitcher::{new,with_limits,try_append,undo,create_frame}`; `GifEncoder<W: Write>::{new,write,finish,into_inner}`. Native text rasterization enters annotation as a transparent BGRA stamp.

Verification: tests are being written before algorithms. Test results and limitations will be appended here.

Reference review completed: `Geometry.cs`, `SelectionGeometry.cs`, `AnnotationDocument.cs` (including `RasterEffects`), `ScrollingCaptureStitcher.cs`, `GifEncoder.cs`; C# annotation, scroll, core, and recording encoder tests.

Test-first fixtures now cover negative desktop origin and exclusive edges; overflow; selection dragging/eight handles; BGRA crop/orientation/resize; invalid mutable buffers; shape tools, alpha stamps, exact undo/redo and branch discard, 80-operation baking, configurable byte budget, sequence numbering; downward overlap, reverse and low overlap, limits, header/footer, repetitive content; GIF independent LZW decode through dictionary resets, exact palette colors, delta offset, repeat merge, 60 FPS duration, long delay splitting, gradient dithering, I/O failure and timing validation.

Pre-RED check: `rustc --edition 2024 --test --emit=metadata -A dead_code tests/algorithms_portable.rs` succeeded. Initial executable run was blocked by missing `link.exe` and SDK libraries. Implementation was held at stubs until the root agent supplied a project-local SDK and bundled rust-lld. This compilation check was **not** counted as a RED result.

Ruling: native text raster is supplied by the UI through `Stamp`; number badges have built-in digit rasterization. This keeps font/COM platform ownership in the UI and makes every annotation undoable as a pure pixel operation.

Ruling: one operation larger than the history byte budget will be baked immediately, so no single oversized effect can bypass the 128 MiB bound. Its pixels remain visible but the operation cannot be undone.

## Actual verification

- RED: root compiled `tests/algorithms_portable.rs` with bundled rust-lld, static CRT and the project-local SDK; executing `.artifacts/algorithms-red.exe` failed 21/22 tests against stubs. The algorithm worker reran it and read the failure list. The one initially passing test only asserts conservative rejection (the stub rejected everything); positive append tests failed.
- GREEN: `rustc --test tests/algorithms_portable.rs --edition 2024 -C linker=<bundled rust-lld.exe> -C target-feature=+crt-static -C debuginfo=0 -D warnings -o .artifacts/algorithms-test.exe`, then `.artifacts/algorithms-test.exe`: **23 passed, 0 failed**.
- Extra RED/GREEN: `history_accounts_for_retained_allocation_capacity` caught a retained-capacity budget bypass, failed before counting `Vec::capacity`, then passed after the fix. Stamp and pen storage now account for actual retained allocations.
- `clippy-driver --edition 2024 --test --emit=metadata tests/algorithms_portable.rs -D warnings`: passed with no warnings.
- `rustfmt --edition 2024 --check` on all owned Rust files: passed.
- With `CAP_GIF_FIXTURES` set to an isolated artifact folder, the GIF test emitted a fixture. `powershell -NoProfile -File tests/gif-native.ps1 -GifPath .artifacts/gif/algorithm-roundtrip.gif -BgraPath .artifacts/gif/algorithm-roundtrip.bgra`: **passed** using native GDI+ decoding, all 16,000 first-frame palette pixels, orientation, two coalesced frames, 7/13-centisecond delays, and the red 1x1 delta at (70,50).
- Initial `cargo test --offline --lib` could not run because the app build script required `rc.exe`. The root subsequently supplied RC; the full application/native suite is owned by the coordinator/backend and is not claimed here.

## Final behavior and integration

- Geometry validates checked exclusive edges and negative origins; selection helpers cover eight handles, clamped reverse dragging, movement and resizing.
- Frames validate exact tightly packed top-down BGRA length; `copy`, local `crop`, `resize_nearest` and pixel-center `resize_bilinear` are fallible. `Debug` prints dimensions and byte count instead of pixel dumps.
- `Style::color` and stamps use straight BGRA alpha. Commands include rectangle, ellipse, line, arrow, pen, number and stamp; native text is a stamp. Filters act on the currently rendered image, including previous annotations. `effect` receives stroke thickness and derives mosaic block `max(8, thickness*4)` / blur radius `max(3, thickness*2)`.
- Default history is at most 80 operations and 128 MiB across undo/redo payloads. Evicted commands remain baked into the base image. A new operation clears the redo branch. Number sequence includes baked number badges.
- Scroll rejects reverse/repeated/ambiguous/low-overlap frames before mutation; sticky top headers are ignored. Default limits are 40,000,000 pixels and 30,000 height; undo retains appended segments rather than full-frame history.
- Ruling: footer detection checks three stationary consecutive rows within the lower 20% (up to 48 rows), instead of averaging a 12-row footer band. A real 10-row fixed footer failed the original average due to two moving rows; the stronger check passes the regression. This deliberately prefers rejecting a page with stationary lower whitespace over introducing a bad seam.
- GIF keeps only pending/previous quantized frames, a bounded LZW dictionary and a 255-byte sub-block. RGB332 dithering is position-stable. Identical quantized frames merge; changes encode their bounding rectangle; long holds split 16-bit delays with 1x1 continuation images. Timing is in 100ns units, cumulative delay rounding avoids frame-rate drift, invalid/overlapping timestamps fail, and output errors permanently prevent finalization.

Limitations: native UI/font integration, real monitor/HDR captures, long-running recording memory, and native WIC integration belong to the other tasks. Native GIF decoding above used GDI+; backend WIC decoding is separate. No new dependency, C# modification, commit, desktop deployment, browser opening or source overwrite from a backup occurred. All owned source files are below 1,000 lines.
