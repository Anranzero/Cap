//! Feishu colour picker: a small magnifier that follows the cursor and shows the
//! sampled coordinate plus RGB/HEX value (Ctrl+C copies it, Shift toggles format).
use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
};
use cap_rust::{pixels::Frame, settings::Theme};
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

/// Feishu's picker is 145x171dp; at 150% scaling that is 218x257 physical pixels.
pub const WIDTH: i32 = 218;
pub const HEIGHT: i32 = 257;
const ZOOM_HEIGHT: i32 = 180;
/// Sampled pixels across the magnified area (Feishu shows a small 9x9 neighbourhood).
const SAMPLE: i32 = 9;
const PANEL: u32 = 0x1A_1A_1A;

pub struct MagnifierState {
    pub frame: Option<Frame>,
    pub cursor: (i32, i32),
    pub hex: bool,
    pub theme: Theme,
}
impl MagnifierState {
    pub fn new(frame: Frame, theme: Theme) -> Self {
        Self {
            frame: Some(frame),
            cursor: (0, 0),
            hex: false,
            theme,
        }
    }
    pub fn sample(&self) -> Option<[u8; 3]> {
        let frame = self.frame.as_ref()?;
        let (x, y) = self.cursor;
        if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 {
            return None;
        }
        let index = ((y as usize) * frame.width as usize + x as usize) * 4;
        let pixel = &frame.pixels[index..index + 4];
        Some([pixel[2], pixel[1], pixel[0]])
    }
    pub fn value_text(&self) -> String {
        match self.sample() {
            Some([r, g, b]) if self.hex => format!("#{r:02X}{g:02X}{b:02X}"),
            Some([r, g, b]) => format!("{r},{g},{b}"),
            None => String::new(),
        }
    }
}

