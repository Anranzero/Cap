use crate::pixels::Frame;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAppendResult {
    Appended,
    Unchanged,
    NoReliableOverlap,
    LimitReached,
}
pub struct ScrollStitcher {
    previous: Frame,
    segments: Vec<Vec<u8>>,
    height: u32,
    viewport_height: u32,
    max_pixels: u64,
    max_height: u32,
}
pub type ScrollingCaptureStitcher = ScrollStitcher;
impl ScrollStitcher {
    pub fn new(first: Frame) -> Result<Self, String> {
        Self::with_limits(first, 40_000_000, 30_000)
    }
    pub fn with_limits(first: Frame, pixels: u64, height: u32) -> Result<Self, String> {
        first.validate()?;
        if pixels == 0
            || height == 0
            || first.width as u64 * first.height as u64 > pixels
            || first.height > height
        {
            return Err("Selected viewport exceeds scrolling screenshot limits.".into());
        }
        let previous = first.copy()?;
        Ok(Self {
            height: first.height,
            viewport_height: first.height,
            segments: vec![first.pixels],
            previous,
            max_pixels: pixels,
            max_height: height,
        })
    }
    pub fn try_append(&mut self, next: &Frame) -> Result<ScrollAppendResult, String> {
        next.validate()?;
        if next.width != self.width() || next.height != self.viewport_height {
            return Err("Scrolling viewports must have equal dimensions.".into());
        }
        if Self::are_similar(&self.previous, next) {
            return Ok(ScrollAppendResult::Unchanged);
        }
        let min_shift = 8.max(self.viewport_height / 100);
        let max_shift = self
            .viewport_height
            .saturating_sub(32.max(self.viewport_height / 5));
        let (mut best_shift, mut best_score) = (0, f64::MAX);
        for shift in min_shift..=max_shift {
            let score = difference(&self.previous, next, shift, 9, 32);
            if score < best_score {
                best_shift = shift;
                best_score = score;
            }
        }
        if best_shift == 0 || best_score > 3.0 {
            return Ok(ScrollAppendResult::NoReliableOverlap);
        }
        let threshold = 2.0f64.max(best_score + 1.5);
        for shift in min_shift..=max_shift {
            if shift.abs_diff(best_shift) > 3
                && difference(&self.previous, next, shift, 9, 32) < threshold
                || difference(next, &self.previous, shift, 9, 32) < threshold
            {
                return Ok(ScrollAppendResult::NoReliableOverlap);
            }
        }
        if difference(&self.previous, next, best_shift, 45, 48) > 3.0
            || has_stationary_footer(&self.previous, next)
        {
            return Ok(ScrollAppendResult::NoReliableOverlap);
        }
        let Some(height) = self.height.checked_add(best_shift) else {
            return Ok(ScrollAppendResult::LimitReached);
        };
        if height > self.max_height || self.width() as u64 * height as u64 > self.max_pixels {
            return Ok(ScrollAppendResult::LimitReached);
        }
        // Complete allocations before committing any state.
        let previous = next.copy()?;
        let addition = next
            .crop(crate::geometry::Rect::new(
                0,
                (next.height - best_shift) as i32,
                next.width,
                best_shift,
            )?)?
            .pixels;
        self.segments.push(addition);
        self.previous = previous;
        self.height = height;
        Ok(ScrollAppendResult::Appended)
    }
    pub fn undo(&mut self) -> bool {
        if !self.can_undo() {
            return false;
        }
        let last = self.segments.pop().expect("at least two segments");
        self.height -= (last.len() / self.previous.stride()) as u32;
        let mut remaining = self.previous.pixels.len();
        for segment in self.segments.iter().rev() {
            let n = segment.len().min(remaining);
            remaining -= n;
            self.previous.pixels[remaining..remaining + n]
                .copy_from_slice(&segment[segment.len() - n..]);
            if remaining == 0 {
                break;
            }
        }
        true
    }
    pub fn can_undo(&self) -> bool {
        self.segments.len() > 1
    }
    pub fn width(&self) -> u32 {
        self.previous.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn create_frame(&self) -> Result<Frame, String> {
        let mut frame = Frame::blank(self.width(), self.height, [0; 4])?;
        let mut offset = 0;
        for segment in &self.segments {
            frame.pixels[offset..offset + segment.len()].copy_from_slice(segment);
            offset += segment.len();
        }
        Ok(frame)
    }
    pub fn matches_current_viewport(&self, frame: &Frame) -> bool {
        frame.validate().is_ok()
            && frame.width == self.width()
            && frame.height == self.viewport_height
            && difference(&self.previous, frame, 0, 45, 48) <= 3.0
    }
    pub fn are_similar(a: &Frame, b: &Frame) -> bool {
        if a.validate().is_err()
            || b.validate().is_err()
            || a.width != b.width
            || a.height != b.height
        {
            return false;
        }
        let mut total = 0.0;
        let mut count = 0;
        for y in (0..a.height).step_by((a.height / 48).max(1) as usize) {
            for x in (0..a.width).step_by((a.width / 48).max(1) as usize) {
                let p = (y as usize * a.width as usize + x as usize) * 4;
                total += (luminance(&a.pixels, p) - luminance(&b.pixels, p)).abs();
                count += 1;
            }
        }
        total / (count as f64) < 0.4
    }
}
fn luminance(pixels: &[u8], p: usize) -> f64 {
    (pixels[p] as f64 * 29.0 + pixels[p + 1] as f64 * 150.0 + pixels[p + 2] as f64 * 77.0) / 256.0
}
fn difference(older: &Frame, newer: &Frame, shift: u32, rows: u32, columns: u32) -> f64 {
    let top = 2.max(older.height / 10);
    let bottom = older.height.saturating_sub(shift);
    if bottom.saturating_sub(top) < 16 {
        return f64::MAX;
    }
    let mut sum = 0.0;
    for r in 0..rows {
        let y = top + (bottom - top - 1) * r / (rows - 1);
        for c in 0..columns {
            let x = (older.width / 12
                + (older.width.saturating_mul(5) / 6).saturating_sub(1) * c / (columns - 1))
                .min(older.width - 1);
            let a = ((y + shift) as usize * older.width as usize + x as usize) * 4;
            let b = (y as usize * newer.width as usize + x as usize) * 4;
            sum += (luminance(&older.pixels, a) - luminance(&newer.pixels, b)).abs();
        }
    }
    sum / (rows * columns) as f64
}
fn has_stationary_footer(a: &Frame, b: &Frame) -> bool {
    // Check rows separately: averaging 12 rows hides a smaller fixed footer
    // behind a few moving rows. Include padding above the last visible row.
    let rows = (a.height / 5).clamp(3, 48).min(a.height);
    let mut stationary_rows = 0;
    for y in a.height - rows..a.height {
        let (mut difference, mut count) = (0.0, 0u32);
        for x in
            (a.width / 12..a.width.saturating_mul(11) / 12).step_by((a.width / 48).max(1) as usize)
        {
            let p = (y as usize * a.width as usize + x as usize) * 4;
            difference += (luminance(&a.pixels, p) - luminance(&b.pixels, p)).abs();
            count += 1;
        }
        if count > 0 && difference / (count as f64) < 1.0 {
            stationary_rows += 1;
            if stationary_rows >= 3 {
                return true;
            }
        } else {
            stationary_rows = 0;
        }
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    fn page(start: u32, height: u32) -> Frame {
        let mut f = Frame::blank(160, height, [0, 0, 0, 255]).unwrap();
        for y in 0..height {
            for x in 0u32..160 {
                let mut h = (y + start).wrapping_mul(73856093) ^ x.wrapping_mul(19349663);
                h ^= h >> 13;
                h = h.wrapping_mul(1274126177);
                h ^= h >> 16;
                let p = ((y * 160 + x) * 4) as usize;
                f.pixels[p..p + 3].copy_from_slice(&h.to_le_bytes()[..3]);
            }
        }
        f
    }
    #[test]
    fn forward_overlap_undo_and_reappend_keep_exact_rows() {
        let mut s = ScrollStitcher::new(page(0, 240)).unwrap();
        assert_eq!(
            s.try_append(&page(100, 240)).unwrap(),
            ScrollAppendResult::Appended
        );
        assert_eq!(s.create_frame().unwrap(), page(0, 340));
        assert_eq!(
            s.try_append(&page(100, 240)).unwrap(),
            ScrollAppendResult::Unchanged
        );
        assert_eq!(
            s.try_append(&page(0, 240)).unwrap(),
            ScrollAppendResult::NoReliableOverlap
        );
        assert_eq!(
            s.try_append(&page(320, 240)).unwrap(),
            ScrollAppendResult::NoReliableOverlap
        );
        assert_eq!(
            s.try_append(&page(200, 240)).unwrap(),
            ScrollAppendResult::Appended
        );
        assert!(s.undo());
        assert_eq!(s.height(), 340);
        assert_eq!(
            s.try_append(&page(190, 240)).unwrap(),
            ScrollAppendResult::Appended
        );
        assert_eq!(s.create_frame().unwrap(), page(0, 430));
    }
    #[test]
    fn limits_and_fixed_footer_reject_without_mutation() {
        let first = page(0, 240);
        let mut s = ScrollStitcher::with_limits(first.clone(), 160 * 300, 1000).unwrap();
        assert_eq!(
            s.try_append(&page(100, 240)).unwrap(),
            ScrollAppendResult::LimitReached
        );
        assert_eq!(s.height(), 240);
        for rows in [10, 12, 24] {
            let mut a = first.clone();
            let mut b = page(100, 240);
            let p = ((240 - rows) * 160 * 4) as usize;
            b.pixels[p..].copy_from_slice(&a.pixels[p..]);
            if rows == 24 {
                let pad = (226 * 160 * 4) as usize;
                a.pixels[pad..].fill(255);
                b.pixels[pad..].fill(255);
            }
            assert_eq!(
                ScrollStitcher::new(a).unwrap().try_append(&b).unwrap(),
                ScrollAppendResult::NoReliableOverlap,
                "stationary footer with {rows} rows"
            );
        }
    }
    #[test]
    fn sticky_header_allowed_and_blank_repeat_unchanged() {
        let first = page(0, 240);
        let mut next = page(100, 240);
        next.pixels[..160 * 20 * 4].copy_from_slice(&first.pixels[..160 * 20 * 4]);
        let mut s = ScrollStitcher::new(first).unwrap();
        assert_eq!(s.try_append(&next).unwrap(), ScrollAppendResult::Appended);
        assert_eq!(s.create_frame().unwrap(), page(0, 340));
        let blank = Frame::blank(160, 240, [255; 4]).unwrap();
        assert_eq!(
            ScrollStitcher::new(blank.clone())
                .unwrap()
                .try_append(&blank)
                .unwrap(),
            ScrollAppendResult::Unchanged
        );
    }
    #[test]
    fn repeated_bands_and_untextured_bottom_bar_rejected() {
        let tile = page(0, 32);
        let mut a = page(0, 240);
        let mut b = a.clone();
        for y in 0..240 {
            a.pixels[y * 640..(y + 1) * 640]
                .copy_from_slice(&tile.pixels[y % 32 * 640..(y % 32 + 1) * 640]);
            b.pixels[y * 640..(y + 1) * 640]
                .copy_from_slice(&tile.pixels[(y + 4) % 32 * 640..((y + 4) % 32 + 1) * 640]);
        }
        assert_eq!(
            ScrollStitcher::new(a).unwrap().try_append(&b).unwrap(),
            ScrollAppendResult::NoReliableOverlap
        );
        let mut a = page(0, 240);
        let mut b = page(100, 240);
        a.pixels[230 * 640..].fill(255);
        b.pixels[230 * 640..].fill(255);
        assert_eq!(
            ScrollStitcher::new(a).unwrap().try_append(&b).unwrap(),
            ScrollAppendResult::NoReliableOverlap
        );
    }
}
