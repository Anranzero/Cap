use crate::{
    capture::{Dib, Point, last_error},
    geometry::Rect,
    pixels::Frame,
};
use std::{
    cell::RefCell,
    mem::size_of,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};
#[derive(Clone, Debug)]
pub struct OverlayOptions {
    pub show_cursor: bool,
    pub highlight_clicks: bool,
    pub show_coordinates: bool,
    pub relative_coordinates: bool,
    pub coordinate_font_size: u32,
    pub coordinate_color: String,
}
impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            show_cursor: true,
            highlight_clicks: true,
            show_coordinates: true,
            relative_coordinates: false,
            coordinate_font_size: 18,
            coordinate_color: "#FFFFFF".into(),
        }
    }
}
impl OverlayOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(8..=96).contains(&self.coordinate_font_size) {
            return Err("Coordinate font size must be 8–96".into());
        }
        let c = &self.coordinate_color;
        if c.len() != 7 || !c.starts_with('#') || u32::from_str_radix(&c[1..], 16).is_err() {
            return Err("Coordinate color must use #RRGGBB".into());
        }
        Ok(())
    }
}
pub struct OverlayProcessor {
    options: OverlayOptions,
    clicks: Option<ClickHook>,
}
impl OverlayProcessor {
    pub fn new(options: OverlayOptions) -> Result<Self, String> {
        options.validate()?;
        let clicks = if options.highlight_clicks {
            Some(ClickHook::new()?)
        } else {
            None
        };
        Ok(Self { options, clicks })
    }
    pub fn process(
        &mut self,
        mut frame: Frame,
        bounds: Rect,
        width: u32,
        height: u32,
    ) -> Result<Frame, String> {
        frame.validate()?;
        if let Some(h) = &self.clicks {
            if let Some(click) = h
                .latest
                .lock()
                .map_err(|_| "Mouse click state poisoned")?
                .clone()
            {
                draw_click(
                    &mut frame,
                    bounds,
                    click.x,
                    click.y,
                    click.time.elapsed().as_secs_f64(),
                );
            }
        }
        if self.options.show_cursor || self.options.show_coordinates {
            let dib = Dib::new(frame.width, frame.height)?;
            dib.copy_from(&frame)?;
            if self.options.show_cursor {
                draw_cursor(&dib, bounds)?;
            }
            if self.options.show_coordinates {
                draw_coordinates(&dib, &frame, bounds, &self.options)?;
            }
            frame = dib.frame()?;
        }
        if frame.width == width && frame.height == height {
            Ok(frame)
        } else {
            frame.resize_bilinear(width, height)
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct CursorInfo {
    size: u32,
    flags: u32,
    cursor: isize,
    point: Point,
}
#[repr(C)]
#[derive(Default)]
struct IconInfo {
    icon: i32,
    x: u32,
    y: u32,
    mask: isize,
    color: isize,
}
fn draw_cursor(dib: &Dib, bounds: Rect) -> Result<(), String> {
    let mut c = CursorInfo {
        size: size_of::<CursorInfo>() as u32,
        ..Default::default()
    };
    if unsafe { native::GetCursorInfo(&mut c) } == 0 {
        return Err(last_error("Reading native cursor"));
    }
    if c.flags & 1 == 0 || c.cursor == 0 {
        return Ok(());
    }
    let mut p = Point::default();
    if unsafe { native::GetPhysicalCursorPos(&mut p) } == 0 {
        return Err(last_error("Reading physical cursor position"));
    }
    draw_cursor_handle(dib, bounds, c.cursor, p.x, p.y)
}
fn draw_cursor_handle(
    dib: &Dib,
    bounds: Rect,
    cursor: isize,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let mut icon = IconInfo::default();
    if unsafe { native::GetIconInfo(cursor, &mut icon) } == 0 {
        return Err(last_error("Reading cursor hotspot"));
    }
    let result = if unsafe {
        native::DrawIconEx(
            dib.dc,
            x - bounds.x - icon.x as i32,
            y - bounds.y - icon.y as i32,
            cursor,
            0,
            0,
            0,
            0,
            3,
        )
    } == 0
    {
        Err(last_error("Drawing native cursor"))
    } else {
        Ok(())
    };
    unsafe {
        if icon.mask != 0 {
            native::DeleteObject(icon.mask);
        }
        if icon.color != 0 {
            native::DeleteObject(icon.color);
        }
    }
    result
}
fn draw_coordinates(
    dib: &Dib,
    frame: &Frame,
    bounds: Rect,
    options: &OverlayOptions,
) -> Result<(), String> {
    let mut p = Point::default();
    if unsafe { native::GetPhysicalCursorPos(&mut p) } == 0 {
        return Err(last_error("Reading coordinate pointer"));
    }
    let (x, y) = if options.relative_coordinates {
        (p.x - bounds.x, p.y - bounds.y)
    } else {
        (p.x, p.y)
    };
    let text = format!("X: {x}  Y: {y}").encode_utf16().collect::<Vec<_>>();
    let name = "Segoe UI\0".encode_utf16().collect::<Vec<_>>();
    let font = unsafe {
        native::CreateFontW(
            -(options.coordinate_font_size as i32),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            4,
            0,
            name.as_ptr(),
        )
    };
    if font == 0 {
        return Err(last_error("Creating coordinate font"));
    }
    let old = unsafe { native::SelectObject(dib.dc, font) };
    let mut size = Size::default();
    let result = (|| {
        if unsafe {
            native::GetTextExtentPoint32W(dib.dc, text.as_ptr(), text.len() as i32, &mut size)
        } == 0
        {
            return Err(last_error("Measuring coordinate text"));
        }
        let lx = (p.x - bounds.x + 20).clamp(4, (frame.width as i32 - size.cx - 4).max(4));
        let ly = (p.y - bounds.y + 24).clamp(4, (frame.height as i32 - size.cy - 4).max(4));
        let value =
            u32::from_str_radix(&options.coordinate_color[1..], 16).map_err(|e| e.to_string())?;
        let color = ((value >> 16) & 255) | (value & 0x00ff00) | ((value & 255) << 16);
        unsafe {
            native::SetBkColor(dib.dc, 0x141414);
            native::SetTextColor(dib.dc, color);
            native::SetBkMode(dib.dc, 2);
        }
        if unsafe { native::TextOutW(dib.dc, lx, ly, text.as_ptr(), text.len() as i32) } == 0 {
            return Err(last_error("Drawing coordinate text"));
        }
        Ok(())
    })();
    unsafe {
        native::SelectObject(dib.dc, old);
        native::DeleteObject(font);
    }
    result
}
pub fn draw_click(frame: &mut Frame, bounds: Rect, x: i32, y: i32, age_seconds: f64) {
    if !(0.0..=0.5).contains(&age_seconds) {
        return;
    }
    let radius = 16. + age_seconds * 38.;
    let cx = (x - bounds.x) as f64;
    let cy = (y - bounds.y) as f64;
    let alpha = (220. * (1. - age_seconds / 0.5)) as u32;
    let l = (cx - radius - 2.).max(0.) as u32;
    let r = (cx + radius + 2.).min(frame.width as f64) as u32;
    let t = (cy - radius - 2.).max(0.) as u32;
    let b = (cy + radius + 2.).min(frame.height as f64) as u32;
    for yy in t..b {
        for xx in l..r {
            let d = ((xx as f64 - cx).powi(2) + (yy as f64 - cy).powi(2)).sqrt();
            if d > radius + 1.5 {
                continue;
            }
            let a = if d >= radius - 1.5 { alpha } else { alpha / 3 };
            let i = (yy as usize * frame.width as usize + xx as usize) * 4;
            for (ch, target) in [0u32, 185, 255].iter().enumerate() {
                frame.pixels[i + ch] =
                    ((frame.pixels[i + ch] as u32 * (255 - a) + target * a) / 255) as u8;
            }
            frame.pixels[i + 3] = 255;
        }
    }
}
#[derive(Clone)]
struct Click {
    x: i32,
    y: i32,
    time: Instant,
}
type Latest = Arc<Mutex<Option<Click>>>;
thread_local! {static HOOK_LATEST:RefCell<Option<Latest>>=const{RefCell::new(None)};}
struct ClickHook {
    latest: Latest,
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}
#[repr(C)]
#[derive(Default)]
struct Message {
    window: isize,
    id: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
    private: u32,
}
#[repr(C)]
struct MouseData {
    point: Point,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
impl ClickHook {
    fn new() -> Result<Self, String> {
        let latest = Arc::new(Mutex::new(None));
        let state = latest.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("Cap mouse overlay".into())
            .spawn(move || {
                let id = unsafe { native::GetCurrentThreadId() };
                let mut message = Message::default();
                unsafe {
                    native::PeekMessageW(&mut message, 0, 0, 0, 0);
                }
                HOOK_LATEST.with(|s| *s.borrow_mut() = Some(state));
                let hook = unsafe { native::SetWindowsHookExW(14, Some(mouse_callback), 0, 0) };
                if hook == 0 {
                    let _ = tx.send(Err(last_error("Installing mouse click hook")));
                    return;
                }
                if tx.send(Ok(id)).is_err() {
                    unsafe {
                        native::UnhookWindowsHookEx(hook);
                    }
                    return;
                }
                loop {
                    let result = unsafe { native::GetMessageW(&mut message, 0, 0, 0) };
                    if result <= 0 {
                        if result < 0 {
                            eprintln!(
                                "Mouse hook message pump: {}",
                                std::io::Error::last_os_error()
                            )
                        }
                        break;
                    }
                    unsafe {
                        native::TranslateMessage(&message);
                        native::DispatchMessageW(&message);
                    }
                }
                if unsafe { native::UnhookWindowsHookEx(hook) } == 0 {
                    eprintln!("Removing mouse hook: {}", std::io::Error::last_os_error())
                }
            })
            .map_err(|e| e.to_string())?;
        let thread_id = rx.recv().map_err(|e| e.to_string())??;
        Ok(Self {
            latest,
            thread_id,
            worker: Some(worker),
        })
    }
}
unsafe extern "system" fn mouse_callback(code: i32, message: usize, data: isize) -> isize {
    if code >= 0 && matches!(message, 0x201 | 0x204 | 0x207 | 0x20b) {
        let p = unsafe { &*(data as *const MouseData) };
        HOOK_LATEST.with(|v| {
            if let Some(s) = v.borrow().as_ref() {
                if let Ok(mut latest) = s.lock() {
                    *latest = Some(Click {
                        x: p.point.x,
                        y: p.point.y,
                        time: Instant::now(),
                    })
                }
            }
        });
    }
    unsafe { native::CallNextHookEx(0, code, message, data) }
}
impl Drop for ClickHook {
    fn drop(&mut self) {
        if unsafe { native::PostThreadMessageW(self.thread_id, 0x12, 0, 0) } == 0 {
            eprintln!(
                "Stopping mouse hook thread: {}",
                std::io::Error::last_os_error()
            );
            return;
        }
        if let Some(w) = self.worker.take() {
            if w.join().is_err() {
                eprintln!("Mouse overlay thread panicked")
            }
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct Size {
    cx: i32,
    cy: i32,
}
mod native {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetCursorInfo(c: *mut CursorInfo) -> i32;
        pub fn GetPhysicalCursorPos(p: *mut Point) -> i32;
        pub fn GetIconInfo(cursor: isize, i: *mut IconInfo) -> i32;
        pub fn DrawIconEx(
            dc: isize,
            x: i32,
            y: i32,
            c: isize,
            w: i32,
            h: i32,
            step: u32,
            brush: isize,
            flags: u32,
        ) -> i32;
        pub fn SetWindowsHookExW(
            kind: i32,
            callback: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
            module: isize,
            thread: u32,
        ) -> isize;
        pub fn UnhookWindowsHookEx(hook: isize) -> i32;
        pub fn CallNextHookEx(h: isize, c: i32, w: usize, l: isize) -> isize;
        pub fn GetMessageW(m: *mut Message, w: isize, min: u32, max: u32) -> i32;
        pub fn PeekMessageW(m: *mut Message, w: isize, min: u32, max: u32, remove: u32) -> i32;
        pub fn TranslateMessage(m: *const Message) -> i32;
        pub fn DispatchMessageW(m: *const Message) -> isize;
        pub fn PostThreadMessageW(id: u32, message: u32, w: usize, l: isize) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn CreateFontW(
            h: i32,
            w: i32,
            e: i32,
            o: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strike: u32,
            charset: u32,
            out: u32,
            clip: u32,
            quality: u32,
            pitch: u32,
            name: *const u16,
        ) -> isize;
        pub fn GetTextExtentPoint32W(dc: isize, text: *const u16, len: i32, size: *mut Size)
        -> i32;
        pub fn TextOutW(dc: isize, x: i32, y: i32, text: *const u16, len: i32) -> i32;
        pub fn SetTextColor(dc: isize, color: u32) -> u32;
        pub fn SetBkColor(dc: isize, color: u32) -> u32;
        pub fn SetBkMode(dc: isize, mode: i32) -> i32;
        pub fn SelectObject(dc: isize, o: isize) -> isize;
        pub fn DeleteObject(o: isize) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetCurrentThreadId() -> u32;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_monochrome_cursor_uses_its_native_hotspot() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn CreateCursor(
                instance: isize,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                and: *const u8,
                xor: *const u8,
            ) -> isize;
            fn DestroyCursor(cursor: isize) -> i32;
        }
        let and = [0u8; 128];
        let xor = [255u8; 128];
        let cursor = unsafe { CreateCursor(0, 4, 6, 32, 32, and.as_ptr(), xor.as_ptr()) };
        assert_ne!(cursor, 0);
        struct OwnedCursor(isize);
        impl Drop for OwnedCursor {
            fn drop(&mut self) {
                assert_ne!(unsafe { DestroyCursor(self.0) }, 0)
            }
        }
        let cursor = OwnedCursor(cursor);
        let dib = Dib::new(128, 128).unwrap();
        dib.copy_from(&Frame::blank(128, 128, [128, 128, 128, 255]).unwrap())
            .unwrap();
        draw_cursor_handle(
            &dib,
            Rect::new(100, 100, 128, 128).unwrap(),
            cursor.0,
            164,
            164,
        )
        .unwrap();
        let frame = dib.frame().unwrap();
        assert_eq!(
            &frame.pixels[(58 * 128 + 60) * 4..(58 * 128 + 60) * 4 + 4],
            &[255, 255, 255, 255]
        );
        assert_eq!(
            &frame.pixels[(57 * 128 + 60) * 4..(57 * 128 + 60) * 4 + 4],
            &[128, 128, 128, 255]
        );
        assert_eq!(
            frame.pixels.chunks_exact(4).filter(|p| p[0] == 255).count(),
            1024
        );
    }
    #[test]
    fn coordinate_font_and_color_are_drawn_natively() {
        let options = OverlayOptions {
            show_cursor: false,
            highlight_clicks: false,
            show_coordinates: true,
            coordinate_color: "#00FF00".into(),
            ..Default::default()
        };
        let mut processor = OverlayProcessor::new(options).unwrap();
        let frame = processor
            .process(
                Frame::blank(320, 128, [0, 0, 0, 255]).unwrap(),
                Rect::new(0, 0, 320, 128).unwrap(),
                320,
                128,
            )
            .unwrap();
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .filter(|p| p[1] > 80 && p[0] < 30 && p[2] < 30)
                .count()
                > 20
        );
    }
    #[test]
    fn native_mouse_hook_starts_and_stops_cleanly() {
        let options = OverlayOptions {
            show_cursor: false,
            show_coordinates: false,
            highlight_clicks: true,
            ..Default::default()
        };
        let processor = OverlayProcessor::new(options).unwrap();
        drop(processor);
    }
    #[test]
    fn click_pulse_is_visible_and_expires() {
        let bounds = Rect::new(-100, -100, 64, 64).unwrap();
        let mut f = Frame::new(64, 64, vec![0; 64 * 64 * 4]).unwrap();
        draw_click(&mut f, bounds, -68, -68, 0.1);
        assert!(
            f.pixels
                .chunks_exact(4)
                .filter(|p| p[2] > 50 && p[1] > 30)
                .count()
                > 20
        );
        let before = f.pixels.clone();
        draw_click(&mut f, bounds, -68, -68, 0.6);
        assert_eq!(before, f.pixels);
    }
}
