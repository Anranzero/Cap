# Recording module implementation and verification

## Public API

`Cap.Recording.RecordingSession` implements `IDisposable`:

- Constructor: `RecordingSession(RecordingOptions options)`.
- `State`, `Completion: Task<RecordingResult>`.
- `Start()`, `Pause()`, `Resume()`, `StopAsync(): Task<RecordingResult>`.
- `Progress: EventHandler<RecordingProgress>` and `Failed: EventHandler<Exception>`.

Events run on the recording worker thread. UI consumers must dispatch to their UI thread.
Start initiates background initialization; the first Progress event indicates capture readiness.
Initialization time and paused wall time are excluded from the media timeline. Repeated
StopAsync calls return the same completion task. Dispose requests stop and waits for file
finalization, except when called by the worker's own event handler. A session starts only once.
Closing a captured window finalizes existing frames, including when paused; closing before
the first captured frame reports failure. Paused window closure is checked every 250 ms.

`RecordingRecovery.FindCandidates(string directory)` returns `IReadOnlyList<RecoveryCandidate>`.
Each candidate exposes JournalPath, OriginalOutputPath, CreatedUtc, Width, Height,
FramesPerSecond and Format. Invalid or inaccessible individual journals are logged and skipped.

`RecordingRecovery.RecoverAsync(string journalPath, string outputPath,
CancellationToken cancellationToken = default)` returns a new `RecordingResult`.
It preserves its source journal and never overwrites an existing output. Failed or canceled
recovery removes only its own uniquely named temporary output, allowing retry.

## Encoding and timing

MP4 uses Windows Media Foundation's inbox H.264 encoder and MPEG-4 sink writer. Managed BGRA
frames are converted to top-down, limited-range BT.601 NV12. Supported requested rates are
15, 24, 30 and 60 FPS. No FFmpeg, downloaded codec, native redistribution or third-party
production package is required. Windows N without Media Feature Pack receives an actionable
initialization error. Device/codec resolution limits may be lower than the API's dimension cap.

GIF is a streaming GIF89a encoder with a 256-color RGB332 global palette, stable screen-aligned
8x8 ordered dithering, 9–12-bit LZW and dictionary resets. Exact palette colors stay exact.
Consecutive equal quantized frames merge; changes encode only their bounding rectangle with
keep-previous disposal. Cumulative centisecond rounding prevents drift at 24/30/60 FPS.
GIF supports requested integer rates 1–60. Individual viewers may impose minimum frame delays;
therefore actual 60 FPS playback is viewer-dependent. GIF is lossy and is not true-color output.

Frame pacing follows a monotonic active-time slot counter. Late captures skip slots and update
DroppedFrames; the previous sample's duration extends across skipped slots, maintaining elapsed
media time without an unbounded frame queue. Captured-frame counts can exceed physical GIF
image counts because of identical-frame merging. Final duration rounds up by at most one slot.

OutputWidth=0 and OutputHeight=0 retain capture dimensions. Supplying only one dimension keeps
aspect ratio and caps it to the capture dimension to avoid upscaling. Supplying both defines
an explicit output shape, scaled down if it exceeds capture bounds. H.264 rounds to even
dimensions with a minimum of 2x2. Maximum API output is 8192 per side and 33,554,432 total pixels.

## Burned-in overlays

The actual system cursor is sampled with CURSORINFO and physical pointer coordinates.
DrawIconEx renders its native shape through a small top-down DIB, respecting hotspot and
monochrome AND/XOR masks. Cursor pixels become opaque before encoding. A low-level mouse hook
with its own message pump tracks left/right/middle/X button presses between captures and
drives a fading 500 ms yellow pulse. Physical absolute or capture-relative X/Y text follows
the pointer with configured #RRGGBB color and 8–96 pixel font size. Placement uses measured
text size. Very small capture regions may still clip text larger than the region.
All overlays are applied before output resize and encoding, so they appear in exported media.

## Failure and recovery behavior

Recording stages media at `<output>.partial.mp4` or `<output>.partial.gif`, and publishes by
moving to the requested output after encoder finalization and resource release. Successful
completion removes `<output>.capjournal`; failures retain journal and partial media.
Existing final/staging/journal files are not overwritten.

The append-only recovery journal stores JSON metadata and independently Deflate-compressed
BGRA frames with timestamps, durations and commit markers. Memory stays proportional to a
few frames, independent of recording duration. Journal writes flush to disk about once per
second and at graceful disposal. Recovery ignores an incomplete final record and re-encodes
all preceding complete records into a normal MP4/GIF. It does not repair arbitrary corruption.
The last pending capture and roughly the last flush interval can be lost on abrupt termination;
power-loss durability also depends on the filesystem/device. This is recovery, not zero-loss
crash-proof recording.

The journal trades disk and compression CPU for recovery. Static desktop frames compress well;
high-motion/noisy content can approach raw BGRA disk rates (1080p at 15 FPS is approximately
7.46 GB/minute before compression). It has no fixed total-size cap. Recording checks free
space at startup and about once per second and stops below 256 MB; an earlier write failure
also stops and retains committed records. Long/high-resolution performance and available disk
space must be evaluated on target hardware. Journal contents are local unencrypted screen data.

## Actual verification

Command, using the local .NET 8.0.426 SDK:

```powershell
& "$env:LOCALAPPDATA\cap-dev\dotnet\dotnet.exe" run --project tests\Cap.Recording.Tests\Cap.Recording.Tests.csproj -c Release
```

The latest run passed 16,078 assertions covering:

- WPF GIF decode of every pixel in random 256-color data, including LZW width transitions and resets.
- Identical-frame delays, one-pixel delta dimensions/offsets, 60 FPS cumulative timing, and gradient color averages.
- Actual H.264 encode followed by Windows source-reader decode: 30 frames, dimensions, subtype,
  one-second timestamps, red upper half and blue lower half without inversion.
- Committed journal recovery with a truncated final record; mixed malformed/valid candidate files;
  failure followed by recovery retry to the same output path.
- A separate process writing the real journal, forcibly killed without Dispose, followed by
  recovery into an MP4 whose committed frame count, dimensions and subtype decode correctly.
- Native DrawIconEx rendering of a deterministic owned cursor and its hotspot; configured green
  coordinate pixels; timed yellow pulse pixels; exact resized dimensions/color/opaque alpha.
- Real screen capture pause/resume/idempotent stop and cleanup; closing an owned captured window
  while recording and while paused, both resulting in a decodable saved GIF.

The native mouse hook's registration/message-thread/disposal executes in the overlay tests.
Those tests use a deterministic click snapshot for pixel assertions; actual live mouse-click
delivery was not independently asserted. The automated desktop did not reliably give the test
window ownership of the global system cursor, so its bitmap/hotspot test supplies an owned
cursor snapshot directly to the production renderer. Live cursor sampling remains a target
desktop acceptance check. No browser was opened for validation.

No full-length endurance test, multi-hour disk-pressure test, Windows N installation test,
or exhaustive codec/hardware/high-DPI multi-monitor matrix was run. The project builds with
zero compiler warnings. The tests have no third-party test-runner dependencies.

Media Foundation implementation follows the official sink-writer sequence:
https://learn.microsoft.com/en-us/windows/win32/medfound/tutorial--using-the-sink-writer-to-encode-video
