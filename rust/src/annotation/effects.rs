use crate::pixels::Frame;

pub fn pixelate(frame: &mut Frame, block: u32) -> Result<(), String> {
    frame.validate()?;
    if block == 0 {
        return Err("Mosaic block must be nonzero.".into());
    }
    let (w, h) = (frame.width as usize, frame.height as usize);
    let block = block as usize;
    for y in (0..h).step_by(block) {
        for x in (0..w).step_by(block) {
            let right = x.saturating_add(block).min(w);
            let bottom = y.saturating_add(block).min(h);
            let mut sum = [0u64; 4];
            let count = ((right - x) * (bottom - y)) as u64;
            for yy in y..bottom {
                for xx in x..right {
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += frame.pixels[(yy * w + xx) * 4 + c] as u64;
                    }
                }
            }
            let color = sum.map(|s| (s / count) as u8);
            for yy in y..bottom {
                for xx in x..right {
                    frame.pixels[(yy * w + xx) * 4..(yy * w + xx) * 4 + 4].copy_from_slice(&color);
                }
            }
        }
    }
    Ok(())
}

/// Two sliding window passes keep work linear, including large radii.
pub fn blur(frame: &mut Frame, radius: u32) -> Result<(), String> {
    frame.validate()?;
    let (w, h) = (frame.width as usize, frame.height as usize);
    let radius = (radius as usize).min(w.max(h));
    let mut horizontal = Frame::blank(frame.width, frame.height, [0; 4])?;
    for y in 0..h {
        for c in 0..4 {
            let mut sum = 0u64;
            for x in 0..=radius.min(w - 1) {
                sum += frame.pixels[(y * w + x) * 4 + c] as u64;
            }
            for x in 0..w {
                let left = x.saturating_sub(radius);
                let right = x.saturating_add(radius).min(w - 1);
                horizontal.pixels[(y * w + x) * 4 + c] = (sum / (right - left + 1) as u64) as u8;
                if x >= radius {
                    sum -= frame.pixels[(y * w + x - radius) * 4 + c] as u64;
                }
                if x + radius + 1 < w {
                    sum += frame.pixels[(y * w + x + radius + 1) * 4 + c] as u64;
                }
            }
        }
    }
    for x in 0..w {
        for c in 0..4 {
            let mut sum = 0u64;
            for y in 0..=radius.min(h - 1) {
                sum += horizontal.pixels[(y * w + x) * 4 + c] as u64;
            }
            for y in 0..h {
                let top = y.saturating_sub(radius);
                let bottom = y.saturating_add(radius).min(h - 1);
                frame.pixels[(y * w + x) * 4 + c] = (sum / (bottom - top + 1) as u64) as u8;
                if y >= radius {
                    sum -= horizontal.pixels[((y - radius) * w + x) * 4 + c] as u64;
                }
                if y + radius + 1 < h {
                    sum += horizontal.pixels[((y + radius + 1) * w + x) * 4 + c] as u64;
                }
            }
        }
    }
    Ok(())
}
