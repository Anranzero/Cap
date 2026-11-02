//! Physical-pixel desktop capture. DIB memory is always top-down BGRA.
use crate::{geometry::Rect, pixels::Frame};
use std::{ffi::c_void, mem::size_of};
#[cfg(test)]
pub(crate) mod native_tests;
#[cfg(test)]
pub(crate) mod test_window;
mod wgc;

#[derive(Clone, Copy, Debug)]
pub struct CaptureTarget {
    pub bounds: Rect,
    pub window_handle: isize,
    pub prefer_graphics_capture: bool,
}
#[derive(Clone, Debug)]
pub struct Monitor {
    pub handle: isize,
    pub bounds: Rect,
}
#[derive(Clone, Debug)]
pub struct DesktopWindow {
    pub handle: isize,
    pub title: String,
    pub bounds: Rect,
}
pub const TARGET_CLOSED: &str = "The capture target window has closed";
pub struct CaptureSession {
    target: CaptureTarget,
    graphics: Option<wgc::GraphicsSource>,
    pub fallback_reason: Option<String>,
    _com: crate::media::ComGuard,
}
impl CaptureSession {
    pub fn new(target: CaptureTarget) -> Result<Self, String> {
        Rect::new(
            target.bounds.x,
            target.bounds.y,
            target.bounds.width,
            target.bounds.height,
        )?;
        if target.bounds.is_empty()
            || target.bounds.width > 32768
            || target.bounds.height > 32768
            || target.bounds.width as u64 * target.bounds.height as u64 * 4 > i32::MAX as u64
        {
            return Err("Capture dimensions are invalid or exceed supported buffer size".into());
        }
        if target.window_handle != 0 && !is_window(target.window_handle) {
            return Err(TARGET_CLOSED.into());
        }
        let mut s = Self {
            target,
            graphics: None,
            fallback_reason: None,
            _com: crate::media::ComGuard::new()?,
        };
        if target.prefer_graphics_capture {
            match wgc::GraphicsSource::new(target) {
                Ok(v) => s.graphics = v,
                Err(e) => s.fallback_reason = Some(e),
            }
        }
        Ok(s)
    }
    pub fn bounds(&self) -> Result<Rect, String> {
        if self.graphics.is_some() && self.target.window_handle != 0 {
            let r = window_bounds(self.target.window_handle)?;
            Rect::new(
                r.x,
                r.y,
                self.target.bounds.width,
                self.target.bounds.height,
            )
        } else {
            Ok(self.target.bounds)
        }
    }
    pub fn backend_name(&self) -> &str {
        if self.graphics.is_some() {
            "Windows Graphics Capture"
        } else {
            "GDI"
        }
    }
    pub fn capture(&mut self) -> Result<Frame, String> {
        if self.target.window_handle != 0 && !is_window(self.target.window_handle) {
            return Err(TARGET_CLOSED.into());
        }
        if let Some(g) = self.graphics.as_mut() {
            match g.capture() {
                Ok(f) => return Ok(f),
                Err(e) => {
                    if g.target_closed() {
                        return Err(TARGET_CLOSED.into());
                    }
                    eprintln!("WGC failed; switching to GDI: {e}");
                    self.fallback_reason = Some(e);
                    self.graphics = None;
                }
            }
        }
        screenshot(self.target.bounds)
    }
}
pub fn is_window(window: isize) -> bool {
    unsafe { native::IsWindow(window) != 0 }
}
pub fn virtual_bounds() -> Result<Rect, String> {
    let _dpi = DpiScope::new();
    unsafe {
        Rect::new(
            native::GetSystemMetrics(76),
            native::GetSystemMetrics(77),
            native::GetSystemMetrics(78) as u32,
            native::GetSystemMetrics(79) as u32,
        )
    }
}
pub fn monitors() -> Result<Vec<Monitor>, String> {
    unsafe extern "system" fn callback(
        m: isize,
        _: isize,
        r: *const NativeRect,
        data: isize,
    ) -> i32 {
        let v = unsafe { &mut *(data as *mut Vec<Monitor>) };
        if let Ok(bounds) = unsafe { (*r).rect() } {
            v.push(Monitor { handle: m, bounds });
        }
        1
    }
    let _dpi = DpiScope::new();
    let mut result = Vec::new();
    if unsafe {
        native::EnumDisplayMonitors(
            0,
            std::ptr::null(),
            Some(callback),
            &mut result as *mut _ as isize,
        )
    } == 0
    {
        return Err(last_error("Enumerating monitors"));
    }
    Ok(result)
}
pub fn window_bounds(window: isize) -> Result<Rect, String> {
    let _dpi = DpiScope::new();
    if !is_window(window) {
        return Err(TARGET_CLOSED.into());
    }
    let mut r = NativeRect::default();
    unsafe {
        let needs_fallback = native::DwmGetWindowAttribute(
            window,
            9,
            &mut r as *mut _ as *mut c_void,
            size_of::<NativeRect>() as u32,
        ) < 0
            || r.right <= r.left
            || r.bottom <= r.top;
        if needs_fallback && native::GetWindowRect(window, &mut r) == 0 {
            return Err(last_error("Reading capture window bounds"));
        }
    }
    r.rect()
}
fn describe(window: isize) -> Option<DesktopWindow> {
    unsafe {
        if native::IsWindowVisible(window) == 0
            || native::IsIconic(window) != 0
            || native::GetWindowLongPtrW(window, -20) & 0x80 != 0
        {
            return None;
        }
        let mut cloaked = 0u32;
        native::DwmGetWindowAttribute(window, 14, &mut cloaked as *mut _ as *mut c_void, 4);
        if cloaked != 0 {
            return None;
        }
        let mut affinity = 0;
        if native::GetWindowDisplayAffinity(window, &mut affinity) != 0 && affinity != 0 {
            return None;
        }
        let len = native::GetWindowTextLengthW(window);
        if len <= 0 || len > 32768 {
            return None;
        }
        let mut text = vec![0u16; len as usize + 1];
        let count = native::GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32);
        if count <= 0 {
            return None;
        }
        Some(DesktopWindow {
            handle: window,
            title: String::from_utf16_lossy(&text[..count as usize]),
            bounds: window_bounds(window).ok()?,
        })
    }
}
pub fn windows() -> Result<Vec<DesktopWindow>, String> {
    unsafe extern "system" fn callback(w: isize, p: isize) -> i32 {
        if let Some(d) = describe(w) {
            unsafe { (&mut *(p as *mut Vec<DesktopWindow>)).push(d) }
        }
        1
    }
    let _dpi = DpiScope::new();
    let mut result = Vec::new();
    if unsafe { native::EnumWindows(Some(callback), &mut result as *mut _ as isize) } == 0 {
        return Err(last_error("Enumerating desktop windows"));
    }
    Ok(result)
}
pub fn window_at(x: i32, y: i32) -> Option<DesktopWindow> {
    let _dpi = DpiScope::new();
    let window = unsafe { native::GetAncestor(native::WindowFromPoint(Point { x, y }), 2) };
    describe(window).or_else(|| {
        windows()
            .ok()?
            .into_iter()
            .find(|w| w.bounds.contains(x, y))
    })
}
pub fn exclude_window(window: isize, exclude: bool) -> Result<(), String> {
    if unsafe { native::SetWindowDisplayAffinity(window, if exclude { 0x11 } else { 0 }) } == 0 {
        Err(last_error("Updating capture exclusion"))
    } else {
        Ok(())
    }
}
pub fn screenshot(bounds: Rect) -> Result<Frame, String> {
    let _dpi = DpiScope::new();
    let dib = Dib::new(bounds.width, bounds.height)?;
    let virtual_screen = virtual_bounds()?;
    let visible = bounds.intersect(virtual_screen);
    let desktop = unsafe { native::GetDC(0) };
    if desktop == 0 {
        return Err(last_error("Acquiring desktop DC"));
    }
    let result = if visible.width > 0
        && visible.height > 0
        && unsafe {
            native::BitBlt(
                dib.dc,
                visible.x - bounds.x,
                visible.y - bounds.y,
                visible.width as i32,
                visible.height as i32,
                desktop,
                visible.x,
                visible.y,
                0x40cc0020,
            )
        } == 0
    {
        Err(last_error("Capturing desktop pixels"))
    } else {
        dib.frame()
    };
    unsafe {
        native::ReleaseDC(0, desktop);
    }
    result
}
pub(crate) fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Point {
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl NativeRect {
    fn rect(self) -> Result<Rect, String> {
        Rect::new(
            self.left,
            self.top,
            (i64::from(self.right) - i64::from(self.left))
                .try_into()
                .map_err(|_| "Invalid window width")?,
            (i64::from(self.bottom) - i64::from(self.top))
                .try_into()
                .map_err(|_| "Invalid window height")?,
        )
    }
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    size_image: u32,
    xppm: i32,
    yppm: i32,
    colors: u32,
    important: u32,
}
pub(crate) struct Dib {
    pub dc: isize,
    bitmap: isize,
    previous: isize,
    pub data: *mut u8,
    width: u32,
    height: u32,
    len: usize,
}
impl Dib {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or("DIB dimensions overflow")?;
        if width == 0 || height == 0 || width > 32768 || height > 32768 || len > i32::MAX as usize {
            return Err("DIB dimensions invalid".into());
        }
        let dc = unsafe { native::CreateCompatibleDC(0) };
        if dc == 0 {
            return Err(last_error("Creating DIB DC"));
        }
        let mut dib = Self {
            dc,
            bitmap: 0,
            previous: 0,
            data: std::ptr::null_mut(),
            width,
            height,
            len,
        };
        let info = BitmapInfo {
            size: size_of::<BitmapInfo>() as u32,
            width: width as i32,
            height: -(height as i32),
            planes: 1,
            bits: 32,
            compression: 0,
            size_image: len as u32,
            xppm: 0,
            yppm: 0,
            colors: 0,
            important: 0,
        };
        unsafe {
            dib.bitmap = native::CreateDIBSection(
                dc,
                &info,
                0,
                &mut dib.data as *mut _ as *mut *mut c_void,
                0,
                0,
            );
            if dib.bitmap == 0 || dib.data.is_null() {
                return Err(last_error("Allocating DIB"));
            }
            dib.previous = native::SelectObject(dc, dib.bitmap);
            if dib.previous == 0 || dib.previous == -1 {
                return Err(last_error("Selecting DIB"));
            }
            std::ptr::write_bytes(dib.data, 0, len);
        }
        Ok(dib)
    }
    pub fn copy_from(&self, frame: &Frame) -> Result<(), String> {
        frame.validate()?;
        if frame.width != self.width || frame.height != self.height {
            return Err("DIB frame dimensions differ".into());
        }
        unsafe { std::ptr::copy_nonoverlapping(frame.pixels.as_ptr(), self.data, self.len) }
        Ok(())
    }
    pub fn frame(&self) -> Result<Frame, String> {
        if unsafe { native::GdiFlush() } == 0 {
            return Err(last_error("Flushing GDI"));
        }
        let mut pixels = unsafe { std::slice::from_raw_parts(self.data, self.len) }.to_vec();
        for a in pixels.iter_mut().skip(3).step_by(4) {
            *a = 255;
        }
        Frame::new(self.width, self.height, pixels)
    }
}
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            if self.previous != 0 && self.previous != -1 {
                native::SelectObject(self.dc, self.previous);
            }
            if self.bitmap != 0 {
                native::DeleteObject(self.bitmap);
            }
            if self.dc != 0 {
                native::DeleteDC(self.dc);
            }
        }
    }
}
struct DpiScope(isize);
impl DpiScope {
    fn new() -> Self {
        Self(unsafe { native::SetThreadDpiAwarenessContext(-4) })
    }
}
impl Drop for DpiScope {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                native::SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
}
mod native {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetDC(w: isize) -> isize;
        pub fn ReleaseDC(w: isize, dc: isize) -> i32;
        pub fn GetSystemMetrics(i: i32) -> i32;
        pub fn EnumDisplayMonitors(
            dc: isize,
            r: *const NativeRect,
            callback: Option<
                unsafe extern "system" fn(isize, isize, *const NativeRect, isize) -> i32,
            >,
            p: isize,
        ) -> i32;
        pub fn EnumWindows(
            callback: Option<unsafe extern "system" fn(isize, isize) -> i32>,
            p: isize,
        ) -> i32;
        pub fn GetWindowRect(w: isize, r: *mut NativeRect) -> i32;
        pub fn IsWindow(w: isize) -> i32;
        pub fn IsWindowVisible(w: isize) -> i32;
        pub fn IsIconic(w: isize) -> i32;
        pub fn GetWindowLongPtrW(w: isize, i: i32) -> isize;
        pub fn GetWindowTextLengthW(w: isize) -> i32;
        pub fn GetWindowTextW(w: isize, t: *mut u16, len: i32) -> i32;
        pub fn WindowFromPoint(p: Point) -> isize;
        pub fn GetAncestor(w: isize, flags: u32) -> isize;
        pub fn GetWindowDisplayAffinity(w: isize, a: *mut u32) -> i32;
        pub fn SetWindowDisplayAffinity(w: isize, a: u32) -> i32;
        pub fn SetThreadDpiAwarenessContext(v: isize) -> isize;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn CreateCompatibleDC(dc: isize) -> isize;
        pub fn CreateDIBSection(
            dc: isize,
            info: *const BitmapInfo,
            usage: u32,
            data: *mut *mut c_void,
            section: isize,
            offset: u32,
        ) -> isize;
        pub fn SelectObject(dc: isize, obj: isize) -> isize;
        pub fn DeleteObject(obj: isize) -> i32;
        pub fn DeleteDC(dc: isize) -> i32;
        pub fn BitBlt(
            dst: isize,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            src: isize,
            sx: i32,
            sy: i32,
            op: u32,
        ) -> i32;
        pub fn GdiFlush() -> i32;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        pub fn DwmGetWindowAttribute(w: isize, a: u32, value: *mut c_void, size: u32) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dib_preserves_bgra_and_opaque_alpha() {
        let source = Frame::new(
            2,
            2,
            vec![1, 2, 3, 0, 4, 5, 6, 255, 7, 8, 9, 0, 10, 11, 12, 0],
        )
        .unwrap();
        let dib = Dib::new(2, 2).unwrap();
        dib.copy_from(&source).unwrap();
        let actual = dib.frame().unwrap();
        assert_eq!(&actual.pixels[..4], &[1, 2, 3, 255]);
        assert_eq!(&actual.pixels[8..12], &[7, 8, 9, 255]);
    }
    #[test]
    fn native_owned_window_capture_smoke() {
        native_tests::run_cases();
    }
}
