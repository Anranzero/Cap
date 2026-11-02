use crate::geometry::Rect;
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.pixels.len())
            .finish()
    }
}
fn byte_length(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("Frame dimensions must be nonzero.".into());
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or_else(|| "Frame byte length overflow.".into())
}
impl Frame {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        let frame = Self {
            width,
            height,
            pixels,
        };
        frame.validate()?;
        Ok(frame)
    }
    pub fn blank(width: u32, height: u32, color: [u8; 4]) -> Result<Self, String> {
        let len = byte_length(width, height)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(|e| format!("Cannot allocate frame: {e}"))?;
        pixels.resize(len, 0);
        for p in pixels.chunks_exact_mut(4) {
            p.copy_from_slice(&color);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.pixels.len() != byte_length(self.width, self.height)? {
            return Err("BGRA frame buffer length does not match its dimensions.".into());
        }
        Ok(())
    }
    pub fn stride(&self) -> usize {
        self.width as usize * 4
    }
    pub fn copy(&self) -> Result<Self, String> {
        self.validate()?;
        let mut f = Self::blank(self.width, self.height, [0; 4])?;
        f.pixels.copy_from_slice(&self.pixels);
        Ok(f)
    }
    pub fn crop(&self, rect: Rect) -> Result<Self, String> {
        self.validate()?;
        rect.validate()?;
        if rect.is_empty()
            || rect.x < 0
            || rect.y < 0
            || rect.right() as u32 > self.width
            || rect.bottom() as u32 > self.height
        {
            return Err("Crop must be a nonempty local image rectangle.".into());
        }
        let mut out = Self::blank(rect.width, rect.height, [0; 4])?;
        let stride = out.stride();
        for y in 0..rect.height as usize {
            let offset = (rect.y as usize + y) * self.stride() + rect.x as usize * 4;
            out.pixels[y * stride..(y + 1) * stride]
                .copy_from_slice(&self.pixels[offset..offset + stride]);
        }
        Ok(out)
    }
    pub fn resize_nearest(&self, width: u32, height: u32) -> Result<Self, String> {
        self.validate()?;
        let mut out = Self::blank(width, height, [0; 4])?;
        for y in 0..height {
            let sy = y as u64 * self.height as u64 / height as u64;
            for x in 0..width {
                let sx = x as u64 * self.width as u64 / width as u64;
                let s = (sy * self.width as u64 + sx) as usize * 4;
                let d = (y as usize * width as usize + x as usize) * 4;
                out.pixels[d..d + 4].copy_from_slice(&self.pixels[s..s + 4]);
            }
        }
        Ok(out)
    }
    pub fn resize_bilinear(&self, width: u32, height: u32) -> Result<Self, String> {
        self.validate()?;
        let mut out = Self::blank(width, height, [0; 4])?;
        for y in 0..height {
            let sy = ((y as f64 + 0.5) * self.height as f64 / height as f64 - 0.5)
                .clamp(0.0, (self.height - 1) as f64);
            let y0 = sy.floor() as usize;
            let y1 = (y0 + 1).min(self.height as usize - 1);
            let fy = sy - y0 as f64;
            for x in 0..width {
                let sx = ((x as f64 + 0.5) * self.width as f64 / width as f64 - 0.5)
                    .clamp(0.0, (self.width - 1) as f64);
                let x0 = sx.floor() as usize;
                let x1 = (x0 + 1).min(self.width as usize - 1);
                let fx = sx - x0 as f64;
                for c in 0..4 {
                    let sample =
                        |xx, yy| self.pixels[(yy * self.width as usize + xx) * 4 + c] as f64;
                    let top = sample(x0, y0) * (1.0 - fx) + sample(x1, y0) * fx;
                    let bottom = sample(x0, y1) * (1.0 - fx) + sample(x1, y1) * fx;
                    out.pixels[(y as usize * width as usize + x as usize) * 4 + c] =
                        (top * (1.0 - fy) + bottom * fy).round() as u8;
                }
            }
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_dimensions_and_public_buffer_mutation_rejected() {
        assert!(Frame::new(0, 1, vec![]).is_err());
        assert!(Frame::new(2, 2, vec![0; 4]).is_err());
        assert!(Frame::new(u32::MAX, u32::MAX, vec![]).is_err());
        let mut f = Frame::blank(2, 2, [0, 0, 0, 255]).unwrap();
        f.pixels.pop();
        assert!(f.validate().is_err());
    }
    #[test]
    fn crop_and_resize_preserve_top_down_bgra() {
        let f = Frame::new(
            2,
            2,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255],
        )
        .unwrap();
        assert_eq!(
            f.crop(Rect::new(1, 0, 1, 2).unwrap()).unwrap().pixels,
            vec![4, 5, 6, 255, 10, 11, 12, 255]
        );
        assert!(f.crop(Rect::new(-1, 0, 1, 1).unwrap()).is_err());
        assert_eq!(
            f.resize_nearest(1, 2).unwrap().pixels,
            vec![1, 2, 3, 255, 7, 8, 9, 255]
        );
        assert!(f.resize_nearest(0, 2).is_err());
    }
    #[test]
    fn bilinear_uses_pixel_centers_and_clamps_edges() {
        let f = Frame::new(
            3,
            1,
            vec![0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255],
        )
        .unwrap();
        assert_eq!(
            f.resize_bilinear(2, 1).unwrap().pixels,
            vec![25, 25, 25, 255, 175, 175, 175, 255]
        );
        let single = Frame::blank(1, 1, [10, 20, 30, 128]).unwrap();
        assert_eq!(
            single.resize_bilinear(2, 2).unwrap().pixels,
            [10, 20, 30, 128].repeat(4)
        );
        assert!(f.resize_bilinear(0, 2).is_err());
    }
}
