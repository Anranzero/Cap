//! Feishu recording starter: after selecting a region in recording mode the overlay
//! shows a card with 开始录制, the MP4/GIF format choice and the device row.
use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
    toolbar::TOOL_COMMAND,
};
use cap_rust::settings::Theme;
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

/// 320x196dp at 150% scaling.
pub const WIDTH: i32 = 480;
pub const HEIGHT: i32 = 294;
const BUTTON: (i32, i32, i32, i32) = (18, 18, 444, 51);
const DEVICES: (i32, i32, i32, i32) = (18, 180, 444, 96);

pub const CMD_RECORD_MP4: usize = 32;
pub const CMD_RECORD_GIF: usize = 33;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp4,
    Gif,
}

pub fn show(target: HWND, theme: Theme) -> Result<HWND, String> {
    runtime::create(
        Box::new(Card {
            target,
            theme,
            format: Format::Mp4,
            hover: None,
            back_buffer: None,
        }),
        WindowOptions {
            title: "cap · 开始录制".into(),
            rect: common::rect(20, 20, WIDTH, HEIGHT),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}

fn mp4_zone() -> RECT {
    common::rect(130, 78, 78, 34)
}
fn gif_zone() -> RECT {
    common::rect(212, 78, 70, 34)
}
fn button_zone() -> RECT {
    common::rect(BUTTON.0, BUTTON.1, BUTTON.2, BUTTON.3)
}

struct Card {
    target: HWND,
    theme: Theme,
    format: Format,
    hover: Option<usize>,
    back_buffer: Option<PaintBuffer>,
}

impl Card {
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        for (index, zone) in [button_zone(), mp4_zone(), gif_zone()].iter().enumerate() {
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
            .ok_or("录制卡片绘图缓存未创建。")?;
        let light = self.theme == Theme::Light;
        let panel = if light {
            common::rgb(0xFF, 0xFF, 0xFF)
        } else {
            common::rgb(0x37, 0x37, 0x37)
        };
        let devices_back = if light {
            common::rgb(0xF5, 0xF5, 0xF7)
        } else {
            common::rgb(0x29, 0x29, 0x29)
        };
        let label_colour = if light {
            common::rgb(0x1F, 0x23, 0x29)
        } else {
            common::rgb(0xFF, 0xFF, 0xFF)
        };
        let muted_colour = if light {
            common::rgb(0x64, 0x6A, 0x73)
        } else {
            common::rgb(0xC8, 0xC8, 0xC8)
        };
        common::rounded(c.dc, rect, panel, 15);
        // 开始录制 button
        let button = button_zone();
        common::rounded(
            c.dc,
            button,
            if self.hover == Some(0) {
                common::rgb(0x2C, 0x63, 0xD2)
            } else {
                common::rgb(0x33, 0x70, 0xEB)
            },
            9,
        );
        common::text(
            c.dc,
            "开始录制",
            button,
            common::rgb(0xFF, 0xFF, 0xFF),
            (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX).0,
        );
        // Format row
        common::label(
            c.dc,
            "选择格式",
            common::rect(24, 78, 100, 34),
            muted_colour,
        );
        for (index, (zone, label)) in [(mp4_zone(), "MP4"), (gif_zone(), "GIF")]
            .into_iter()
            .enumerate()
        {
            let selected = matches!((index, self.format), (0, Format::Mp4) | (1, Format::Gif));
            let center = (zone.left + 9, zone.top + (zone.bottom - zone.top) / 2);
            unsafe {
                let brush = CreateSolidBrush(COLORREF(if selected {
                    common::rgb(0x33, 0x70, 0xEB)
                } else {
                    common::rgb(0x37, 0x37, 0x37)
                }));
                let pen = CreatePen(
                    PS_SOLID,
                    2,
                    COLORREF(if selected {
                        common::rgb(0x33, 0x70, 0xEB)
                    } else {
                        common::rgb(0x8A, 0x8A, 0x8A)
                    }),
                );
                let previous_brush = SelectObject(c.dc, brush.into());
                let previous_pen = SelectObject(c.dc, pen.into());
                let _ = Ellipse(c.dc, center.0 - 7, center.1 - 7, center.0 + 7, center.1 + 7);
                SelectObject(c.dc, previous_pen);
                SelectObject(c.dc, previous_brush);
                let _ = DeleteObject(pen.into());
                let _ = DeleteObject(brush.into());
            }
            common::label(
                c.dc,
                label,
                common::rect(zone.left + 22, zone.top, 50, 34),
                label_colour,
            );
        }
        // Device row
        let devices = common::rect(DEVICES.0, DEVICES.1, DEVICES.2, DEVICES.3);
        common::rounded(c.dc, devices, devices_back, 9);
        let labels = ["扬声器", "麦克风", "摄像头", "鼠标设置"];
        let enabled = [false, false, false, true];
        for (index, (label, on)) in labels.iter().zip(enabled).enumerate() {
            let cell = common::rect(
                devices.left + index as i32 * (devices.right - devices.left) / 4,
                devices.top,
                (devices.right - devices.left) / 4,
                devices.bottom - devices.top,
            );
            let colour = if on {
                if light {
                    common::rgb(0x1F, 0x23, 0x29)
                } else {
                    common::rgb(0xE8, 0xE8, 0xE8)
                }
            } else if light {
                common::rgb(0x8F, 0x95, 0x9E)
            } else {
                common::rgb(0x7A, 0x7A, 0x7A)
            };
            draw_device_icon(c.dc, index, cell, colour);
            if !on {
                unsafe {
                    let pen = CreatePen(PS_SOLID, 2, COLORREF(common::rgb(0xC8, 0x3B, 0x3B)));
                    let previous = SelectObject(c.dc, pen.into());
                    let _ = MoveToEx(c.dc, cell.left + 18, cell.top + 34, None);
                    let _ = LineTo(c.dc, cell.left + 42, cell.top + 12);
                    SelectObject(c.dc, previous);
                    let _ = DeleteObject(pen.into());
                }
            }
            common::label(
                c.dc,
                label,
                common::rect(cell.left, cell.bottom - 30, cell.right - cell.left, 26),
                colour,
            );
        }
        c.present(canvas.dc)
    }
    fn click(&self, index: usize) -> Result<(), String> {
        match index {
            0 => {
                let id = match self.format {
                    Format::Mp4 => CMD_RECORD_MP4,
                    Format::Gif => CMD_RECORD_GIF,
                };
                unsafe { PostMessageW(Some(self.target), TOOL_COMMAND, WPARAM(id), LPARAM(0)) }
                    .map_err(|e| e.to_string())
            }
            1 | 2 => Ok(()),
            _ => Ok(()),
        }
    }
    fn set_format(&mut self, hwnd: HWND, format: Format) {
        if self.format != format {
            self.format = format;
            common::invalidate(hwnd);
        }
    }
}

fn draw_device_icon(dc: HDC, index: usize, cell: RECT, colour: u32) {
    let centre = (cell.left + (cell.right - cell.left) / 2, cell.top + 22);
    unsafe {
        let pen = CreatePen(PS_SOLID, 2, COLORREF(colour));
        let previous = SelectObject(dc, pen.into());
        let brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        match index {
            0 => {
                let _ = Rectangle(dc, centre.0 - 8, centre.1 - 6, centre.0 + 2, centre.1 + 6);
                let points = [
                    POINT {
                        x: centre.0 + 2,
                        y: centre.1 - 2,
                    },
                    POINT {
                        x: centre.0 + 10,
                        y: centre.1 - 8,
                    },
                    POINT {
                        x: centre.0 + 10,
                        y: centre.1 + 8,
                    },
                    POINT {
                        x: centre.0 + 2,
                        y: centre.1 + 2,
                    },
                ];
                let _ = Polyline(dc, &points);
            }
            1 => {
                let _ = RoundRect(
                    dc,
                    centre.0 - 5,
                    centre.1 - 10,
                    centre.0 + 5,
                    centre.1 + 6,
                    8,
                    8,
                );
                let _ = MoveToEx(dc, centre.0, centre.1 + 6, None);
                let _ = LineTo(dc, centre.0, centre.1 + 12);
                let _ = MoveToEx(dc, centre.0 - 7, centre.1 + 12, None);
                let _ = LineTo(dc, centre.0 + 7, centre.1 + 12);
            }
            2 => {
                let _ = Rectangle(dc, centre.0 - 10, centre.1 - 6, centre.0 + 4, centre.1 + 6);
                let points = [
                    POINT {
                        x: centre.0 + 4,
                        y: centre.1 - 2,
                    },
                    POINT {
                        x: centre.0 + 12,
                        y: centre.1 - 8,
                    },
                    POINT {
                        x: centre.0 + 12,
                        y: centre.1 + 8,
                    },
                    POINT {
                        x: centre.0 + 4,
                        y: centre.1 + 2,
                    },
                ];
                let _ = Polyline(dc, &points);
            }
            _ => {
                let points = [
                    POINT {
                        x: centre.0 - 4,
                        y: centre.1 - 10,
                    },
                    POINT {
                        x: centre.0 - 4,
                        y: centre.1 + 6,
                    },
                    POINT {
                        x: centre.0,
                        y: centre.1 + 2,
                    },
                    POINT {
                        x: centre.0 + 4,
                        y: centre.1 + 11,
                    },
                    POINT {
                        x: centre.0 + 8,
                        y: centre.1 + 9,
                    },
                    POINT {
                        x: centre.0 + 4,
                        y: centre.1,
                    },
                    POINT {
                        x: centre.0 + 9,
                        y: centre.1 - 2,
                    },
                ];
                let _ = Polyline(dc, &points);
            }
        }
        SelectObject(dc, brush);
        SelectObject(dc, previous);
        let _ = DeleteObject(pen.into());
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
                match self.hit(x, y) {
                    Some(1) => self.set_format(hwnd, Format::Mp4),
                    Some(2) => self.set_format(hwnd, Format::Gif),
                    Some(index) => self.click(index)?,
                    None => {}
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

impl Format {
    pub fn recording(self) -> cap_rust::recording::RecordingFormat {
        match self {
            Format::Mp4 => cap_rust::recording::RecordingFormat::Mp4,
            Format::Gif => cap_rust::recording::RecordingFormat::Gif,
        }
    }
}
