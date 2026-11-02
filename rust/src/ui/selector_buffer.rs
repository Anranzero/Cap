//! A persistent off-screen surface for publishing complete selection frames.
use windows::Win32::Graphics::Gdi::*;

pub(in crate::ui) struct PaintBuffer {
    pub(in crate::ui) dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    width: i32,
    height: i32,
}

impl PaintBuffer {
    pub(in crate::ui) fn new(reference: HDC, width: u32, height: u32) -> Result<Self, String> {
        let width = i32::try_from(width).map_err(|_| "选区绘图缓存宽度溢出。")?;
        let height = i32::try_from(height).map_err(|_| "选区绘图缓存高度溢出。")?;
        if width <= 0 || height <= 0 {
            return Err("选区绘图缓存尺寸无效。".into());
        }
        let dc = unsafe { CreateCompatibleDC(Some(reference)) };
        if dc.is_invalid() {
            return Err(format!(
                "创建选区绘图缓存失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let mut buffer = Self {
            dc,
            bitmap: HBITMAP::default(),
            previous: HGDIOBJ::default(),
            width,
            height,
        };
        buffer.bitmap = unsafe { CreateCompatibleBitmap(reference, width, height) };
        if buffer.bitmap.is_invalid() {
            return Err(format!(
                "创建选区缓存位图失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        buffer.previous = unsafe { SelectObject(buffer.dc, buffer.bitmap.into()) };
        if buffer.previous.0.is_null() || buffer.previous.0 as isize == -1 {
            return Err(format!(
                "选择选区缓存位图失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(buffer)
    }

    pub(in crate::ui) fn matches(&self, width: u32, height: u32) -> bool {
        self.width as u32 == width && self.height as u32 == height
    }

    pub(in crate::ui) fn present(&self, destination: HDC) -> Result<(), String> {
        unsafe {
            BitBlt(
                destination,
                0,
                0,
                self.width,
                self.height,
                Some(self.dc),
                0,
                0,
                SRCCOPY,
            )
        }
        .map_err(|error| format!("显示选区绘图缓存失败：{error}"))
    }
}

impl Drop for PaintBuffer {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.0.is_null() && self.previous.0 as isize != -1 {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            let _ = DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::common;

    #[test]
    fn partial_drawing_stays_offscreen_until_complete_frame_is_presented() {
        let reference = unsafe { GetDC(None) };
        assert!(!reference.is_invalid());
        let destination = PaintBuffer::new(reference, 64, 32).unwrap();
        let staging = PaintBuffer::new(reference, 64, 32).unwrap();
        assert_ne!(unsafe { ReleaseDC(None, reference) }, 0);
        let background = common::rgb(20, 30, 40);
        common::fill(destination.dc, common::rect(0, 0, 64, 32), background);
        common::fill(
            staging.dc,
            common::rect(0, 0, 32, 32),
            common::rgb(240, 10, 20),
        );
        assert_eq!(unsafe { GetPixel(destination.dc, 10, 10) }.0, background);
        common::fill(
            staging.dc,
            common::rect(32, 0, 32, 32),
            common::rgb(10, 20, 240),
        );
        assert_eq!(unsafe { GetPixel(destination.dc, 50, 10) }.0, background);
        staging.present(destination.dc).unwrap();
        assert_eq!(
            unsafe { GetPixel(destination.dc, 10, 10) }.0,
            common::rgb(240, 10, 20)
        );
        assert_eq!(
            unsafe { GetPixel(destination.dc, 50, 10) }.0,
            common::rgb(10, 20, 240)
        );
        common::fill(
            staging.dc,
            common::rect(0, 0, 64, 32),
            common::rgb(10, 230, 20),
        );
        assert_eq!(
            unsafe { GetPixel(destination.dc, 50, 10) }.0,
            common::rgb(10, 20, 240)
        );
        staging.present(destination.dc).unwrap();
        assert_eq!(
            unsafe { GetPixel(destination.dc, 50, 10) }.0,
            common::rgb(10, 230, 20)
        );
    }
}
