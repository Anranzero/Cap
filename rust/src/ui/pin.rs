//! Native always-on-top screenshot pin, excluded from subsequent captures.
use super::{
    UiEvent, common,
    runtime::{self, WindowHandler, WindowOptions},
};
use cap_rust::{capture, geometry::Rect, pixels::Frame, settings::Settings};
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
            WindowsAndMessaging::*,
        },
    },
    core::PCWSTR,
};

const COPY: usize = 1;
const CLOSE: usize = 2;

pub fn show(frame: Frame, settings: Rc<RefCell<Settings>>) -> Result<HWND, String> {
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor).map_err(|e| format!("读取贴图位置失败：{e}"))?;
    }
    let bounds = Rect::new(cursor.x, cursor.y, frame.width, frame.height)?;
    show_at(frame, bounds, settings)
}

pub fn show_at(
    frame: Frame,
    bounds: Rect,
    settings: Rc<RefCell<Settings>>,
) -> Result<HWND, String> {
    frame.validate()?;
    bounds.validate()?;
    let monitor = work_area(POINT {
        x: bounds.x,
        y: bounds.y,
    })?;
    let scale = 1.0f64
        .min(1000.0 / frame.width as f64)
        .min((monitor.right - monitor.left) as f64 * 0.9 / frame.width as f64)
        .min((monitor.bottom - monitor.top) as f64 * 0.9 / frame.height as f64);
    let (width, height) = scaled_size(&frame, frame.width as f64 * scale, monitor);
    let x = bounds
        .x
        .clamp(monitor.left, (monitor.right - width).max(monitor.left));
    let y = bounds
        .y
        .clamp(monitor.top, (monitor.bottom - height).max(monitor.top));
    runtime::create(
        Box::new(Pin {
            frame,
            settings,
            original: (width, height),
            drag: None,
        }),
        WindowOptions {
            title: "Cap 贴图 · 拖动移动 · 滚轮缩放 · 双击恢复 · 右键菜单 · Esc 关闭".into(),
            rect: common::rect(x, y, width, height),
            style: WS_POPUP | WS_THICKFRAME,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            owner: None,
            visible: true,
        },
    )
}

fn work_area(point: POINT) -> Result<RECT, String> {
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(format!(
            "读取显示器工作区失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(info.rcWork)
}

fn scaled_size(frame: &Frame, desired_width: f64, work: RECT) -> (i32, i32) {
    let max_width = (work.right - work.left).max(64);
    let max_height = (work.bottom - work.top).max(48);
    let scale = (desired_width / frame.width as f64)
        .max((64.0 / frame.width as f64).min(48.0 / frame.height as f64))
        .min((max_width as f64 / frame.width as f64).min(max_height as f64 / frame.height as f64));
    (
        ((frame.width as f64 * scale).round() as i32).clamp(64, max_width),
        ((frame.height as f64 * scale).round() as i32).clamp(48, max_height),
    )
}

struct Pin {
    frame: Frame,
    settings: Rc<RefCell<Settings>>,
    original: (i32, i32),
    drag: Option<(POINT, RECT)>,
}

impl Pin {
    fn menu(&self, hwnd: HWND, point: POINT) -> Result<(), String> {
        let menu = unsafe { CreatePopupMenu() }.map_err(|e| format!("创建贴图菜单失败：{e}"))?;
        let result = (|| {
            let copy = common::wide("复制图片");
            let close = common::wide("关闭贴图\tEsc");
            unsafe {
                AppendMenuW(menu, MF_STRING, COPY, PCWSTR(copy.as_ptr()))
                    .map_err(|e| e.to_string())?;
                AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).map_err(|e| e.to_string())?;
                AppendMenuW(menu, MF_STRING, CLOSE, PCWSTR(close.as_ptr()))
                    .map_err(|e| e.to_string())?;
                let command = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    hwnd,
                    None,
                )
                .0 as usize;
                if command == COPY {
                    runtime::emit(UiEvent::CopyImage(self.frame.copy()?));
                }
                if command == CLOSE {
                    runtime::close(hwnd);
                }
            }
            Ok(())
        })();
        unsafe {
            DestroyMenu(menu).map_err(|e| format!("释放贴图菜单失败：{e}"))?;
        }
        result
    }
}

