use super::{ComGuard, wide};
use crate::pixels::Frame;
use std::path::Path;
use windows::{Win32::Media::MediaFoundation::*, core::PCWSTR};
pub struct Mp4Writer {
    writer: Option<IMFSinkWriter>,
    stream: u32,
    width: u32,
    height: u32,
    _runtime: Runtime,
    finished: bool,
}
struct Runtime {
    _com: ComGuard,
}
impl Runtime {
    fn new() -> Result<Self, String> {
        let com = ComGuard::new()?;
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
            .map_err(|e| format!("Media Foundation unavailable: {e}"))?;
        Ok(Self { _com: com })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Err(e) = unsafe { MFShutdown() } {
            eprintln!("Media Foundation shutdown: {e}")
        }
    }
}
fn video_type(
    subtype: &windows::core::GUID,
    width: u32,
    height: u32,
    fps: u32,
) -> windows::core::Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
        t.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        Ok(t)
    }
}
impl Mp4Writer {
    pub fn new(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        if width < 2
            || height < 2
            || width % 2 != 0
            || height % 2 != 0
            || width > 8192
            || height > 8192
            || width as u64 * height as u64 > 33_554_432
            || fps == 0
            || fps > 60
        {
            return Err(
                "H.264 requires even dimensions, at least 2px, up to 8192px/32M pixels, 1–60 FPS"
                    .into(),
            );
        }
        if path.exists() {
            return Err("MP4 output already exists".into());
        }
        let runtime = Runtime::new()?;
        let p = wide(path);
        (|| -> windows::core::Result<Self> {
            unsafe {
                let writer = MFCreateSinkWriterFromURL(PCWSTR(p.as_ptr()), None, None)?;
                let output = video_type(&MFVideoFormat_H264, width, height, fps)?;
                output.SetUINT32(
                    &MF_MT_AVG_BITRATE,
                    ((width as u64 * height as u64 * fps as u64 / 5).clamp(600_000, 40_000_000))
                        as u32,
                )?;
                let stream = writer.AddStream(&output)?;
                let input = video_type(&MFVideoFormat_NV12, width, height, fps)?;
                writer.SetInputMediaType(stream, &input, None)?;
                writer.BeginWriting()?;
                Ok(Self {
                    writer: Some(writer),
                    stream,
                    width,
                    height,
                    _runtime: runtime,
                    finished: false,
                })
            }
        })()
        .map_err(|e| {
            format!("Starting H.264 encoder: {e}. On Windows N install Media Feature Pack.")
        })
    }
    pub fn write(&mut self, frame: &Frame, time: i64, duration: i64) -> Result<(), String> {
        frame.validate()?;
        if self.finished
            || frame.width != self.width
            || frame.height != self.height
            || time < 0
            || duration <= 0
            || time.checked_add(duration).is_none()
        {
            return Err("Invalid MP4 sample state, dimensions or timestamp".into());
        }
        let data = to_nv12(frame);
        let writer = self.writer.as_ref().ok_or("MP4 writer has closed")?;
        (|| -> windows::core::Result<()> {
            unsafe {
                let buffer = MFCreateMemoryBuffer(data.len() as u32)?;
                let mut ptr = std::ptr::null_mut();
                buffer.Lock(&mut ptr, None, None)?;
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
                buffer.Unlock()?;
                buffer.SetCurrentLength(data.len() as u32)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buffer)?;
                sample.SetSampleTime(time)?;
                sample.SetSampleDuration(duration)?;
                writer.WriteSample(self.stream, &sample)?;
                Ok(())
            }
        })()
        .map_err(|e| format!("Encoding H.264 frame: {e}"))
    }
    pub fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        unsafe { self.writer.as_ref().ok_or("MP4 writer closed")?.Finalize() }
            .map_err(|e| format!("Finalizing MP4: {e}"))?;
        self.finished = true;
        self.writer = None;
        Ok(())
    }
}
fn to_nv12(frame: &Frame) -> Vec<u8> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    let plane = w * h;
    let mut out = vec![0; plane * 3 / 2];
    let clip = |v: i32| v.clamp(0, 255) as u8;
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let p = (y + dy) * w + x + dx;
                    let i = p * 4;
                    let (b0, g0, r0) = (
                        frame.pixels[i] as i32,
                        frame.pixels[i + 1] as i32,
                        frame.pixels[i + 2] as i32,
                    );
                    out[p] = clip(((66 * r0 + 129 * g0 + 25 * b0 + 128) >> 8) + 16);
                    r += r0;
                    g += g0;
                    b += b0;
                }
            }
            r /= 4;
            g /= 4;
            b /= 4;
            let uv = plane + y / 2 * w + x;
            out[uv] = clip(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
            out[uv + 1] = clip(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);
        }
    }
    out
}
pub struct DecodedVideo {
    pub width: u32,
    pub height: u32,
    pub frame_count: u64,
    pub duration_ticks: i64,
    pub first: Frame,
}
pub struct VideoDecoder {
    reader: IMFSourceReader,
    pub width: u32,
    pub height: u32,
    _runtime: Runtime,
    pub last_time: i64,
    pub last_duration: i64,
    pub duration_ticks: i64,
    seek_target: Option<i64>,
}
impl VideoDecoder {
    pub fn new(path: &Path) -> Result<Self, String> {
        let runtime = Runtime::new()?;
        let p = wide(path);
        (move || -> windows::core::Result<Self> {
            unsafe {
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 1)?;
                let attrs = attrs.unwrap();
                attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
                let reader = MFCreateSourceReaderFromURL(PCWSTR(p.as_ptr()), &attrs)?;
                let t = MFCreateMediaType()?;
                t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                reader.SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &t,
                )?;
                let current =
                    reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)?;
                let size = current.GetUINT64(&MF_MT_FRAME_SIZE)?;
                let (width, height) = ((size >> 32) as u32, size as u32);
                if width == 0
                    || height == 0
                    || width > 8192
                    || height > 8192
                    || width as u64 * height as u64 > 33_554_432
                {
                    return Err(windows::core::Error::new(
                        windows::core::HRESULT(0x80070057u32 as i32),
                        "Video dimensions exceed supported decoder bounds",
                    ));
                }
                let duration = reader
                    .GetPresentationAttribute(
                        MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                        &MF_PD_DURATION,
                    )
                    .ok()
                    .and_then(|v| u64::try_from(&v).ok())
                    .and_then(|v| i64::try_from(v).ok())
                    .unwrap_or(0);
                Ok(Self {
                    reader,
                    width,
                    height,
                    _runtime: runtime,
                    last_time: 0,
                    last_duration: 0,
                    duration_ticks: duration,
                    seek_target: None,
                })
            }
        })()
        .map_err(|e| format!("Opening video decoder: {e}"))
    }
    pub fn seek(&mut self, time_100ns: i64) -> Result<(), String> {
        if time_100ns < 0 || self.duration_ticks > 0 && time_100ns > self.duration_ticks {
            return Err("Video seek position is outside duration".into());
        }
        let position =
            windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from(time_100ns);
        unsafe {
            self.reader
                .SetCurrentPosition(&windows::core::GUID::zeroed(), &position)
        }
        .map_err(|e| format!("Seeking video: {e}"))?;
        self.last_time = time_100ns;
        self.last_duration = 0;
        self.seek_target = Some(time_100ns);
        Ok(())
    }
    pub fn next_frame(&mut self) -> Result<Option<Frame>, String> {
        (move || -> windows::core::Result<Option<Frame>> {
            unsafe {
                loop {
                    let mut flags = 0;
                    let mut sample = None;
                    let mut time = 0;
                    self.reader.ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut time),
                        Some(&mut sample),
                    )?;
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        return Ok(None);
                    }
                    let Some(sample) = sample else { continue };
                    self.last_time = time;
                    self.last_duration = sample.GetSampleDuration().unwrap_or(0);
                    if self
                        .seek_target
                        .is_some_and(|target| time.saturating_add(self.last_duration) <= target)
                    {
                        continue;
                    }
                    self.seek_target = None;
                    let buffer = sample.ConvertToContiguousBuffer()?;
                    let mut ptr = std::ptr::null_mut();
                    let mut len = 0;
                    buffer.Lock(&mut ptr, None, Some(&mut len))?;
                    let expected = self.width as usize * self.height as usize * 4;
                    if (len as usize) < expected {
                        buffer.Unlock()?;
                        return Err(windows::core::Error::new(
                            windows::core::HRESULT(0x80004005u32 as i32),
                            "Decoded video buffer is smaller than frame",
                        ));
                    }
                    let pixels = std::slice::from_raw_parts(ptr, expected).to_vec();
                    buffer.Unlock()?;
                    let mut frame = Frame {
                        width: self.width,
                        height: self.height,
                        pixels,
                    };
                    for a in frame.pixels.iter_mut().skip(3).step_by(4) {
                        *a = 255
                    }
                    return Ok(Some(frame));
                }
            }
        })()
        .map_err(|e| format!("Decoding video sample: {e}"))
    }
}
pub fn decode_video(path: &Path) -> Result<DecodedVideo, String> {
    let mut reader = VideoDecoder::new(path)?;
    let mut first = None;
    let mut count = 0;
    let mut end = 0;
    while let Some(f) = reader.next_frame()? {
        if first.is_none() {
            first = Some(f)
        }
        count += 1;
        end = end.max(reader.last_time.saturating_add(reader.last_duration));
    }
    Ok(DecodedVideo {
        width: reader.width,
        height: reader.height,
        frame_count: count,
        duration_ticks: end,
        first: first.ok_or("Video contains no decoded frames")?,
    })
}
