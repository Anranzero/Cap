use crate::pixels::Frame;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingFormat {
    Mp4,
    Gif,
}
impl Serialize for RecordingFormat {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(if *self == Self::Mp4 { 0 } else { 1 })
    }
}
impl<'de> Deserialize<'de> for RecordingFormat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match u8::deserialize(d)? {
            0 => Ok(Self::Mp4),
            1 => Ok(Self::Gif),
            _ => Err(serde::de::Error::custom("Unsupported recording format")),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct JournalHeader {
    pub version: u32,
    pub output_path: String,
    pub created_utc: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: RecordingFormat,
}
pub struct JournalWriter {
    tx: Option<std::sync::mpsc::SyncSender<(Vec<u8>, i64, i64)>>,
    worker: Option<std::thread::JoinHandle<Result<(), String>>>,
    header: JournalHeader,
    last_end: i64,
}
pub struct JournalReader {
    file: File,
    pub header: JournalHeader,
    last_end: i64,
}
pub struct JournalFrame {
    pub frame: Frame,
    pub time: i64,
    pub duration: i64,
}
pub fn inspect(path: &Path) -> Result<JournalHeader, String> {
    Ok(JournalReader::open(path)?.header)
}
#[derive(Clone, Debug)]
pub struct RecoveryCandidate {
    pub journal_path: PathBuf,
    pub header: JournalHeader,
}
pub fn find_candidates(directory: &Path) -> Result<Vec<RecoveryCandidate>, String> {
    let mut found = Vec::new();
    if !directory.exists() {
        return Ok(found);
    }
    for entry in
        std::fs::read_dir(directory).map_err(|e| format!("Reading recovery directory: {e}"))?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("capjournal"))
        {
            match inspect(&path) {
                Ok(header) => found.push(RecoveryCandidate {
                    journal_path: path,
                    header,
                }),
                Err(e) => eprintln!("Cannot inspect recovery journal {}: {e}", path.display()),
            }
        }
    }
    Ok(found)
}
pub fn recover(
    journal_path: &Path,
    output_path: &Path,
) -> Result<crate::recording::RecordingResult, String> {
    recover_with_cancellation(
        journal_path,
        output_path,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
pub fn recover_with_cancellation(
    journal_path: &Path,
    output_path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<crate::recording::RecordingResult, String> {
    check_recovery_cancellation(cancel)?;
    if output_path.exists() {
        return Err("Recovery output already exists".into());
    }
    let mut reader = JournalReader::open(journal_path)?;
    let header = reader.header.clone();
    let ext = if header.format == RecordingFormat::Mp4 {
        "mp4"
    } else {
        "gif"
    };
    if !output_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    {
        return Err("Recovery output extension differs from journal format".into());
    }
    let parent = output_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    check_free_space(parent)?;
    static RECOVERY_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let sequence = RECOVERY_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = PathBuf::from(format!(
        "{}.{}.{nonce}.{sequence}.recovering.{ext}",
        output_path.display(),
        std::process::id()
    ));
    let result = (|| {
        let mut encoder = crate::recording::FrameEncoder::new(
            &temporary,
            header.format,
            header.width,
            header.height,
            header.fps,
        )?;
        let mut count = 0;
        let mut end = 0;
        loop {
            check_recovery_cancellation(cancel)?;
            let Some(frame) = reader.read_frame()? else {
                break;
            };
            check_recovery_cancellation(cancel)?;
            encoder.write(&frame.frame, frame.time, frame.duration)?;
            end = frame
                .time
                .checked_add(frame.duration)
                .ok_or("Recovery duration overflow")?;
            count += 1;
        }
        if count == 0 {
            return Err("Journal contains no committed frames".into());
        }
        check_recovery_cancellation(cancel)?;
        encoder.finish()?;
        drop(encoder);
        check_recovery_cancellation(cancel)?;
        publish(&temporary, output_path)?;
        Ok(crate::recording::RecordingResult {
            file_path: output_path.into(),
            duration: crate::recording::duration_from_ticks(end),
            frame_count: count,
            warning: None,
        })
    })();
    if result.is_err() && temporary.exists() {
        if let Err(e) = std::fs::remove_file(&temporary) {
            eprintln!(
                "Removing interrupted recovery output {}: {e}",
                temporary.display()
            )
        }
    }
    result
}
fn check_recovery_cancellation(cancel: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err("Recovery canceled".into())
    } else {
        Ok(())
    }
}
pub fn check_free_space(path: &Path) -> Result<(), String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let parent = if absolute.is_file() {
        absolute.parent().unwrap_or(&absolute)
    } else {
        &absolute
    };
    let p = crate::media::wide(parent);
    let mut available = 0u64;
    if unsafe {
        GetDiskFreeSpaceExW(
            p.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "Checking recording free space: {}",
            std::io::Error::last_os_error()
        ));
    }
    if available < 256 * 1024 * 1024 {
        return Err("Recording stopped: less than 256 MiB free disk space. Completed frames remain in the .capjournal recovery file".into());
    }
    Ok(())
}
pub(crate) fn publish(partial: &Path, output: &Path) -> Result<(), String> {
    let a = crate::media::wide(partial);
    let b = crate::media::wide(output);
    if unsafe { MoveFileW(a.as_ptr(), b.as_ptr()) } == 0 {
        return Err(format!(
            "Publishing completed recording without replacing existing files: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}
pub(crate) fn utc_now() -> String {
    let mut t = SystemTime::default();
    unsafe { GetSystemTime(&mut t) }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.milliseconds
    )
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDiskFreeSpaceExW(
        path: *const u16,
        available: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
    fn MoveFileW(from: *const u16, to: *const u16) -> i32;
    fn GetSystemTime(t: *mut SystemTime);
}
impl JournalWriter {
    pub fn new(path: &Path, header: JournalHeader) -> Result<Self, String> {
        validate_header(&header)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("Creating recovery journal: {e}"))?;
        let json =
            serde_json::to_vec(&header).map_err(|e| format!("Serializing recovery header: {e}"))?;
        if json.len() > 65536 {
            return Err("Recovery header exceeds supported length".into());
        }
        file.write_all(b"CAPJ0001")
            .and_then(|_| file.write_all(&(json.len() as i32).to_le_bytes()))
            .and_then(|_| file.write_all(&json))
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Writing recovery journal header: {e}"))?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<(Vec<u8>, i64, i64)>(2);
        let journal_path = path.to_path_buf();
        let worker = std::thread::Builder::new()
            .name("cap-journal".into())
            .spawn(move || -> Result<(), String> {
                let mut file = file;
                let mut last_flush = std::time::Instant::now();
                for (pixels, time, duration) in rx {
                    let mut deflater =
                        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                    deflater
                        .write_all(&pixels)
                        .map_err(|e| format!("Compressing recovery pixels: {e}"))?;
                    let compressed = deflater
                        .finish()
                        .map_err(|e| format!("Completing recovery compression: {e}"))?;
                    let length = i32::try_from(compressed.len())
                        .map_err(|_| "Recovery compressed frame is too large")?;
                    file.write_all(&length.to_le_bytes())
                        .and_then(|_| file.write_all(&time.to_le_bytes()))
                        .and_then(|_| file.write_all(&duration.to_le_bytes()))
                        .and_then(|_| file.write_all(&compressed))
                        .and_then(|_| file.write_all(&0xca91f00du32.to_le_bytes()))
                        .map_err(|e| format!("Writing recovery frame: {e}"))?;
                    if last_flush.elapsed() >= std::time::Duration::from_secs(1) {
                        file.flush()
                            .and_then(|_| file.sync_all())
                            .map_err(|e| format!("Flushing recovery frames: {e}"))?;
                        last_flush = std::time::Instant::now();
                        check_free_space(
                            journal_path
                                .parent()
                                .filter(|p| !p.as_os_str().is_empty())
                                .unwrap_or(Path::new(".")),
                        )?;
                    }
                }
                file.flush()
                    .and_then(|_| file.sync_all())
                    .map_err(|e| format!("Flushing committed recovery frames: {e}"))
            })
            .map_err(|e| format!("Starting recovery journal thread: {e}"))?;
        Ok(Self {
            tx: Some(tx),
            worker: Some(worker),
            header,
            last_end: 0,
        })
    }
    pub fn write(&mut self, frame: &Frame, time: i64, duration: i64) -> Result<(), String> {
        frame.validate()?;
        if frame.width != self.header.width
            || frame.height != self.header.height
            || time < self.last_end
            || duration <= 0
        {
            return Err("Recovery frame dimensions or timeline are invalid".into());
        }
        let end = time
            .checked_add(duration)
            .ok_or("Recovery frame duration overflows")?;
        self.last_end = end;
        // Bounded queue: when the writer falls behind we drop recovery frames rather than
        // stalling the capture loop (the video itself is unaffected).
        let pixels = frame.pixels.clone();
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.try_send((pixels, time, duration));
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        self.tx = None;
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| "Recovery journal thread panicked".to_string())?,
            None => Ok(()),
        }
    }
}
impl JournalReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("Opening recovery journal: {e}"))?;
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)
            .map_err(|e| format!("Reading journal signature: {e}"))?;
        if &magic != b"CAPJ0001" {
            return Err("Invalid Cap recovery journal signature".into());
        }
        let mut size = [0u8; 4];
        file.read_exact(&mut size)
            .map_err(|e| format!("Reading recovery header length: {e}"))?;
        let size = i32::from_le_bytes(size);
        if !(2..=65536).contains(&size) {
            return Err("Invalid recovery header length".into());
        }
        let mut json = vec![0u8; size as usize];
        file.read_exact(&mut json)
            .map_err(|e| format!("Reading recovery header: {e}"))?;
        let header: JournalHeader = serde_json::from_slice(&json)
            .map_err(|e| format!("Invalid recovery JSON header: {e}"))?;
        validate_header(&header)?;
        Ok(Self {
            file,
            header,
            last_end: 0,
        })
    }
    pub fn read_frame(&mut self) -> Result<Option<JournalFrame>, String> {
        let remaining = self
            .file
            .metadata()
            .map_err(|e| e.to_string())?
            .len()
            .saturating_sub(self.file.stream_position().map_err(|e| e.to_string())?);
        if remaining < 20 {
            return Ok(None);
        }
        let mut record = [0u8; 20];
        self.file
            .read_exact(&mut record)
            .map_err(|e| format!("Reading recovery frame metadata: {e}"))?;
        let length = i32::from_le_bytes(record[0..4].try_into().unwrap());
        let time = i64::from_le_bytes(record[4..12].try_into().unwrap());
        let duration = i64::from_le_bytes(record[12..20].try_into().unwrap());
        let raw_length = self.header.width as usize * self.header.height as usize * 4;
        if length <= 0
            || length as usize > raw_length + raw_length / 8 + 65536
            || time < self.last_end
            || duration <= 0
        {
            return Err("Corrupt recovery frame dimensions, compressed length, or timeline".into());
        }
        let end = time
            .checked_add(duration)
            .ok_or("Recovery frame duration overflow")?;
        if remaining - 20 < length as u64 + 4 {
            return Ok(None);
        }
        let mut compressed = vec![0; length as usize];
        self.file
            .read_exact(&mut compressed)
            .map_err(|e| format!("Reading compressed recovery pixels: {e}"))?;
        let mut commit = [0u8; 4];
        self.file
            .read_exact(&mut commit)
            .map_err(|e| e.to_string())?;
        if u32::from_le_bytes(commit) != 0xca91f00d {
            return Err("Recovery frame commit marker is corrupt".into());
        }
        let mut decoder = flate2::read::DeflateDecoder::new(compressed.as_slice());
        let mut pixels = Vec::with_capacity(raw_length);
        (&mut decoder)
            .take(raw_length as u64 + 1)
            .read_to_end(&mut pixels)
            .map_err(|e| format!("Inflating recovery pixels: {e}"))?;
        if pixels.len() != raw_length || decoder.total_in() != length as u64 {
            return Err(
                "Recovery pixels do not match declared dimensions or DEFLATE length".into(),
            );
        }
        let frame = Frame::new(self.header.width, self.header.height, pixels)?;
        self.last_end = end;
        Ok(Some(JournalFrame {
            frame,
            time,
            duration,
        }))
    }
}
fn validate_header(header: &JournalHeader) -> Result<(), String> {
    if header.version != 1
        || header.width == 0
        || header.height == 0
        || header.width > 8192
        || header.height > 8192
        || header.width as u64 * header.height as u64 > 33_554_432
        || header.fps == 0
        || header.fps > 60
    {
        return Err("Unsupported recovery version, dimensions or frame rate".into());
    }
    Ok(())
}
impl Drop for JournalWriter {
    fn drop(&mut self) {
        if let Err(e) = self.finish() {
            eprintln!("Recovery journal close failed: {e}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cap-rust-{}-{}", std::process::id(), name))
    }
    fn header() -> JournalHeader {
        JournalHeader {
            version: 1,
            output_path: "legacy.gif".into(),
            created_utc: "2026-10-27T00:00:00Z".into(),
            width: 2,
            height: 2,
            fps: 10,
            format: RecordingFormat::Gif,
        }
    }
    #[test]
    fn journal_roundtrip_and_truncated_tail() {
        let p = path("journal-roundtrip.capjournal");
        let _ = std::fs::remove_file(&p);
        let frame = Frame::new(2, 2, vec![93; 16]).unwrap();
        {
            let mut w = JournalWriter::new(&p, header()).unwrap();
            w.write(&frame, 0, 1_000_000).unwrap();
            w.finish().unwrap();
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(&[7, 0, 0]).unwrap();
        drop(f);
        let mut r = JournalReader::open(&p).unwrap();
        let a = r.read_frame().unwrap().unwrap();
        assert_eq!(a.frame.pixels, frame.pixels);
        assert_eq!(a.duration, 1_000_000);
        assert!(r.read_frame().unwrap().is_none());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn rejects_malicious_header_length() {
        let p = path("journal-badlen.capjournal");
        std::fs::write(
            &p,
            [b"CAPJ0001".as_slice(), &i32::MAX.to_le_bytes()].concat(),
        )
        .unwrap();
        assert!(JournalReader::open(&p).is_err());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn reads_dotnet_raw_deflate_fixture() {
        let p = path("journal-legacy.capjournal");
        let json=br#"{"Version":1,"OutputPath":"legacy.gif","CreatedUtc":"2026-10-27T00:00:00Z","Width":2,"Height":2,"Fps":10,"Format":1}"#;
        // Raw RFC1951 stored block, generated independently of the Rust compressor.
        let data = [
            1, 16, 0, 239, 255, 0, 1, 2, 255, 3, 4, 5, 255, 6, 7, 8, 255, 9, 10, 11, 255,
        ];
        let mut f = File::create(&p).unwrap();
        f.write_all(b"CAPJ0001").unwrap();
        f.write_all(&(json.len() as i32).to_le_bytes()).unwrap();
        f.write_all(json).unwrap();
        f.write_all(&(data.len() as i32).to_le_bytes()).unwrap();
        f.write_all(&0i64.to_le_bytes()).unwrap();
        f.write_all(&100i64.to_le_bytes()).unwrap();
        f.write_all(&data).unwrap();
        f.write_all(&0xca91f00du32.to_le_bytes()).unwrap();
        drop(f);
        let mut r = JournalReader::open(&p).unwrap();
        assert_eq!(r.header.format, RecordingFormat::Gif);
        assert_eq!(r.read_frame().unwrap().unwrap().frame.pixels, &data[5..]);
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn corrupt_record_length_is_rejected_before_allocation() {
        let p = path("journal-record-badlen.capjournal");
        let json = serde_json::to_vec(&header()).unwrap();
        let mut bytes = b"CAPJ0001".to_vec();
        bytes.extend((json.len() as i32).to_le_bytes());
        bytes.extend(json);
        bytes.extend(i32::MAX.to_le_bytes());
        bytes.extend(0i64.to_le_bytes());
        bytes.extend(10i64.to_le_bytes());
        std::fs::write(&p, bytes).unwrap();
        let mut r = JournalReader::open(&p).unwrap();
        assert!(r.read_frame().is_err());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn invalid_enum_and_dimension_header_is_rejected() {
        let p = path("journal-dimensions.capjournal");
        let mut h = header();
        h.width = 8193;
        let json = serde_json::to_vec(&h).unwrap();
        let mut bytes = b"CAPJ0001".to_vec();
        bytes.extend((json.len() as i32).to_le_bytes());
        bytes.extend(json);
        std::fs::write(&p, bytes).unwrap();
        assert!(JournalReader::open(&p).is_err());
        std::fs::remove_file(p).unwrap();
        assert!(serde_json::from_str::<RecordingFormat>("2").is_err());
    }
    #[test]
    fn recovery_reencodes_committed_gif_and_keeps_journal() {
        let journal = path("recover-roundtrip.capjournal");
        let output = path("recover-roundtrip.gif");
        let _ = std::fs::remove_file(&journal);
        let _ = std::fs::remove_file(&output);
        {
            let mut w = JournalWriter::new(&journal, header()).unwrap();
            w.write(
                &Frame::new(2, 2, [0, 0, 255, 255].repeat(4)).unwrap(),
                0,
                1_000_000,
            )
            .unwrap();
            w.finish().unwrap();
        }
        let result = recover(&journal, &output).unwrap();
        assert_eq!(result.frame_count, 1);
        assert!(journal.exists());
        let decoded = crate::media::load_image(&output).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert!(decoded.pixels[2] > 200);
        std::fs::remove_file(journal).unwrap();
        std::fs::remove_file(output).unwrap();
    }
    #[test]
    fn completed_publication_never_replaces_existing_file() {
        let partial = path("publish.partial");
        let output = path("publish-existing.gif");
        std::fs::write(&partial, b"new recording").unwrap();
        std::fs::write(&output, b"existing content").unwrap();
        assert!(publish(&partial, &output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing content");
        assert!(partial.exists());
        std::fs::remove_file(partial).unwrap();
        std::fs::remove_file(output).unwrap();
    }
    #[test]
    fn concurrent_recovery_cannot_delete_another_attempts_partial_file() {
        let journal = path("concurrent.capjournal");
        let output = path("concurrent.gif");
        let _ = std::fs::remove_file(&journal);
        let _ = std::fs::remove_file(&output);
        let mut h = header();
        h.width = 512;
        h.height = 256;
        {
            let mut writer = JournalWriter::new(&journal, h).unwrap();
            for i in 0..4 {
                writer
                    .write(
                        &Frame::blank(512, 256, [i as u8 * 50, 80, 180, 255]).unwrap(),
                        i * 1_000_000,
                        1_000_000,
                    )
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        let gate = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let gate = gate.clone();
            let j = journal.clone();
            let o = output.clone();
            workers.push(std::thread::spawn(move || {
                gate.wait();
                recover(&j, &o)
            }));
        }
        gate.wait();
        let successes = workers
            .into_iter()
            .filter_map(|w| w.join().unwrap().ok())
            .count();
        assert_eq!(
            successes, 1,
            "Exactly one recovery must publish the requested output"
        );
        assert_eq!(&std::fs::read(&output).unwrap()[..6], b"GIF89a");
        std::fs::remove_file(journal).unwrap();
        std::fs::remove_file(output).unwrap();
    }
    fn cancellation_directory(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cap-rust-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        dir
    }
    fn remove_cancellation_directory(dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn pre_cancelled_recovery_preserves_journal_and_destination() {
        use std::sync::atomic::AtomicBool;
        let dir = cancellation_directory("pre-cancel");
        let journal = dir.join("source.capjournal");
        let output = dir.join("output.gif");
        {
            let mut writer = JournalWriter::new(&journal, header()).unwrap();
            writer
                .write(&Frame::blank(2, 2, [0, 0, 255, 255]).unwrap(), 0, 1_000_000)
                .unwrap();
            writer.finish().unwrap();
        }
        let before = std::fs::read(&journal).unwrap();
        let cancelled = AtomicBool::new(true);
        let result = recover_with_cancellation(&journal, &output, &cancelled);
        let was_published = output.exists();
        if was_published {
            std::fs::remove_file(&output).unwrap();
        }
        std::fs::write(&output, b"existing destination").unwrap();
        let existing_result = recover_with_cancellation(&journal, &output, &cancelled);
        let source_unchanged = std::fs::read(&journal).unwrap() == before;
        let destination_unchanged = std::fs::read(&output).unwrap() == b"existing destination";
        remove_cancellation_directory(&dir);
        assert!(
            result.is_err(),
            "Pre-cancelled recovery must return cancellation"
        );
        assert!(!was_published, "Pre-cancelled recovery published an output");
        assert!(existing_result.is_err());
        assert!(source_unchanged);
        assert!(destination_unchanged);
    }
    #[test]
    fn mid_loop_cancellation_removes_only_its_own_temporary_output() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let dir = cancellation_directory("mid-cancel");
        let journal = dir.join("source.capjournal");
        let output = dir.join("output.gif");
        let sentinel = dir.join("unrelated-attempt.recovering.gif");
        std::fs::write(&sentinel, b"other attempt").unwrap();
        let mut h = header();
        h.width = 256;
        h.height = 256;
        let mut frame = Frame::blank(256, 256, [0, 0, 0, 255]).unwrap();
        let mut seed = 0x12345678u32;
        for pixel in frame.pixels.chunks_exact_mut(4) {
            for channel in &mut pixel[..3] {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *channel = seed as u8;
            }
        }
        {
            let mut writer = JournalWriter::new(&journal, h).unwrap();
            for i in 0..64 {
                // Cross the RGB332 palette boundary so the encoder must emit
                // earlier frames rather than coalescing all input until finish.
                frame.pixels[0] = if i % 2 == 0 { 0 } else { 255 };
                writer.write(&frame, i * 1_000_000, 1_000_000).unwrap();
            }
            writer.finish().unwrap();
        }
        let before = std::fs::read(&journal).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_journal = journal.clone();
        let worker_output = output.clone();
        let worker = std::thread::spawn(move || {
            recover_with_cancellation(&worker_journal, &worker_output, &worker_cancel)
        });
        let start = std::time::Instant::now();
        let mut observed_encoded_frame = false;
        while !worker.is_finished() && start.elapsed() < std::time::Duration::from_secs(5) {
            observed_encoded_frame =
                std::fs::read_dir(&dir)
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("output.gif.")
                            && File::open(entry.path())
                                .and_then(|file| file.metadata())
                                .is_ok_and(|m| m.len() > 8192)
                    });
            if observed_encoded_frame {
                break;
            }
            std::thread::yield_now();
        }
        cancel.store(true, Ordering::Release);
        let result = worker.join().unwrap();
        let published = output.exists();
        let journal_unchanged = std::fs::read(&journal).unwrap() == before;
        let sentinel_unchanged = std::fs::read(&sentinel).unwrap() == b"other attempt";
        let partial_remains = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("output.gif."));
        remove_cancellation_directory(&dir);
        assert!(
            observed_encoded_frame,
            "Did not observe a live encoded recovery frame"
        );
        assert!(result.is_err(), "Recovery ignored mid-loop cancellation");
        assert!(!published, "Cancelled recovery was published");
        assert!(journal_unchanged);
        assert!(sentinel_unchanged);
        assert!(
            !partial_remains,
            "Cancelled recovery left its temporary output"
        );
    }
}