impl WindowHandler for Pin {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        capture::exclude_window(hwnd.0 as isize, true)
    }

    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                let rect = common::client(hwnd);
                let palette = common::palette(self.settings.borrow().theme);
                common::fill(canvas.dc, rect, palette.accent);
                let inside = RECT {
                    left: rect.left + 1,
                    top: rect.top + 1,
                    right: rect.right - 1,
                    bottom: rect.bottom - 1,
                };
                common::fill(canvas.dc, inside, 0);
                common::draw_frame(canvas.dc, &self.frame, common::fit(&self.frame, inside))?;
            }
            WM_LBUTTONDOWN => {
                let mut cursor = POINT::default();
                let mut rect = RECT::default();
                unsafe {
                    GetCursorPos(&mut cursor).map_err(|e| e.to_string())?;
                    GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
                    SetCapture(hwnd);
                }
                self.drag = Some((cursor, rect));
            }
            WM_MOUSEMOVE if self.drag.is_some() => {
                let (start, rect) = self.drag.unwrap();
                let mut cursor = POINT::default();
                unsafe {
                    GetCursorPos(&mut cursor).map_err(|e| e.to_string())?;
                    SetWindowPos(
                        hwnd,
                        None,
                        rect.left + cursor.x - start.x,
                        rect.top + cursor.y - start.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER,
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            WM_LBUTTONUP => {
                if self.drag.take().is_some() {
                    unsafe {
                        ReleaseCapture().map_err(|e| e.to_string())?;
                    }
                }
            }
            WM_CAPTURECHANGED => self.drag = None,
            WM_LBUTTONDBLCLK => unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    self.original.0,
                    self.original.1,
                    SWP_NOMOVE | SWP_NOZORDER,
                )
                .map_err(|e| e.to_string())?;
                common::invalidate(hwnd);
            },
            WM_MOUSEWHEEL => {
                let delta = ((w.0 >> 16) as u16) as i16;
                if delta == 0 {
                    return Ok(Some(LRESULT(0)));
                }
                let mut rect = RECT::default();
                unsafe {
                    GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
                }
                let work = work_area(POINT {
                    x: rect.left,
                    y: rect.top,
                })?;
                let desired = (rect.right - rect.left) as f64 * 1.1f64.powf(delta as f64 / 120.0);
                let (width, height) = scaled_size(&self.frame, desired, work);
                unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        rect.left
                            .clamp(work.left, (work.right - width).max(work.left)),
                        rect.top
                            .clamp(work.top, (work.bottom - height).max(work.top)),
                        width,
                        height,
                        SWP_NOZORDER,
                    )
                    .map_err(|e| e.to_string())?;
                }
                common::invalidate(hwnd);
            }
            WM_CONTEXTMENU => {
                let (x, y) = common::point(l);
                let mut point = POINT { x, y };
                if x == -1 && y == -1 {
                    unsafe {
                        GetCursorPos(&mut point).map_err(|e| e.to_string())?;
                    }
                }
                self.menu(hwnd, point)?;
            }
            WM_KEYDOWN if w.0 == 0x1b => runtime::close(hwnd),
            WM_KEYDOWN
                if w.0 == b'C' as usize
                    && unsafe {
                        windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11)
                    } < 0 =>
            {
                runtime::emit(UiEvent::CopyImage(self.frame.copy()?))
            }
            WM_GETMINMAXINFO => {
                let info = unsafe { &mut *(l.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize = POINT { x: 64, y: 48 };
            }
            WM_SIZE => common::invalidate(hwnd),
            WM_CLOSE => runtime::close(hwnd),
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pin_is_topmost_excluded_zoomable_resettable_and_escape_closes() {
        let frame = Frame::blank(160, 90, [0, 0, 255, 255]).unwrap();
        let hwnd = show_at(
            frame,
            Rect::new(100, 100, 160, 90).unwrap(),
            Rc::new(RefCell::new(Settings::default())),
        )
        .unwrap();
        let mut before = RECT::default();
        let mut after = RECT::default();
        unsafe {
            let mut affinity = 0;
            GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap();
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
            assert_ne!(
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0,
                0
            );
            GetWindowRect(hwnd, &mut before).unwrap();
            SendMessageW(hwnd, WM_MOUSEWHEEL, Some(WPARAM(120usize << 16)), None);
            GetWindowRect(hwnd, &mut after).unwrap();
            assert!(after.right - after.left > before.right - before.left);
            SendMessageW(hwnd, WM_LBUTTONDBLCLK, None, None);
            SendMessageW(hwnd, WM_LBUTTONUP, None, None);
            GetWindowRect(hwnd, &mut after).unwrap();
            assert_eq!(after.right - after.left, before.right - before.left);
            assert_eq!(after.bottom - after.top, before.bottom - before.top);
            SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(0x1b)), None);
            assert!(!IsWindow(Some(hwnd)).as_bool());
        }
        assert!(runtime::next_event().is_none());
    }

    #[test]
    fn pin_zoom_keeps_aspect_ratio_and_fits_negative_origin_monitor() {
        let frame = Frame::blank(1600, 900, [0; 4]).unwrap();
        let work = common::rect(-1920, -200, 1920, 1080);
        assert_eq!(scaled_size(&frame, 800.0, work), (800, 450));
        assert_eq!(scaled_size(&frame, 8000.0, work), (1920, 1080));
        assert_eq!(scaled_size(&frame, 1.0, work), (64, 48));
    }
}
