use crate::{capture::Dib, pixels::Frame};
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

pub fn rasterize(text: &str, color: [u8; 4], thickness: f64) -> Result<Frame, String> {
    if text.trim().is_empty() || text.chars().count() > 32768 {
        return Err("文字为空或过长。".into());
    }
    let size = (16.0 + thickness * 3.0).round().clamp(12.0, 72.0) as i32;
    let mut font_info = LOGFONTW {
        lfHeight: -size,
        lfWeight: 400,
        lfQuality: ANTIALIASED_QUALITY,
        ..Default::default()
    };
    for (dst, value) in font_info
        .lfFaceName
        .iter_mut()
        .zip("Microsoft YaHei UI".encode_utf16())
    {
        *dst = value;
    }
    let font = unsafe { CreateFontIndirectW(&font_info) };
    if font.is_invalid() {
        return Err("创建文字字体失败。".into());
    }
    let result = (|| {
        let measuring = Dib::new(1, 1)?;
        let dc = HDC(measuring.dc as *mut _);
        let previous = unsafe { SelectObject(dc, font.into()) };
        let mut width = 1;
        let lines: Vec<&str> = text.lines().collect();
        for line in &lines {
            let chars: Vec<u16> = line.encode_utf16().collect();
            if chars.is_empty() {
                continue;
            }
            let mut extent = SIZE::default();
            if !unsafe { GetTextExtentPoint32W(dc, &chars, &mut extent) }.as_bool() {
                unsafe {
                    SelectObject(dc, previous);
                }
                return Err("测量文字失败。".into());
            }
            width = width.max(extent.cx + 4);
        }
        unsafe {
            SelectObject(dc, previous);
        }
        let height = (lines.len() as i32 * (size + 6) + 4).max(size + 6);
        if width > 8192 || height > 8192 || width as u64 * height as u64 > 16_000_000 {
            return Err("文字图像过大。".into());
        }
        let dib = Dib::new(width as u32, height as u32)?;
        dib.copy_from(&Frame::blank(width as u32, height as u32, [0, 0, 0, 255])?)?;
        let dc = HDC(dib.dc as *mut _);
        let previous = unsafe { SelectObject(dc, font.into()) };
        unsafe {
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00ffffff));
        }
        let drawn: Result<(), String> = (|| {
            for (index, line) in lines.iter().enumerate() {
                let chars: Vec<u16> = line.encode_utf16().collect();
                if !chars.is_empty()
                    && !unsafe { TextOutW(dc, 2, 2 + index as i32 * (size + 6), &chars) }.as_bool()
                {
                    return Err("绘制文字失败。".into());
                }
            }
            Ok(())
        })();
        unsafe {
            SelectObject(dc, previous);
        }
        drawn?;
        let mut frame = dib.frame()?;
        for pixel in frame.pixels.chunks_exact_mut(4) {
            let alpha = pixel[0].max(pixel[1]).max(pixel[2]);
            pixel[..3].copy_from_slice(&color[..3]);
            pixel[3] = alpha;
        }
        Ok(frame)
    })();
    unsafe {
        let _ = DeleteObject(font.into());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_unicode_text_has_transparent_background_and_multiline_height() {
        assert!(rasterize("", [0, 0, 255, 255], 3.).is_err());
        let single = rasterize("CAP 中文", [0, 0, 255, 255], 3.).unwrap();
        let multiline = rasterize("CAP 中文\n123", [0, 0, 255, 255], 3.).unwrap();
        assert!(multiline.height > single.height);
        assert!(
            single
                .pixels
                .chunks_exact(4)
                .any(|p| p[3] > 0 && p[2] == 255 && p[0] == 0)
        );
        assert!(single.pixels.chunks_exact(4).any(|p| p[3] == 0));
    }
}
