# Native backend verification

Implemented only `src/capture*`, `src/media*`, `src/recording.rs`, `src/recovery.rs`, `src/overlays.rs` and this report. Original C# and user configuration, clipboard, startup settings and desktop shortcuts were not modified. Dependencies remain the approved windows, serde/serde_json and flate2 crates.

The public API shared with UI is `CaptureSession`, `CaptureTarget`, monitor/window enumeration and GDI screenshot; `RecordingSession::{start,pause,resume,request_stop,result,status,stop}`; `RecordingConfig`; `JournalReader/JournalWriter`, recovery discovery and re-encoding; WIC `save_image/load_image`; offline `ocr`; `Mp4Writer`; and incremental `VideoDecoder::{new,seek,next_frame}` with source duration/timestamps.

Implemented behavior includes hardware/WARP D3D11 WGC and row-pitch staging readback, fixed output dimensions while window origin follows movement, resizing crop/padding, GDI cross-monitor fallback and direct region GIF, capture exclusion, actual cursor shape/hotspot, native mouse hook and click pulse, configurable coordinate text, bilinear output scaling, one-worker bounded frame scheduling and active-time pause accounting, H.264/NV12 Media Foundation output and RGB32 source-reader playback, no-overwrite complete-file publication, and 256 MiB free-space checks.

CAPJ0001 uses PascalCase JSON, numeric MP4/GIF enums, raw RFC1951 DEFLATE and the original commit marker. Header dimensions and record lengths are bounded before allocation; truncated final records preserve earlier committed frames; corrupt records fail explicitly. Recovery preserves its source journal. Per-attempt recovery paths include PID, timestamp and an atomic sequence so concurrent attempts cannot delete each other's partial files.

The native WGC target-loss test initially reproduced a real shutdown deadlock in `GraphicsCaptureSession::Close`/COM Release. A child-process watchdog isolated normal stop from external target destruction. Normal stop worked; target destruction raced the provider's queued `Item.Closed` event. Sleeping did not deliver that event; pumping the creator thread's Win32 queue did, and teardown then completed. Capture now pumps that queue while reading and before shutdown, awaits the closed-window provider event for up to 1500 ms, revokes both event handlers, releases GPU staging state, and closes session then pool. Every acquired frame also has a lease that closes it on error. There is no background cleanup-thread workaround. The watchdog terminates only its own spawned test child on a timeout.

Verification was run from the project root after dot-sourcing `D:\Desktop\项目\.build-tools\Enter-RustBuild.ps1`, which configures the portable MSVC/SDK libraries, bundled rust-lld, static CRT, Cargo cache and resource compiler. Initial builds were blocked by missing linker/import libraries; these were environment failures, not passing tests.

Actual behavioral RED was observed for journal stubs: 4 of 7 tests failed with the expected missing reader/writer errors. Implementing the journal made them pass. A final concurrency regression test also showed RED (zero successful publications instead of one), then GREEN after unique recovery paths.

Executed verification commands:

```powershell
cargo test --manifest-path rust\Cargo.toml --lib recovery::tests --offline
cargo test --manifest-path rust\Cargo.toml --lib media::tests --offline
cargo test --manifest-path rust\Cargo.toml --lib capture::tests --offline -- --nocapture
cargo test --manifest-path rust\Cargo.toml --lib recording::tests --offline -- --nocapture
cargo test --manifest-path rust\Cargo.toml --lib overlays::tests --offline
cargo test --manifest-path rust\Cargo.toml --lib media::tests::native_ocr_recognizes_owned_text --offline -- --ignored --nocapture
cargo clippy --manifest-path rust\Cargo.toml --lib --offline -- -D warnings
cargo test --manifest-path rust\Cargo.toml --lib --offline
```

Scoped `rustfmt --edition 2024 --check` covers all nine backend source/test files and passed. Final `cargo clippy --lib --offline -- -D warnings` passed with no warnings. Final `cargo test --lib --offline` passed: **50 passed, 0 failed, 4 ignored**; explicit OCR acceptance then passed separately (**1 passed**). Recovery tests passed **8/8**, recording tests **4/4** plus its invoked child helper, and overlay tests **4/4**. All source files are below 1000 lines (largest: recording.rs, 637 lines). Verified native cases include exact PNG roundtrip; ten H.264 frames spanning one second, top/down colors and halfway/back-to-zero seeking; OCR of owned GDI text; actual WGC selection with normal/external-closed/moved/resized targets; automatic recording publication on target closure while recording and paused; direct GIF pause/repeated stop; a deterministic native monochrome cursor hotspot; native green coordinate text; hook start/stop; eight journal/recovery tests including the independent legacy DEFLATE fixture and concurrent recovery.

