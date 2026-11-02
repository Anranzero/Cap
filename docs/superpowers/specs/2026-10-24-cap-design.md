# cap design

User directly authorized full implementation and waived an additional design confirmation.

Windows 10 2004+/11 x64, C# .NET 8 WPF; native WGC preferred, GDI desktop fallback, Media Foundation H.264/MP4, streaming GIF89a. No third-party production dependencies, no network features, no installation or elevation. Portable self-contained single-file dist/cap.exe.

Modules: Core owns physical pixel geometry and recording contracts; Capture owns monitor/window discovery and native capture; Annotation owns a frozen desktop selection/editor and pin windows; Recording owns frame pacing, cursor/coordinate/click composition and video/GIF writers; System owns settings, hotkeys and startup; App owns tray and recording/settings/preview orchestration.

All capture bounds are physical pixels including negative desktop coordinates. UI converts between physical pixels and per-monitor WPF DIPs. Annotations are composited at screenshot pixel resolution. Recording overlay is written into frames before encoding. Pause excludes wall time. Frame queues are bounded. GIF uses streaming encoding instead of holding all frames in memory. Recording failures preserve recoverable files and show actionable errors.

Capture alternatives: WGC is preferred for window capture; GDI is the compatibility fallback for desktop regions across monitors. DXGI Desktop Duplication would require additional GPU/adapter handling and is not needed as another fallback. Encoding alternatives: system MF avoids an FFmpeg redistribution; a native streaming GIF writer avoids a large dependency and unbounded frame memory.

Validate each library with builds, geometry/settings/encoding with automated checks, real capture/MP4/GIF with integration smoke tests, then publish and launch the independent EXE. No browser validation. Document hardware, protected-content, secure-desktop, high-FPS and mixed-DPI verification limits.
