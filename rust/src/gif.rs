//! Streaming GIF89a with stable RGB332 dithering and bounded image history.
use crate::pixels::Frame;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};
pub struct GifEncoder<W: Write> {
    writer: W,
    width: u32,
    height: u32,
    pending: Option<Vec<u8>>,
    previous: Option<Vec<u8>>,
    pending_end: i64,
    last_end: Option<i64>,
    written_centiseconds: i64,
    completed: bool,
    failed: bool,
}
impl<W: Write> GifEncoder<W> {
    pub fn new(mut writer: W, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        if !(1..=65535).contains(&width)
            || !(1..=65535).contains(&height)
            || !(1..=100).contains(&fps)
        {
            return Err("GIF dimensions or frame rate are invalid.".into());
        }
        let mut header = Vec::with_capacity(800);
        header.extend_from_slice(b"GIF89a");
        header.extend_from_slice(&(width as u16).to_le_bytes());
        header.extend_from_slice(&(height as u16).to_le_bytes());
        header.extend_from_slice(&[0xf7, 0, 0]);
        for i in 0u16..256 {
            header.extend_from_slice(&[
                (((i >> 5) & 7) * 255 / 7) as u8,
                (((i >> 2) & 7) * 255 / 7) as u8,
                ((i & 3) * 255 / 3) as u8,
            ]);
        }
        header.extend_from_slice(&[0x21, 0xff, 11]);
        header.extend_from_slice(b"NETSCAPE2.0");
        header.extend_from_slice(&[3, 1, 0, 0, 0]);
        writer.write_all(&header).map_err(io_error)?;
        Ok(Self {
            writer,
            width,
            height,
            pending: None,
            previous: None,
            pending_end: 0,
            last_end: None,
            written_centiseconds: 0,
            completed: false,
            failed: false,
        })
    }
    pub fn write(&mut self, frame: &Frame, time: i64, duration: i64) -> Result<(), String> {
        if self.completed || self.failed {
            return Err("GIF encoder has completed or failed.".into());
        }
        frame.validate()?;
        let end = time
            .checked_add(duration)
            .ok_or("GIF timestamp overflow.")?;
        if frame.width != self.width
            || frame.height != self.height
            || time < 0
            || duration <= 0
            || self.last_end.is_some_and(|last| time < last)
        {
            return Err("GIF frame dimensions or non-overlapping timeline are invalid.".into());
        }
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(frame.pixels.len() / 4)
            .map_err(|e| format!("Cannot allocate GIF indices: {e}"))?;
        let (eight, four) = quantizers();
        for (p, pixel) in frame.pixels.chunks_exact(4).enumerate() {
            let (x, y) = (p % self.width as usize, p / self.width as usize);
            let t = BAYER[(y & 7) * 8 + (x & 7)] as usize * 256;
            indices.push(
                (eight[t + pixel[2] as usize] << 5)
                    | (eight[t + pixel[1] as usize] << 2)
                    | four[t + pixel[0] as usize],
            );
        }
        if self.pending.as_ref() == Some(&indices) {
            self.pending_end = end;
            self.last_end = Some(end);
            return Ok(());
        }
        if let Err(error) = self.flush_pending() {
            self.failed = true;
            return Err(error);
        }
        self.pending = Some(indices);
        self.pending_end = end;
        self.last_end = Some(end);
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        if self.completed {
            return Ok(());
        }
        if self.failed {
            return Err("GIF encoder previously failed.".into());
        }
        if self.pending.is_none() && self.previous.is_none() {
            return Err("GIF contains no frames.".into());
        }
        let result = self
            .flush_pending()
            .and_then(|_| self.writer.write_all(&[0x3b]).map_err(io_error))
            .and_then(|_| self.writer.flush().map_err(io_error));
        match result {
            Ok(()) => {
                self.completed = true;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    pub fn into_inner(mut self) -> Result<W, String> {
        self.finish()?;
        Ok(self.writer)
    }
    fn flush_pending(&mut self) -> Result<(), String> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        // Round cumulative time instead of each duration to avoid 24/30/60 FPS drift.
        let target = (self.pending_end / 100000 + i64::from(self.pending_end % 100000 >= 50000))
            .max(self.written_centiseconds + 1);
        let mut delay = target - self.written_centiseconds;
        let (mut left, mut top, mut right, mut bottom) = (0, 0, self.width - 1, self.height - 1);
        if let Some(previous) = &self.previous {
            left = self.width;
            top = self.height;
            right = 0;
            bottom = 0;
            let mut changed = false;
            for (p, (&a, &b)) in previous.iter().zip(&pending).enumerate() {
                if a != b {
                    let (x, y) = (
                        (p % self.width as usize) as u32,
                        (p / self.width as usize) as u32,
                    );
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x);
                    bottom = bottom.max(y);
                    changed = true;
                }
            }
            if !changed {
                left = 0;
                top = 0;
                right = 0;
                bottom = 0;
            }
        }
        while delay > 0 {
            let part = delay.min(u16::MAX as i64) as u16;
            self.writer
                .write_all(&[0x21, 0xf9, 4, 4])
                .map_err(io_error)?;
            self.writer
                .write_all(&part.to_le_bytes())
                .map_err(io_error)?;
            self.writer.write_all(&[0, 0, 0x2c]).map_err(io_error)?;
            for value in [left, top, right - left + 1, bottom - top + 1] {
                self.writer
                    .write_all(&(value as u16).to_le_bytes())
                    .map_err(io_error)?;
            }
            self.writer.write_all(&[0]).map_err(io_error)?;
            lzw::write(
                &mut self.writer,
                &pending,
                self.width,
                left,
                top,
                right,
                bottom,
            )?;
            self.written_centiseconds += part as i64;
            delay -= part as i64;
            // An extended hold needs only one unchanged pixel after the first image.
            left = 0;
            top = 0;
            right = 0;
            bottom = 0;
        }
        self.previous = Some(pending);
        Ok(())
    }
}
fn io_error(error: std::io::Error) -> String {
    format!("GIF output failed: {error}")
}
#[path = "gif/lzw.rs"]
mod lzw;
const BAYER: [u8; 64] = [
    0, 48, 12, 60, 3, 51, 15, 63, 32, 16, 44, 28, 35, 19, 47, 31, 8, 56, 4, 52, 11, 59, 7, 55, 40,
    24, 36, 20, 43, 27, 39, 23, 2, 50, 14, 62, 1, 49, 13, 61, 34, 18, 46, 30, 33, 17, 45, 29, 10,
    58, 6, 54, 9, 57, 5, 53, 42, 26, 38, 22, 41, 25, 37, 21,
];
type Quantizer = [u8; 64 * 256];
fn quantizers() -> &'static (Quantizer, Quantizer) {
    static TABLES: std::sync::OnceLock<(Quantizer, Quantizer)> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        fn build(maximum: u32) -> Quantizer {
            let mut table = [0; 64 * 256];
            for threshold in 0..64u32 {
                for value in 0..256u32 {
                    let lower = value * maximum / 255;
                    let upper = if lower == maximum {
                        false
                    } else {
                        let low = lower * 255 / maximum;
                        let high = (lower + 1) * 255 / maximum;
                        (value - low) * 128 > (threshold * 2 + 1) * (high - low)
                    };
                    table[(threshold * 256 + value) as usize] = (lower + u32::from(upper)) as u8;
                }
            }
            table
        }
        (build(7), build(3))
    })
}
pub struct GifWriter(GifEncoder<BufWriter<File>>);
impl GifWriter {
    pub fn new(path: &Path, width: u32, height: u32) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        Ok(Self(GifEncoder::new(
            BufWriter::new(file),
            width,
            height,
            30,
        )?))
    }
    pub fn write(&mut self, frame: &Frame, time: i64, duration: i64) -> Result<(), String> {
        self.0.write(frame, time, duration)
    }
    pub fn finish(&mut self) -> Result<(), String> {
        self.0.finish()
    }
}
#[cfg(test)]
#[path = "gif/tests.rs"]
mod tests;
