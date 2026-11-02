//! Feishu scrolling-capture starter: a pill under the selection that only starts the
//! scroll when clicked, plus a help toggle (screenshot capture never auto-starts).
use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
    toolbar::TOOL_COMMAND,
};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::*, WindowsAndMessaging::*},
};

pub const WIDTH: i32 = 387;
pub const HEIGHT: i32 = 67;
/// Command ids posted to the selector window.
pub const CMD_START: usize = 30;
pub const CMD_HELP: usize = 31;

pub fn show(target: HWND) -> Result<HWND, String> {
    runtime::create(
        Box::new(Pill {
            target,
            hover: None,
            chrome: None,
            back_buffer: None,
        }),
        WindowOptions {
            title: "cap · 滚动截图".into(),
            rect: common::rect(20, 20, WIDTH, HEIGHT),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}

struct Pill {
    target: HWND,
    hover: Option<usize>,
    chrome: Option<Chrome>,
    back_buffer: Option<PaintBuffer>,
}

struct Chrome {
    tooltip: HWND,
    texts: Vec<Vec<u16>>,
}
impl Chrome {
    fn new(owner: HWND, entries: &[(RECT, &str)]) -> Result<Self, String> {
        let mut chrome = Self {
            tooltip: HWND::default(),
            texts: entries.iter().map(|(_, t)| common::wide(t)).collect(),
        };
        let init = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        if !unsafe { InitCommonControlsEx(&init) }.as_bool() {
            return Err("初始化滚动条提示控件失败。".into());
        }
        chrome.tooltip = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                TOOLTIPS_CLASSW,
                windows::core::PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                None,
                None,
            )
        }
        .map_err(|e| format!("创建滚动条提示失败：{e}"))?;
        for (index, (rect, _)) in entries.iter().enumerate() {
            let tool = TTTOOLINFOW {
                cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
                uFlags: TTF_SUBCLASS | TTF_TRANSPARENT,
                hwnd: owner,
                uId: index + 1,
                rect: *rect,
                lpszText: windows::core::PWSTR(chrome.texts[index].as_mut_ptr()),
                ..Default::default()
            };
            unsafe {
                SendMessageW(
                    chrome.tooltip,
                    TTM_ADDTOOLW,
                    Some(WPARAM(0)),
                    Some(LPARAM(&tool as *const TTTOOLINFOW as isize)),
                );
            }
        }
        Ok(chrome)
    }
}
impl Drop for Chrome {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.tooltip)).as_bool() {
                let _ = DestroyWindow(self.tooltip);
            }
        }
    }
}

fn start_zone() -> RECT {
    common::rect(0, 0, 268, HEIGHT)
}
fn help_zone() -> RECT {
    common::rect(268, 0, WIDTH - 268, HEIGHT)
}

impl Pill {
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        if !(0..HEIGHT).contains(&y) {
            return None;
        }
        for (index, zone) in [start_zone(), help_zone()].iter().enumerate() {
            if x >= zone.left && x < zone.right {
                return Some(index);
            }
        }
        None
    }
    fn paint(&mut self, hwnd: HWND) -> Result<(), String> {
        let canvas = Canvas::begin(hwnd);
        let rect = common::client(hwnd);
        if rect.right <= 0 || rect.bottom <= 0 {
            return Ok(());
        }
        if self
            .back_buffer
            .as_ref()
            .is_none_or(|buffer| !buffer.matches(rect.right as u32, rect.bottom as u32))
        {
            self.back_buffer = Some(PaintBuffer::new(
                canvas.dc,
                rect.right as u32,
                rect.bottom as u32,
            )?);
        }
        let c = self.back_buffer.as_ref().ok_or("滚动条绘图缓存未创建。")?;
        common::rounded(c.dc, rect, common::rgb(0x29, 0x29, 0x29), 12);
        if let Some(index) = self.hover {
            let zone = if index == 0 {
                start_zone()
            } else {
                help_zone()
            };
            common::rounded(c.dc, zone, common::rgb(0x35, 0x35, 0x37), 10);
        }
        // Play triangle.
        unsafe {
            let brush = CreateSolidBrush(COLORREF(common::rgb(0xE8, 0xE8, 0xE8)));
            let previous = SelectObject(c.dc, brush.into());
            let pen = SelectObject(c.dc, GetStockObject(NULL_PEN));
            let points = [
                POINT { x: 22, y: 24 },
                POINT { x: 22, y: 44 },
                POINT { x: 38, y: 34 },
            ];
            let _ = Polygon(c.dc, &points);
            SelectObject(c.dc, pen);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(brush.into());
        }
        common::label(
            c.dc,
            "开始滚动截图",
            common::rect(52, 0, 200, HEIGHT),
            common::rgb(0xE8, 0xE8, 0xE8),
        );
        common::fill(
            c.dc,
            common::rect(272, (HEIGHT - 26) / 2, 1, 26),
            common::rgb(0x3F, 0x3F, 0x3F),
        );
        // Info glyph.
        unsafe {
            let pen = CreatePen(PS_SOLID, 2, COLORREF(common::rgb(0xC8, 0xC8, 0xC8)));
            let previous = SelectObject(c.dc, pen.into());
            let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
            let _ = Ellipse(c.dc, 296, 25, 314, 43);
            SelectObject(c.dc, brush);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(pen.into());
            common::fill(
                c.dc,
                common::rect(304, 30, 2, 6),
                common::rgb(0xC8, 0xC8, 0xC8),
            );
            common::fill(
                c.dc,
                common::rect(304, 38, 2, 2),
                common::rgb(0xC8, 0xC8, 0xC8),
            );
        }
        common::label(
            c.dc,
            "帮助",
            common::rect(322, 0, 60, HEIGHT),
            common::rgb(0xC8, 0xC8, 0xC8),
        );
        c.present(canvas.dc)
    }
    fn click(&self, index: usize) -> Result<(), String> {
        let id = if index == 0 { CMD_START } else { CMD_HELP };
        unsafe { PostMessageW(Some(self.target), TOOL_COMMAND, WPARAM(id), LPARAM(0)) }
            .map_err(|e| e.to_string())
    }
}

impl WindowHandler for Pill {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        self.chrome = Some(Chrome::new(
            hwnd,
            &[(start_zone(), "开始滚动截图"), (help_zone(), "帮助")],
        )?);
        Ok(())
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        _w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_LBUTTONUP => {
                let (x, y) = common::point(l);
                if let Some(index) = self.hit(x, y) {
                    self.click(index)?;
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_MOUSEMOVE => {
                let (x, y) = common::point(l);
                let hover = self.hit(x, y);
                if self.hover != hover {
                    self.hover = hover;
                    common::invalidate(hwnd);
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_SETCURSOR => {
                unsafe {
                    let _ = SetCursor(Some(
                        LoadCursorW(None, IDC_HAND).map_err(|e| e.to_string())?,
                    ));
                }
                return Ok(Some(LRESULT(1)));
            }
            WM_PAINT => {
                self.paint(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            _ => {}
        }
        Ok(None)
    }
}