The default suite reports three ignored subprocess entry points that are executed by the enclosing watchdog tests, plus the OCR acceptance test, which was run explicitly and passed. No browser was opened. Hardware scenarios not covered here are actual multi-monitor mixed DPI/HDR, GPU device removal, monitor unplug, secure desktop, extended recording endurance and artificially exhausted disk space. JPEG saving is implemented through WIC but the automated exact image roundtrip uses PNG. Native provider failure after the 1500 ms closure-event wait is logged; a hung system/GPU provider is outside the tested successful shutdown cases.

One initial tooling read used Python's GBK default against the new UTF-8 sources and failed before writing. Explicit UTF-8 checks and exact token corrections followed; no original source encoding was changed.

## Recovery cancellation integration

Added `recover_with_cancellation(journal, output, &AtomicBool)` without changing the original `recover` API; `recover` now supplies a false token. The worker checks cancellation at entry, before each frame is read, between decoding and encoding, before encoder finalization, and immediately before publication. Returning cancellation drops the encoder and its file/COM resources before removing only that attempt's unique partial path. The source journal and an existing final destination are preserved. Cancellation is cooperative: an individual decoder/encoder/native I/O call already in progress must return before the next checkpoint; cancellation after the final publication checkpoint can race a completed atomic publication.

Both initial cancellation tests showed actual RED with an implementation that ignored the token. The pre-cancel test verified that no final output may be created and existing destination bytes must survive. The mid-loop test waits for a live encoded temporary file larger than 8 KiB, then sets the token; it uses no sleeps. Its observation opens the file to query its current length, because Windows directory-entry metadata remained cached until close. Input crosses the GIF palette boundary so frames are emitted before finalization. The test also verifies source bytes, preservation of an unrelated recovery partial, and removal of this attempt's partial.

Final additional verification after the cancellation change:

- `cargo test --manifest-path rust\Cargo.toml --lib recovery::tests --offline`: **10 passed**.
- `cargo clippy --manifest-path rust\Cargo.toml --lib --offline -- -D warnings`: passed.
- `rustfmt --edition 2024 --check rust\src\recovery.rs`: passed.
- Confirmed no other `cap_rust-*.exe` native test was running, then `cargo test --manifest-path rust\Cargo.toml --lib --offline`: **53 passed, 0 failed, 4 ignored**.

The WGC shutdown audit confirms a remaining limit: 1500 ms bounds only the explicit wait for the closed-target event. `pump_messages` drains queued messages, and native event revocation, D3D flush/map, session/pool Close and final COM Release have no hard deadline. A continuous message producer or an unresponsive GPU/system capture provider can therefore still prolong or block total shutdown. Normal stop, resized-target destruction, and target destruction while recording or paused passed the child-process watchdog tests; provider/GPU failure is not proven bounded. No WGC code was changed in this cancellation-only integration task.

## Independent review: capture/bounds race and strict WGC acceptance

Verified the review finding against both implementations: Rust caught `TARGET_CLOSED` only around `capture()`, while the subsequent `bounds()` could fail outside that handler. A private capture-read boundary now reads both as one observation. Target closure at either stage with an existing pending frame ends acquisition and follows the existing encoder/journal finalization path. Closure before any pending frame and unrelated errors still propagate. The normal public recording API is unchanged.

A deterministic capture source closes immediately after returning a frame and fails when its bounds are read. The regression first showed actual RED with `TARGET_CLOSED` escaping, then GREEN after combining the error boundary. A companion test verifies first-frame closure and unrelated bounds failures are not swallowed.

The native target now paints known top-left, right-edge and bottom-edge color markers. Strict native acceptance requires `GraphicsCaptureSession::IsSupported` and explicitly asserts the selected backend remains **Windows Graphics Capture**; a GDI fallback fails this test instead of counting as WGC coverage. WGC BGRA probes verify the known initial pixels, enlarged-window top-left cropping at fixed output dimensions, and reduced-window right/bottom padding as opaque black. Pixel diagnostics contain at most seven four-byte samples, and native cases remain protected by their child-process watchdog.

Executed after this fix:

- `cargo test --manifest-path rust\Cargo.toml --lib recording::tests --offline -- --nocapture`: **6 passed**, plus the child helper executed by its enclosing watchdog.
- `cargo test --manifest-path rust\Cargo.toml --lib capture::tests --offline -- --nocapture`: **2 passed**, including strict WGC normal/closed/enlarge/shrink cases.
- Scoped `rustfmt --edition 2024 --check` on recording.rs and both changed capture test helpers: passed.
- `cargo clippy --manifest-path rust\Cargo.toml --lib --offline -- -D warnings`: passed.
- Confirmed no other native test process was active, then `cargo test --manifest-path rust\Cargo.toml --lib --offline`: **55 passed, 0 failed, 4 ignored**.
