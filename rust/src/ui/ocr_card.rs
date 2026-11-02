//! Feishu-style inline OCR card: recognized text stays in the overlay with a
//! close glyph and 编辑 / 复制 actions instead of being copied silently.
use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
    toolbar::TOOL_COMMAND,
};
use cap_rust::settings::Theme;
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

/// 340x326dp at 150% scaling.
pub const WIDTH: i32 = 510;
pub const HEIGHT: i32 = 489;

pub const CMD_CLOSE: usize = 34;
pub const CMD_COPY: usize = 35;
pub const CMD_EDIT: usize = 36;

const BUTTON_W: i32 = 68;
const BUTTON_H: i32 = 36;
const CLOSE: i32 = 34;

pub struct OcrState {
    pub text: String,
    pub busy: bool,
    pub theme: Theme,
}
impl OcrState {
    pub fn new(theme: Theme) -> Self {
        Self {
            text: String::new(),
            busy: false,
            theme,
        }
    }
}

pub fn show(target: HWND, state: Rc<RefCell<OcrState>>) -> Result<HWND, String> {
    runtime::create(
        Box::new(Card {
            target,
            state,
            hover: None,
            back_buffer: None,
        }),
        WindowOptions {
            title: "cap · 提取文字".into(),
            rect: common::rect(20, 20, WIDTH, HEIGHT),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}

fn edit_zone() -> RECT {
    common::rect(
        WIDTH - 16 - BUTTON_W * 2 - 8,
        HEIGHT - 16 - BUTTON_H,
        BUTTON_W,
        BUTTON_H,
    )
}
fn copy_zone() -> RECT {
    common::rect(
        WIDTH - 16 - BUTTON_W,
        HEIGHT - 16 - BUTTON_H,
        BUTTON_W,
        BUTTON_H,
    )
}
fn close_zone() -> RECT {
    common::rect(WIDTH - 12 - CLOSE, 8, CLOSE, CLOSE)
}

struct Card {
    target: HWND,
    state: Rc<RefCell<OcrState>>,
    hover: Option<usize>,
    back_buffer: Option<PaintBuffer>,
}

impl Card {
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        for (index, zone) in [close_zone(), edit_zone(), copy_zone()].iter().enumerate() {
            if x >= zone.left && x < zone.right && y >= zone.top && y < zone.bottom {
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
        let c = self
            .back_buffer
            .as_ref()
            .ok_or("OCR 卡片绘图缓存未创建。")?;
        let state = self.state.borrow();
        let light = state.theme == Theme::Light;
        let panel = if light {
            common::rgb(0xFF, 0xFF, 0xFF)
        } else {
            common::rgb(0x1A, 0x1A, 0x1A)
        };
        let border_colour = if light {
            common::rgb(0xE4, 0xE5, 0xE6)
        } else {
            common::rgb(0x3C, 0x3C, 0x3C)
        };
        let title_colour = if light {
            common::rgb(0x1F, 0x23, 0x29)
        } else {
            common::rgb(0xFF, 0xFF, 0xFF)
        };
        let body_colour = title_colour;
        let hint_colour = if light {
            common::rgb(0x64, 0x6A, 0x73)
        } else {
            common::rgb(0xC8, 0xC8, 0xC8)
        };
        common::rounded(c.dc, rect, panel, 12);
        let border = unsafe { CreatePen(PS_SOLID, 1, COLORREF(border_colour)) };
        let previous = unsafe { SelectObject(c.dc, border.into()) };
        unsafe {
            let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
            let _ = RoundRect(c.dc, 0, 0, rect.right, rect.bottom, 12, 12);
            SelectObject(c.dc, brush);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(border.into());
        }
        common::label(
            c.dc,
            "提取文字",
            common::rect(18, 10, 200, 30),
            title_colour,
        );
        // Close glyph
        if self.hover == Some(0) {
            common::rounded(c.dc, close_zone(), common::rgb(0x2E, 0x2E, 0x2E), 6);
        }
        unsafe {
            let pen = CreatePen(PS_SOLID, 2, COLORREF(common::rgb(0xC8, 0xC8, 0xC8)));
            let previous = SelectObject(c.dc, pen.into());
            let glyph = close_zone();
            let _ = MoveToEx(c.dc, glyph.left + 11, glyph.top + 11, None);
            let _ = LineTo(c.dc, glyph.right - 11, glyph.bottom - 11);
            let _ = MoveToEx(c.dc, glyph.right - 11, glyph.top + 11, None);
            let _ = LineTo(c.dc, glyph.left + 11, glyph.bottom - 11);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(pen.into());
        }
        let body = common::rect(18, 46, WIDTH - 36, HEIGHT - 16 - BUTTON_H - 60);
        if state.busy {
            common::label(c.dc, "正在识别文字…", body, common::rgb(0xC8, 0xC8, 0xC8));
        } else if state.text.trim().is_empty() {
            common::text(
                c.dc,
                "未识别到文字，可重新框选清晰的文字区域。",
                body,
                hint_colour,
                (DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX).0,
            );
        } else {
            common::text(
                c.dc,
                &state.text,
                body,
                body_colour,
                (DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX | DT_EDITCONTROL).0,
            );
        }
        for (index, (zone, label)) in [(edit_zone(), "编辑"), (copy_zone(), "复制")]
            .into_iter()
            .enumerate()
        {
            let hovered = self.hover == Some(index + 1);
            common::rounded(
                c.dc,
                zone,
                if hovered {
                    common::rgb(0x2C, 0x63, 0xD2)
                } else {
                    common::rgb(0x33, 0x70, 0xEB)
                },
                8,
            );
            common::text(
                c.dc,
                label,
                zone,
                common::rgb(0xFF, 0xFF, 0xFF),
                (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX).0,
            );
        }
        c.present(canvas.dc)
    }
    fn click(&self, index: usize) -> Result<(), String> {
        let id = match index {
            0 => CMD_CLOSE,
            1 => CMD_EDIT,
            _ => CMD_COPY,
        };
        unsafe { PostMessageW(Some(self.target), TOOL_COMMAND, WPARAM(id), LPARAM(0)) }
            .map_err(|e| e.to_string())
    }
}

impl WindowHandler for Card {
    fn created(&mut self, _hwnd: HWND) -> Result<(), String> {
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