pub fn show(target: HWND, state: Rc<RefCell<MagnifierState>>) -> Result<HWND, String> {
    runtime::create(
        Box::new(Magnifier {
            state,
            back_buffer: None,
        }),
        WindowOptions {
            title: "cap · 取色".into(),
            rect: common::rect(20, 20, WIDTH, HEIGHT),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}

struct Magnifier {
    state: Rc<RefCell<MagnifierState>>,
    back_buffer: Option<PaintBuffer>,
}

impl Magnifier {
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
        let c = self.back_buffer.as_ref().ok_or("取色器绘图缓存未创建。")?;
        let state = self.state.borrow();
        let light = state.theme == Theme::Light;
        let panel = if light {
            common::rgb(0xFF, 0xFF, 0xFF)
        } else {
            PANEL
        };
        let text_colour = if light {
            common::rgb(0x1F, 0x23, 0x29)
        } else {
            common::rgb(0xFF, 0xFF, 0xFF)
        };
        let hint_colour = if light {
            common::rgb(0x64, 0x6A, 0x73)
        } else {
            common::rgb(0xC8, 0xC8, 0xC8)
        };
        common::fill(c.dc, common::rect(0, 0, rect.right, rect.bottom), panel);
        if let Some(frame) = state.frame.as_ref() {
            let left = state.cursor.0 - SAMPLE / 2;
            let top = state.cursor.1 - SAMPLE / 2;
            draw_zoom(c.dc, frame, left, top, rect.right, ZOOM_HEIGHT)?;
        }
        // White centre marker (~10px box, like Feishu) marks the sampled pixel.
        let marker = 10;
        let marker_colour = if light {
            common::rgb(0x1F, 0x23, 0x29)
        } else {
            common::rgb(0xFF, 0xFF, 0xFF)
        };
        let cx = rect.right / 2;
        let cy = ZOOM_HEIGHT / 2;
        for (x, y, w, h) in [
            (cx - marker, cy - marker, marker * 2, 2),
            (cx - marker, cy + marker - 2, marker * 2, 2),
            (cx - marker, cy - marker, 2, marker * 2),
            (cx + marker - 2, cy - marker, 2, marker * 2),
        ] {
            common::fill(c.dc, common::rect(x, y, w, h), marker_colour);
        }
        // 1px border keeps the plate readable over bright screenshots.
        let border_colour = if light {
            common::rgb(0xE4, 0xE5, 0xE6)
        } else {
            common::rgb(0x4A, 0x4A, 0x4A)
        };
        let border = unsafe { CreatePen(PS_SOLID, 1, COLORREF(border_colour)) };
        let previous = unsafe { SelectObject(c.dc, border.into()) };
        unsafe {
            let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
            let _ = Rectangle(c.dc, 0, 0, rect.right, rect.bottom);
            SelectObject(c.dc, brush);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(border.into());
        }
        // Readout rows: coordinate, value, and the two shortcuts.
        let value = match state.sample() {
            Some([r, g, b]) if state.hex => format!("HEX: #{r:02X}{g:02X}{b:02X}"),
            Some([r, g, b]) => format!("RGB: {r},{g},{b}"),
            None => "RGB: -".into(),
        };
        let lines = [
            format!("坐标: {},{}", state.cursor.0, state.cursor.1),
            value,
            "按 Ctrl + C 复制色值".into(),
            "按 Shift 切换 RGB/HEX".into(),
        ];
        let mut y = ZOOM_HEIGHT + 6;
        for (index, line) in lines.iter().enumerate() {
            let colour = if index < 2 { text_colour } else { hint_colour };
            common::label(c.dc, line, common::rect(12, y, rect.right - 16, 20), colour);
            y += 18;
        }
        c.present(canvas.dc)
    }
}

fn draw_zoom(
    dc: HDC,
    frame: &Frame,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    let size = (SAMPLE * SAMPLE) as usize;
    let mut buffer = vec![0u8; size * 4];
    for offset_y in 0..SAMPLE {
        for offset_x in 0..SAMPLE {
            let x = (left + offset_x).clamp(0, frame.width as i32 - 1) as usize;
            let y = (top + offset_y).clamp(0, frame.height as i32 - 1) as usize;
            let source = (y * frame.width as usize + x) * 4;
            let target = ((offset_y * SAMPLE + offset_x) as usize) * 4;
            buffer[target..target + 4].copy_from_slice(&frame.pixels[source..source + 4]);
        }
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SAMPLE,
            biHeight: -SAMPLE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = unsafe {
        SetStretchBltMode(dc, COLORONCOLOR);
        StretchDIBits(
            dc,
            0,
            0,
            width,
            height,
            0,
            0,
            SAMPLE,
            SAMPLE,
            Some(buffer.as_ptr().cast()),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };
    if result == GDI_ERROR {
        return Err(format!(
            "绘制取色放大区失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Moves the picker next to the cursor, flipping near screen edges.
pub fn place(window: HWND, cursor: (i32, i32), work: RECT) {
    let mut x = cursor.0 - WIDTH - 16;
    if x < work.left {
        x = cursor.0 + 18;
    }
    let mut y = cursor.1 + 18;
    if y + HEIGHT > work.bottom {
        y = cursor.1 - HEIGHT - 16;
    }
    let x = x.clamp(work.left, (work.right - WIDTH).max(work.left));
    let y = y.clamp(work.top, (work.bottom - HEIGHT).max(work.top));
    unsafe {
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            x,
            y,
            WIDTH,
            HEIGHT,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub fn refresh(
    window: HWND,
    state: &Rc<RefCell<MagnifierState>>,
    frame: &Frame,
    cursor: (i32, i32),
) {
    {
        let mut value = state.borrow_mut();
        if value.frame.is_none() {
            value.frame = Some(frame.clone());
        }
        value.cursor = cursor;
    }
    common::invalidate(window);
}

impl WindowHandler for Magnifier {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        // Keep the picker capturable so remote desktops can show it.
        let _ = hwnd;
        Ok(())
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        _w: WPARAM,
        _l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_PAINT => {
                self.paint(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_SETCURSOR => {
                unsafe {
                    let _ = SetCursor(Some(
                        LoadCursorW(None, IDC_CROSS).map_err(|e| e.to_string())?,
                    ));
                }
                return Ok(Some(LRESULT(1)));
            }
            _ => {}
        }
        Ok(None)
    }
}
