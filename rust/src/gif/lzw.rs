use super::io_error;
use std::{collections::HashMap, io::Write};
pub(super) fn write<W: Write>(
    writer: &mut W,
    pixels: &[u8],
    width: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) -> Result<(), String> {
    writer.write_all(&[8]).map_err(io_error)?;
    let mut out = Bits {
        writer,
        block: [0; 255],
        used: 0,
        value: 0,
        count: 0,
    };
    let mut codes: HashMap<u32, u16> = HashMap::with_capacity(4096);
    let (mut next, mut bits) = (258u16, 9u8);
    out.write(256, bits)?;
    let mut prefix = None;
    for y in top..=bottom {
        for x in left..=right {
            let value = pixels[y as usize * width as usize + x as usize] as u16;
            let Some(old) = prefix else {
                prefix = Some(value);
                continue;
            };
            let key = ((old as u32) << 8) | value as u32;
            if let Some(&code) = codes.get(&key) {
                prefix = Some(code);
                continue;
            }
            out.write(old, bits)?;
            // Decoder adds entries one code later than the encoder.
            if next >= (1 << bits) && bits < 12 {
                bits += 1;
            }
            if next < 4096 {
                codes.insert(key, next);
                next += 1;
            } else {
                out.write(256, bits)?;
                codes.clear();
                next = 258;
                bits = 9;
            }
            prefix = Some(value);
        }
    }
    if let Some(prefix) = prefix {
        out.write(prefix, bits)?;
    }
    if next >= (1 << bits) && bits < 12 {
        bits += 1;
    }
    out.write(257, bits)?;
    out.finish()
}
struct Bits<'a, W: Write> {
    writer: &'a mut W,
    block: [u8; 255],
    used: usize,
    value: u32,
    count: u8,
}
impl<W: Write> Bits<'_, W> {
    fn write(&mut self, code: u16, bits: u8) -> Result<(), String> {
        self.value |= (code as u32) << self.count;
        self.count += bits;
        while self.count >= 8 {
            self.append(self.value as u8)?;
            self.value >>= 8;
            self.count -= 8;
        }
        Ok(())
    }
    fn append(&mut self, byte: u8) -> Result<(), String> {
        self.block[self.used] = byte;
        self.used += 1;
        if self.used == 255 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        if self.used > 0 {
            self.writer
                .write_all(&[self.used as u8])
                .map_err(io_error)?;
            self.writer
                .write_all(&self.block[..self.used])
                .map_err(io_error)?;
            self.used = 0;
        }
        Ok(())
    }
    fn finish(mut self) -> Result<(), String> {
        if self.count > 0 {
            self.append(self.value as u8)?;
        }
        self.flush()?;
        self.writer.write_all(&[0]).map_err(io_error)
    }
}
