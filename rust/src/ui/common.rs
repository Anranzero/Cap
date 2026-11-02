use cap_rust::{pixels::Frame, settings::Theme};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::PCWSTR,
};

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub border: u32,
}
pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            bg: rgb(240, 237, 241),
            panel: rgb(252, 250, 252),
            text: rgb(32, 33, 38),
            muted: rgb(119, 121, 133),
            accent: rgb(8, 125, 255),
            border: rgb(230, 226, 232),
        },
        Theme::Dark => Palette {
            bg: rgb(19, 19, 21),
            panel: rgb(38, 38, 39),
            text: rgb(243, 242, 246),
            muted: rgb(165, 166, 176),
            accent: rgb(8, 125, 255),
            border: rgb(57, 57, 61),
        },
    }
}

pub struct Canvas {
    pub dc: HDC,
    hwnd: HWND,
    paint: PAINTSTRUCT,
}
impl Canvas {
    pub fn begin(hwnd: HWND) -> Self {
        let mut paint = PAINTSTRUCT::default();
        let dc = unsafe { BeginPaint(hwnd, &mut paint) };
        Self { dc, hwnd, paint }
    }
}
impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            let _ = EndPaint(self.hwnd, &self.paint);
        }
    }
}
pub fn client(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    r
}
pub fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}
pub fn fill(dc: HDC, rect: RECT, color: u32) {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        let _ = FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}
pub fn rounded(dc: HDC, rect: RECT, color: u32, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        let previous = SelectObject(dc, brush.into());
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(dc, pen);
        SelectObject(dc, previous);
        let _ = DeleteObject(brush.into());
    }
}
pub fn text(dc: HDC, text: &str, mut rect: RECT, color: u32, flags: u32) {
    if text.is_empty() || rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    let mut text: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(color));
        let old = SelectObject(dc, super::fonts::normal().into());
        DrawTextW(dc, &mut text, &mut rect, DRAW_TEXT_FORMAT(flags));
        SelectObject(dc, old);
    }
}
pub fn label(dc: HDC, value: &str, rect: RECT, color: u32) {
    text(
        dc,
        value,
        rect,
        color,
        (DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX).0,
    )
}
pub fn button(dc: HDC, label: &str, rect: RECT, color: u32, text_color: u32) {
    rounded(dc, rect, color, 10);
    text(
        dc,
        label,
        rect,
        text_color,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX).0,
    )
}
pub fn draw_frame(dc: HDC, frame: &Frame, rect: RECT) -> Result<(), String> {
    frame.validate()?;
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return Ok(());
    }
    let bitmap = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: frame.width as i32,
            biHeight: -(frame.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = unsafe {
        SetStretchBltMode(dc, HALFTONE);
        StretchDIBits(
            dc,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            0,
            0,
            frame.width as i32,
            frame.height as i32,
            Some(frame.pixels.as_ptr().cast()),
            &bitmap,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };
    if result == GDI_ERROR {
        return Err(format!("绘制图像失败：{}", std::io::Error::last_os_error()));
    }
    Ok(())
}
pub fn error(owner: HWND, message: &str) {
    let title = wide("cap · 操作未完成");
    let message = wide(message);
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}
pub fn get_text(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut text = vec![0u16; length + 1];
    let n = unsafe { GetWindowTextW(hwnd, &mut text) }.max(0) as usize;
    String::from_utf16_lossy(&text[..n])
}
pub fn set_text(hwnd: HWND, value: &str) {
    let value = wide(value);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(value.as_ptr()));
    }
}
pub fn point(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 as u16) as i16 as i32,
        ((lparam.0 >> 16) as u16) as i16 as i32,
    )
}
pub fn child(
    parent: HWND,
    class: &str,
    value: &str,
    id: u16,
    rect: RECT,
    style: WINDOW_STYLE,
) -> Result<HWND, String> {
    let class = wide(class);
    let value = wide(value);
    let h = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR(value.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(parent),
            Some(HMENU(id as usize as *mut _)),
            None,
            None,
        )
    }
    .map_err(|e| format!("创建控件失败：{e}"))?;
    unsafe {
        SendMessageW(
            h,
            WM_SETFONT,
            Some(WPARAM(super::fonts::normal().0 as usize)),
            Some(LPARAM(1)),
        );
    }
    Ok(h)
}
pub fn rect(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}
pub fn fit(frame: &Frame, available: RECT) -> RECT {
    let scale = ((available.right - available.left) as f64 / frame.width as f64)
        .min((available.bottom - available.top) as f64 / frame.height as f64)
        .max(0.001);
    let w = (frame.width as f64 * scale).round() as i32;
    let h = (frame.height as f64 * scale).round() as i32;
    rect(
        available.left + (available.right - available.left - w) / 2,
        available.top + (available.bottom - available.top - h) / 2,
        w,
        h,
    )
}
