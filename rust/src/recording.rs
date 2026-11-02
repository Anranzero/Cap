pub use crate::recovery::RecordingFormat;
use crate::{
    capture::{self, CaptureSession, CaptureTarget},
    geometry::Rect,
    overlays::{OverlayOptions, OverlayProcessor},
    pixels::Frame,
    recovery::{self, JournalHeader, JournalWriter},
};
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub bounds: Rect,
    pub window_handle: isize,
    pub format: RecordingFormat,
    pub fps: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub output_path: PathBuf,
    pub overlays: OverlayOptions,
}
impl RecordingConfig {
    pub fn new(bounds: Rect, output_path: PathBuf, format: RecordingFormat) -> Self {
        Self {
            bounds,
            window_handle: 0,
            format,
            fps: if format == RecordingFormat::Mp4 {
                30
            } else {
                10
            },
            output_width: 0,
            output_height: 0,
            output_path,
            overlays: OverlayOptions::default(),
        }
    }
    pub fn dimensions(&self) -> Result<(u32, u32), String> {
        let (mut w, mut h) = (self.output_width, self.output_height);
        let (bw, bh) = (self.bounds.width, self.bounds.height);
        if bw == 0 || bh == 0 || bw > 32768 || bh > 32768 {
            return Err("Capture dimensions invalid".into());
        }
        if w == 0 && h == 0 {
            w = bw;
            h = bh
        } else if h == 0 {
            w = w.min(bw);
            h = ((w as f64 * bh as f64 / bw as f64).round() as u32).max(1)
        } else if w == 0 {
            h = h.min(bh);
            w = ((h as f64 * bw as f64 / bh as f64).round() as u32).max(1)
        } else {
            let scale = 1f64.min(bw as f64 / w as f64).min(bh as f64 / h as f64);
            w = ((w as f64 * scale).round() as u32).max(1);
            h = ((h as f64 * scale).round() as u32).max(1)
        }
        if self.format == RecordingFormat::Mp4 {
            w = (w & !1).max(2);
            h = (h & !1).max(2)
        }
        if w > 8192 || h > 8192 || w as u64 * h as u64 > 33_554_432 {
            return Err("Recording exceeds 8192px / 32M pixel limit".into());
        }
        Ok((w, h))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.dimensions()?;
        Rect::new(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            self.bounds.height,
        )?;
        if self.fps == 0
            || self.fps > 60
            || self.format == RecordingFormat::Mp4 && !matches!(self.fps, 15 | 24 | 30 | 60)
        {
            return Err("MP4 supports 15,24,30,60 FPS; GIF 1–60 FPS".into());
        }
        let ext = self
            .output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !ext.eq_ignore_ascii_case(if self.format == RecordingFormat::Mp4 {
            "mp4"
        } else {
            "gif"
        }) {
            return Err("Recording output extension differs from format".into());
        }
        self.overlays.validate()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    Recording,
    Paused,
    Stopping,
    Completed,
    Failed,
}
#[derive(Clone, Debug)]
pub struct RecordingStatus {
    pub state: RecordingState,
    pub duration: Duration,
    pub frame_count: u64,
    pub dropped_frames: u64,
    pub backend: String,
    pub failure: Option<String>,
}
#[derive(Clone, Debug)]
pub struct RecordingResult {
    pub file_path: PathBuf,
    pub duration: Duration,
    pub frame_count: u64,
    pub warning: Option<String>,
}
struct Shared {
    status: RecordingStatus,
    clock: ActiveClock,
    result: Option<Result<RecordingResult, String>>,
}
pub struct RecordingSession {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl RecordingSession {
    pub fn start(config: RecordingConfig) -> Result<Self, String> {
        config.validate()?;
        if config.output_path.exists() {
            return Err("Recording output already exists".into());
        }
        let shared = Arc::new((
            Mutex::new(Shared {
                status: RecordingStatus {
                    state: RecordingState::Recording,
                    duration: Duration::ZERO,
                    frame_count: 0,
                    dropped_frames: 0,
                    backend: "Starting".into(),
                    failure: None,
                },
                clock: ActiveClock::new(),
                result: None,
            }),
            Condvar::new(),
        ));
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("Cap capture encoder".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(config, &worker_shared)
                }))
                .unwrap_or_else(|_| {
                    Err("Recording worker panicked; recovery journal retained".into())
                });
                let (mut s, cv) = (
                    worker_shared.0.lock().unwrap_or_else(|e| e.into_inner()),
                    &worker_shared.1,
                );
                s.clock.pause();
                match &result {
                    Ok(r) => {
                        s.status.state = RecordingState::Completed;
                        s.status.duration = r.duration;
                        s.status.frame_count = r.frame_count
                    }
                    Err(e) => {
                        s.status.state = RecordingState::Failed;
                        s.status.failure = Some(e.clone())
                    }
                }
                s.result = Some(result);
                cv.notify_all();
            })
            .map_err(|e| format!("Starting recording thread: {e}"))?;
        Ok(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn status(&self) -> RecordingStatus {
        let s = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut status = s.status.clone();
        if matches!(
            status.state,
            RecordingState::Recording | RecordingState::Paused
        ) {
            status.duration = s.clock.elapsed()
        }
        status
    }
    pub fn pause(&self) {
        let mut s = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.status.state == RecordingState::Recording {
            s.clock.pause();
            s.status.state = RecordingState::Paused;
            self.shared.1.notify_all();
        }
    }
    pub fn resume(&self) {
        let mut s = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.status.state == RecordingState::Paused {
            s.clock.resume();
            s.status.state = RecordingState::Recording;
            self.shared.1.notify_all();
        }
    }
    pub fn request_stop(&self) {
        let mut s = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            s.status.state,
            RecordingState::Recording | RecordingState::Paused
        ) {
            s.clock.pause();
            s.status.state = RecordingState::Stopping;
            self.shared.1.notify_all();
        }
    }
    pub fn result(&self) -> Option<Result<RecordingResult, String>> {
        self.shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .result
            .clone()
    }
    pub fn stop(&self) -> Result<RecordingResult, String> {
        self.request_stop();
        if let Some(worker) = self
            .worker
            .lock()
            .map_err(|_| "Recording worker lock poisoned")?
            .take()
        {
            if worker.join().is_err() {
                return Err("Recording worker panicked; recovery journal retained".into());
            }
        }
        let mut s = self
            .shared
            .0
            .lock()
            .map_err(|_| "Recording state lock poisoned")?;
        while s.result.is_none() {
            s = self
                .shared
                .1
                .wait(s)
                .map_err(|_| "Recording completion lock poisoned")?;
        }
        s.result.clone().unwrap()
    }
}
impl Drop for RecordingSession {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            eprintln!("Recording stopped with error: {e}")
        }
    }
}
struct ActiveClock {
    elapsed: Duration,
    running: Option<Instant>,
}
impl ActiveClock {
    fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            running: Some(Instant::now()),
        }
    }
    fn pause(&mut self) {
        if let Some(t) = self.running.take() {
            self.elapsed += t.elapsed()
        }
    }
    fn resume(&mut self) {
        if self.running.is_none() {
            self.running = Some(Instant::now())
        }
    }
    fn elapsed(&self) -> Duration {
        self.elapsed + self.running.map(|i| i.elapsed()).unwrap_or_default()
    }
}
pub(crate) enum FrameEncoder {
    Mp4(crate::media::Mp4Writer),
    Gif(crate::gif::GifEncoder<BufWriter<File>>),
}
impl FrameEncoder {
    pub(crate) fn new(
        path: &Path,
        format: RecordingFormat,
        w: u32,
        h: u32,
        fps: u32,
    ) -> Result<Self, String> {
        match format {
            RecordingFormat::Mp4 => Ok(Self::Mp4(crate::media::Mp4Writer::new(path, w, h, fps)?)),
            RecordingFormat::Gif => {
                let f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| format!("Creating GIF output: {e}"))?;
                Ok(Self::Gif(crate::gif::GifEncoder::new(
                    BufWriter::new(f),
                    w,
                    h,
                    fps,
                )?))
            }
        }
    }
    pub(crate) fn write(&mut self, f: &Frame, t: i64, d: i64) -> Result<(), String> {
        match self {
            Self::Mp4(w) => w.write(f, t, d),
            Self::Gif(w) => w.write(f, t, d),
        }
    }
    pub(crate) fn finish(&mut self) -> Result<(), String> {
        match self {
            Self::Mp4(w) => w.finish(),
            Self::Gif(w) => w.finish(),
        }
    }
}
fn ticks(d: Duration) -> i64 {
    (d.as_nanos() / 100).min(i64::MAX as u128) as i64
}
pub(crate) fn duration_from_ticks(ticks: i64) -> Duration {
    let ticks = ticks.max(0) as u64;
    Duration::new(ticks / 10_000_000, ((ticks % 10_000_000) * 100) as u32)
}
fn slot_time(slot: i64, fps: u32) -> Result<i64, String> {
    slot.checked_mul(10_000_000)
        .map(|t| t / fps as i64)
        .ok_or_else(|| "Recording duration overflow".into())
}
trait CaptureRead {
    fn capture_frame(&mut self) -> Result<Frame, String>;
    fn capture_bounds(&self) -> Result<Rect, String>;
}
impl CaptureRead for CaptureSession {
    fn capture_frame(&mut self) -> Result<Frame, String> {
        self.capture()
    }
    fn capture_bounds(&self) -> Result<Rect, String> {
        self.bounds()
    }
}
fn capture_for_recording(
    source: &mut impl CaptureRead,
    has_pending: bool,
) -> Result<Option<(Frame, Rect)>, String> {
    let observation = (|| {
        let frame = source.capture_frame()?;
        let bounds = source.capture_bounds()?;
        Ok((frame, bounds))
    })();
    match observation {
        Ok(observation) => Ok(Some(observation)),
        Err(error) if error == capture::TARGET_CLOSED && has_pending => Ok(None),
        Err(error) => Err(error),
    }
}
fn run(
    config: RecordingConfig,
    shared: &Arc<(Mutex<Shared>, Condvar)>,
) -> Result<RecordingResult, String> {
    let _com = crate::media::ComGuard::new()?;
    let (w, h) = config.dimensions()?;
    let parent = config
        .output_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| format!("Creating recording directory: {e}"))?;
    recovery::check_free_space(parent)?;
    let journal_path = PathBuf::from(format!("{}.capjournal", config.output_path.display()));
    let partial_path = PathBuf::from(format!(
        "{}.partial.{}",
        config.output_path.display(),
        if config.format == RecordingFormat::Mp4 {
            "mp4"
        } else {
            "gif"
        }
    ));
    let direct = config.format == RecordingFormat::Gif
        && config.window_handle == 0
        && !capture::monitors()?
            .iter()
            .any(|m| m.bounds == config.bounds);
    let mut source = CaptureSession::new(CaptureTarget {
        bounds: config.bounds,
        window_handle: config.window_handle,
        prefer_graphics_capture: !direct,
    })?;
    let mut processor = OverlayProcessor::new(config.overlays.clone())?;
    let header = JournalHeader {
        version: 1,
        output_path: config.output_path.to_string_lossy().into_owned(),
        created_utc: recovery::utc_now(),
        width: w,
        height: h,
        fps: config.fps,
        format: config.format,
    };
    let mut journal = JournalWriter::new(&journal_path, header)?;
    let mut encoder = FrameEncoder::new(&partial_path, config.format, w, h, config.fps)?;
    {
        let mut s = shared
            .0
            .lock()
            .map_err(|_| "Recording state lock poisoned")?;
        s.clock = ActiveClock::new();
        if s.status.state != RecordingState::Recording {
            s.clock.pause();
        }
        s.status.backend = source.backend_name().into();
    }
    let (mut pending, mut pending_slot, mut next_slot, mut count, mut dropped): (
        Option<Frame>,
        i64,
        i64,
        u64,
        u64,
    ) = (None, 0, 0, 0, 0);
    loop {
        let s = shared
            .0
            .lock()
            .map_err(|_| "Recording state lock poisoned")?;
        let state = s.status.state;
        let elapsed = ticks(s.clock.elapsed());
        if state == RecordingState::Stopping {
            break;
        }
        if state == RecordingState::Paused {
            if config.window_handle != 0 && !capture::is_window(config.window_handle) {
                break;
            }
            let (_guard, _) = shared
                .1
                .wait_timeout(s, Duration::from_millis(250))
                .map_err(|_| "Recording pause lock poisoned")?;
            continue;
        }
        let mut slot = elapsed.saturating_mul(config.fps as i64) / 10_000_000;
        if slot < next_slot {
            let wait = ((slot_time(next_slot, config.fps)? - elapsed) / 10_000).clamp(1, 50) as u64;
            let (_guard, _) = shared
                .1
                .wait_timeout(s, Duration::from_millis(wait))
                .map_err(|_| "Recording scheduler lock poisoned")?;
            continue;
        }
        drop(s);
        let Some((captured, bounds)) = capture_for_recording(&mut source, pending.is_some())?
        else {
            break;
        };
        let frame = processor.process(captured, bounds, w, h)?;
        let mut s = shared
            .0
            .lock()
            .map_err(|_| "Recording state lock poisoned")?;
        if s.status.state != RecordingState::Recording {
            continue;
        }
        if let Some(previous) = pending.take() {
            let t = slot_time(pending_slot, config.fps)?;
            let d = slot_time(slot, config.fps)? - t;
            drop(s);
            journal.write(&previous, t, d)?;
            encoder.write(&previous, t, d)?;
            dropped += slot.saturating_sub(next_slot) as u64;
            s = shared
                .0
                .lock()
                .map_err(|_| "Recording state lock poisoned")?;
        } else {
            slot = 0
        }
        pending = Some(frame);
        pending_slot = slot;
        next_slot = slot + 1;
        count += 1;
        s.status.frame_count = count;
        s.status.dropped_frames = dropped;
        s.status.backend = source.backend_name().into();
    }
    if pending.is_none() {
        let f = source.capture()?;
        pending = Some(processor.process(f, source.bounds()?, w, h)?);
        pending_slot = 0;
        count = 1;
    }
    let elapsed = ticks(
        shared
            .0
            .lock()
            .map_err(|_| "Recording state lock poisoned")?
            .clock
            .elapsed(),
    );
    let final_slot = (pending_slot + 1)
        .max((elapsed.saturating_mul(config.fps as i64) + 9_999_999) / 10_000_000);
    let t = slot_time(pending_slot, config.fps)?;
    let end = slot_time(final_slot, config.fps)?;
    let f = pending.unwrap();
    journal.write(&f, t, end - t)?;
    encoder.write(&f, t, end - t)?;
    encoder.finish()?;
    journal.finish()?;
    drop(encoder);
    drop(journal);
    recovery::publish(&partial_path, &config.output_path)?;
    let warning = std::fs::remove_file(&journal_path)
        .err()
        .map(|e| format!("Recording saved, but journal could not be removed: {e}"));
    Ok(RecordingResult {
        file_path: config.output_path,
        duration: duration_from_ticks(end),
        frame_count: count,
        warning,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    struct ClosingBetweenReads {
        closed: std::cell::Cell<bool>,
        bounds_error: String,
    }
    impl CaptureRead for ClosingBetweenReads {
        fn capture_frame(&mut self) -> Result<Frame, String> {
            self.closed.set(true);
            Frame::blank(2, 2, [0, 0, 255, 255])
        }
        fn capture_bounds(&self) -> Result<Rect, String> {
            assert!(self.closed.get());
            Err(self.bounds_error.clone())
        }
    }
    #[test]
    fn close_between_capture_and_bounds_finalizes_existing_pending_frame() {
        let mut source = ClosingBetweenReads {
            closed: std::cell::Cell::new(false),
            bounds_error: capture::TARGET_CLOSED.into(),
        };
        assert!(capture_for_recording(&mut source, true).unwrap().is_none());
    }
    #[test]
    fn capture_boundary_preserves_first_frame_and_other_errors() {
        let mut source = ClosingBetweenReads {
            closed: std::cell::Cell::new(false),
            bounds_error: capture::TARGET_CLOSED.into(),
        };
        assert_eq!(
            capture_for_recording(&mut source, false).unwrap_err(),
            capture::TARGET_CLOSED
        );
        source.bounds_error = "Cannot read selected window bounds".into();
        assert_eq!(
            capture_for_recording(&mut source, true).unwrap_err(),
            source.bounds_error
        );
    }
    #[test]
    fn target_closure_publishes_completed_frames() {
        for case in ["recording", "paused"] {
            crate::capture::native_tests::run_child_case(
                "recording::tests::target_closure_child",
                Some(case),
            );
        }
    }
    #[test]
    #[ignore = "Recording child scenario; parent applies an 8-second watchdog"]
    fn target_closure_child() {
        let path = std::env::temp_dir().join(format!("cap-rust-target-{}.gif", std::process::id()));
        for p in [
            &path,
            &PathBuf::from(format!("{}.capjournal", path.display())),
            &PathBuf::from(format!("{}.partial.gif", path.display())),
        ] {
            let _ = std::fs::remove_file(p);
        }
        let mut target = crate::capture::native_tests::NativeTarget::new();
        let bounds = capture::window_bounds(target.handle).unwrap();
        let mut config = RecordingConfig::new(bounds, path.clone(), RecordingFormat::Gif);
        config.window_handle = target.handle;
        config.overlays = OverlayOptions {
            show_cursor: false,
            highlight_clicks: false,
            show_coordinates: false,
            ..Default::default()
        };
        let session = RecordingSession::start(config).unwrap();
        let start = Instant::now();
        while session.status().frame_count == 0
            && session.result().is_none()
            && start.elapsed() < Duration::from_secs(4)
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(session.status().frame_count > 0, "{:?}", session.status());
        if std::env::var("CAP_RUST_WGC_CASE").unwrap() == "paused" {
            session.pause();
        }
        target.close();
        let start = Instant::now();
        while session.result().is_none() && start.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            session.result().is_some(),
            "Target closure did not complete recording: {:?}",
            session.status()
        );
        let result = session.stop().unwrap();
        assert!(result.frame_count > 0);
        assert!(path.exists());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn paused_clock_excludes_paused_duration() {
        let mut c = ActiveClock::new();
        thread::sleep(Duration::from_millis(15));
        c.pause();
        let before = c.elapsed();
        thread::sleep(Duration::from_millis(30));
        assert_eq!(c.elapsed(), before);
        c.resume();
        thread::sleep(Duration::from_millis(15));
        c.pause();
        assert!(c.elapsed() >= before + Duration::from_millis(10));
    }
    #[test]
    fn mp4_dimensions_are_even_and_preserve_aspect() {
        let mut c = RecordingConfig::new(
            Rect::new(-20, 0, 1919, 1079).unwrap(),
            "test.mp4".into(),
            RecordingFormat::Mp4,
        );
        c.output_width = 640;
        assert_eq!(c.dimensions().unwrap(), (640, 360));
    }
    #[test]
    fn recording_pause_and_repeat_stop() {
        let path =
            std::env::temp_dir().join(format!("cap-rust-session-{}.gif", std::process::id()));
        for p in [
            &path,
            &PathBuf::from(format!("{}.capjournal", path.display())),
            &PathBuf::from(format!("{}.partial.gif", path.display())),
        ] {
            let _ = std::fs::remove_file(p);
        }
        let bounds = capture::virtual_bounds().unwrap();
        let mut config = RecordingConfig::new(
            Rect::new(bounds.x, bounds.y, 64, 64).unwrap(),
            path.clone(),
            RecordingFormat::Gif,
        );
        config.overlays = OverlayOptions {
            show_cursor: false,
            highlight_clicks: false,
            show_coordinates: false,
            ..Default::default()
        };
        let session = RecordingSession::start(config).unwrap();
        thread::sleep(Duration::from_millis(220));
        session.pause();
        let before = session.status().duration;
        thread::sleep(Duration::from_millis(120));
        assert_eq!(session.status().duration, before);
        session.resume();
        thread::sleep(Duration::from_millis(120));
        let first = session.stop().unwrap();
        let second = session.stop().unwrap();
        assert_eq!(first.frame_count, second.frame_count);
        assert!(first.frame_count >= 2);
        assert!(path.exists());
        assert!(!PathBuf::from(format!("{}.capjournal", path.display())).exists());
        std::fs::remove_file(path).unwrap();
    }
}
