//! Feishu-style contextual row: per-tool parameters (stroke width, text size,
//! effect strength) plus the seven-colour palette, floating below the annotation bar.
use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
    toolbar::{self, TOOL_COMMAND, Tool},
};
use cap_rust::settings::Theme;
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, PWSTR},
};

/// Height of the contextual row (Feishu: 54 physical px) plus the 8px caret notch.
const HEIGHT: i32 = 62;
const PILL_TOP: i32 = 8;
const PILL_HEIGHT: i32 = 54;
const PADDING: i32 = 10;
const DOT_CELL: i32 = 34;
const SLIDER_CELL: i32 = 226;
const SEPARATOR: i32 = 17;
const SWATCH: i32 = 27;
const SWATCH_CELL: i32 = 34;
const WIDTHS: [u32; 3] = [2, 3, 6];
const WIDTH_LABELS: [&str; 3] = ["细", "中", "粗"];
/// Local x of the caret; the selector offsets this row so the caret lands under the active tool.
const CARET_X: i32 = 40;
const COLOUR_NAMES: [&str; 7] = ["红色", "黄色", "绿色", "蓝色", "深灰", "灰色", "白色"];
/// Feishu mosaic row: 马赛克 / 模糊 / 块状.
const EFFECT_KINDS: [(&str, &str); 3] = [("▦", "马赛克"), ("◍", "模糊"), ("▩", "块状")];
/// Feishu text row: a 12pt dropdown with five steps.
pub const SIZE_PRESETS: [(u32, &str); 5] = [
    (2, "12pt"),
    (4, "16pt"),
    (5, "20pt"),
    (6, "24pt"),
    (8, "32pt"),
];

/// Label of the closest preset for a thickness value.
pub fn size_label(width: u32) -> &'static str {
    SIZE_PRESETS
        .iter()
        .min_by_key(|(thickness, _)| thickness.abs_diff(width))
        .map(|(_, label)| *label)
        .unwrap_or("12pt")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    ColorDot,
    Width(usize),
    SizeMenu,
    Effect(usize),
    Slider,
    Swatch(usize),
}

fn zones(tool: Tool) -> Vec<Zone> {
    let mut out = Vec::new();
    match tool {
        // Feishu mosaic row: three effect kinds, then the strength slider.
        Tool::Mosaic => {
            for index in 0..EFFECT_KINDS.len() {
                out.push(Zone::Effect(index));
            }
            out.push(Zone::Slider);
        }
        Tool::Blur | Tool::Marker => out.push(Zone::Slider),
        Tool::Text | Tool::Number => out.push(Zone::SizeMenu),
        _ => {
            // Feishu leads the stroke row with the current colour.
            out.push(Zone::ColorDot);
            for index in 0..WIDTHS.len() {
                out.push(Zone::Width(index));
            }
        }
    }
    for index in 0..toolbar::COLORS.len() {
        out.push(Zone::Swatch(index));
    }
    out
}

fn zone_width(zone: Zone) -> i32 {
    match zone {
        Zone::ColorDot => 30,
        Zone::SizeMenu => 82,
        Zone::Effect(_) => 40,
        Zone::Width(_) => DOT_CELL,
        Zone::Slider => SLIDER_CELL,
        Zone::Swatch(_) => SWATCH_CELL,
    }
}

fn starts_palette(zone: Zone) -> bool {
    matches!(zone, Zone::Swatch(0))
}

/// Total pill width for a tool.
pub fn width(tool: Tool) -> i32 {
    let mut total = PADDING * 2;
    for (index, zone) in zones(tool).into_iter().enumerate() {
        if index > 0 && starts_palette(zone) {
            total += SEPARATOR;
        }
        total += zone_width(zone);
    }
    total
}
pub fn height() -> i32 {
    HEIGHT
}

fn zone_rects(tool: Tool) -> Vec<(Zone, RECT)> {
    let mut out = Vec::new();
    let mut x = PADDING;
    for (index, zone) in zones(tool).into_iter().enumerate() {
        if index > 0 && starts_palette(zone) {
            x += SEPARATOR;
        }
        out.push((
            zone,
            common::rect(x, PILL_TOP, zone_width(zone), PILL_HEIGHT),
        ));
        x += zone_width(zone);
    }
    out
}

pub fn show(target: HWND, state: Rc<RefCell<toolbar::ToolbarState>>) -> Result<HWND, String> {
    runtime::create(
        Box::new(Options {
            target,
            state,
            hover: None,
            chrome: None,
            back_buffer: None,
        }),
        WindowOptions {
            title: "cap · 参数".into(),
            rect: common::rect(20, 20, width(Tool::Rectangle), HEIGHT),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}

struct Options {
    target: HWND,
    state: Rc<RefCell<toolbar::ToolbarState>>,
    hover: Option<usize>,
    chrome: Option<Chrome>,
    back_buffer: Option<PaintBuffer>,
}

/// Tooltip host for the contextual row.
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
            return Err("初始化参数条提示控件失败。".into());
        }
        chrome.tooltip = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
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
        .map_err(|e| format!("创建参数条提示失败：{e}"))?;
        for (index, (rect, _)) in entries.iter().enumerate() {
            let tool = TTTOOLINFOW {
                cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
                uFlags: TTF_SUBCLASS | TTF_TRANSPARENT,
                hwnd: owner,
                uId: index + 1,
                rect: *rect,
                lpszText: PWSTR(chrome.texts[index].as_mut_ptr()),
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

impl Options {
    fn hit(&self, x: i32, y: i32) -> Option<(usize, Zone, RECT)> {
        if !(PILL_TOP..PILL_TOP + PILL_HEIGHT).contains(&y) || x < 0 {
            return None;
        }
        zone_rects(self.state.borrow().tool)
            .into_iter()
            .enumerate()
            .find(|(_, (_, rect))| x >= rect.left && x < rect.right)
            .map(|(index, (zone, rect))| (index, zone, rect))
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
        let c = self.back_buffer.as_ref().ok_or("参数条绘图缓存未创建。")?;
        let state = self.state.borrow();
        let light = state.theme == Theme::Light;
        let surface = if light {
            common::rgb(0xFF, 0xFF, 0xFF)
        } else {
            common::rgb(0x29, 0x29, 0x29)
        };
        let text_colour = if light {
            common::rgb(0x1F, 0x23, 0x29)
        } else {
            common::rgb(0xE8, 0xE8, 0xE8)
        };
        let hover_back = if light {
            common::rgb(0xF0, 0xED, 0xF2)
        } else {
            common::rgb(0x35, 0x35, 0x37)
        };
        let muted_colour = if light {
            common::rgb(0x64, 0x6A, 0x73)
        } else {
            common::rgb(0x8A, 0x8A, 0x8A)
        };
        let accent = if light {
            common::rgb(0x33, 0x6D, 0xF4)
        } else {
            common::rgb(0x4C, 0x88, 0xFF)
        };
        common::rounded(
            c.dc,
            common::rect(0, PILL_TOP, rect.right, PILL_HEIGHT),
            surface,
            14,
        );
        // Caret notch pointing at the active tool.
        let caret_x = CARET_X.min(rect.right - 16).max(16);
        unsafe {
            let brush = CreateSolidBrush(COLORREF(surface));
            let previous = SelectObject(c.dc, brush.into());
            let pen = SelectObject(c.dc, GetStockObject(NULL_PEN));
            let points = [
                POINT {
                    x: caret_x - 7,
                    y: PILL_TOP,
                },
                POINT {
                    x: caret_x + 7,
                    y: PILL_TOP,
                },
                POINT { x: caret_x, y: 0 },
            ];
            let _ = Polygon(c.dc, &points);
            SelectObject(c.dc, pen);
            SelectObject(c.dc, previous);
            let _ = DeleteObject(brush.into());
        }
        let palette = toolbar::colors(state.theme);
        for (index, (zone, area)) in zone_rects(state.tool).into_iter().enumerate() {
            let hovered = self.hover == Some(index);
            match zone {
                Zone::ColorDot => {
                    let color = state.color;
                    let centre = (
                        area.left + (area.right - area.left) / 2,
                        area.top + (area.bottom - area.top) / 2,
                    );
                    unsafe {
                        let brush =
                            CreateSolidBrush(COLORREF(common::rgb(color[2], color[1], color[0])));
                        let previous = SelectObject(c.dc, brush.into());
                        // Hollow ring: a darker inner circle keeps the dot readable on light bars.
                        let pen = CreatePen(
                            PS_SOLID,
                            2,
                            COLORREF(common::rgb(color[2], color[1], color[0])),
                        );
                        let old_pen = SelectObject(c.dc, pen.into());
                        let _ =
                            Ellipse(c.dc, centre.0 - 9, centre.1 - 9, centre.0 + 9, centre.1 + 9);
                        SelectObject(c.dc, old_pen);
                        SelectObject(c.dc, previous);
                        let _ = DeleteObject(pen.into());
                        let _ = DeleteObject(brush.into());
                    }
                }
                Zone::Width(dot) => {
                    let diameter = [8, 12, 16][dot];
                    let center = (
                        area.left + (area.right - area.left) / 2,
                        area.top + (area.bottom - area.top) / 2,
                    );
                    let colour = if state.width == WIDTHS[dot] {
                        accent
                    } else if hovered {
                        common::rgb(0xB8, 0xB8, 0xB8)
                    } else {
                        muted_colour
                    };
                    unsafe {
                        let brush = CreateSolidBrush(COLORREF(colour));
                        let previous = SelectObject(c.dc, brush.into());
                        let pen = SelectObject(c.dc, GetStockObject(NULL_PEN));
                        let _ = Ellipse(
                            c.dc,
                            center.0 - diameter / 2,
                            center.1 - diameter / 2,
                            center.0 + diameter / 2,
                            center.1 + diameter / 2,
                        );
                        SelectObject(c.dc, pen);
                        SelectObject(c.dc, previous);
                        let _ = DeleteObject(brush.into());
                    }
                }
                Zone::Effect(kind) => {
                    let selected = state.effect == effect_kind(kind);
                    if selected || hovered {
                        common::rounded(
                            c.dc,
                            common::rect(
                                area.left + 3,
                                area.top + 10,
                                area.right - area.left - 6,
                                34,
                            ),
                            if selected { accent } else { hover_back },
                            8,
                        );
                    }
                    common::text(
                        c.dc,
                        EFFECT_KINDS[kind].0,
                        area,
                        if selected {
                            common::rgb(0xFF, 0xFF, 0xFF)
                        } else {
                            text_colour
                        },
                        (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX).0,
                    );
                }
                Zone::SizeMenu => {
                    let selected_or_hover = hovered;
                    if selected_or_hover {
                        common::rounded(
                            c.dc,
                            common::rect(
                                area.left + 4,
                                area.top + 10,
                                area.right - area.left - 8,
                                34,
                            ),
                            hover_back,
                            8,
                        );
                    }
                    common::text(
                        c.dc,
                        &format!("{} ▾", size_label(state.width)),
                        area,
                        text_colour,
                        (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX).0,
                    );
                }
                Zone::Slider => {
                    let value = state.width.clamp(1, 12) as i32;
                    common::label(
                        c.dc,
                        if state.tool == Tool::Marker {
                            "不透明度"
                        } else {
                            "模糊强度"
                        },
                        common::rect(area.left + 8, area.top, 64, PILL_HEIGHT),
                        muted_colour,
                    );
                    let track = common::rect(area.left + 78, area.top + 26, 108, 3);
                    common::fill(c.dc, track, common::rgb(0x55, 0x55, 0x55));
                    let knob_x = track.left + (track.right - track.left) * (value - 1) / 11;
                    unsafe {
                        let brush = CreateSolidBrush(COLORREF(accent));
                        let previous = SelectObject(c.dc, brush.into());
                        let pen = SelectObject(c.dc, GetStockObject(NULL_PEN));
                        let _ = Ellipse(c.dc, knob_x - 7, track.top - 5, knob_x + 7, track.top + 9);
                        SelectObject(c.dc, pen);
                        SelectObject(c.dc, previous);
                        let _ = DeleteObject(brush.into());
                    }
                    common::label(
                        c.dc,
                        &format!("{}%", value * 8),
                        common::rect(area.left + 192, area.top, 44, PILL_HEIGHT),
                        text_colour,
                    );
                }
                Zone::Swatch(swatch) => {
                    let color = palette[swatch];
                    let tile = common::rect(
                        area.left + (area.right - area.left - SWATCH) / 2,
                        area.top + (area.bottom - area.top - SWATCH) / 2,
                        SWATCH,
                        SWATCH,
                    );
                    let selected = state.color == color;
                    common::rounded(c.dc, tile, common::rgb(color[2], color[1], color[0]), 6);
                    let outline_colour = if light {
                        common::rgb(0xE4, 0xE5, 0xE6)
                    } else {
                        common::rgb(0x4A, 0x4A, 0x4A)
                    };
                    let outline = unsafe { CreatePen(PS_SOLID, 1, COLORREF(outline_colour)) };
                    let previous = unsafe { SelectObject(c.dc, outline.into()) };
                    unsafe {
                        let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
                        let _ = RoundRect(c.dc, tile.left, tile.top, tile.right, tile.bottom, 6, 6);
                        SelectObject(c.dc, brush);
                        SelectObject(c.dc, previous);
                        let _ = DeleteObject(outline.into());
                    }
                    if selected {
                        // Feishu marks the selected swatch with a check mark as well as a ring.
                        unsafe {
                            let pen =
                                CreatePen(PS_SOLID, 2, COLORREF(common::rgb(0xFF, 0xFF, 0xFF)));
                            let previous = SelectObject(c.dc, pen.into());
                            let _ = MoveToEx(c.dc, tile.left + 6, tile.top + 14, None);
                            let _ = LineTo(c.dc, tile.left + 11, tile.top + 19);
                            let _ = LineTo(c.dc, tile.right - 5, tile.top + 8);
                            SelectObject(c.dc, previous);
                            let _ = DeleteObject(pen.into());
                        }
                        let ring = unsafe {
                            CreatePen(PS_SOLID, 2, COLORREF(common::rgb(0xFF, 0xFF, 0xFF)))
                        };
                        let previous = unsafe { SelectObject(c.dc, ring.into()) };
                        unsafe {
                            let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
                            let _ = RoundRect(
                                c.dc,
                                tile.left - 2,
                                tile.top - 2,
                                tile.right + 2,
                                tile.bottom + 2,
                                8,
                                8,
                            );
                            SelectObject(c.dc, brush);
                            SelectObject(c.dc, previous);
                            let _ = DeleteObject(ring.into());
                        }
                    }
                }
            }
        }
        c.present(canvas.dc)
    }
    fn click(&self, zone: Zone, area: RECT, x: i32) -> Result<(), String> {
        let id = match zone {
            // The colour dot is a read-only indicator; clicking it does nothing.
            Zone::ColorDot => return Ok(()),
            Zone::Effect(kind) => toolbar::EFFECT_BASE + kind,
            Zone::SizeMenu => toolbar::CMD_SIZE_MENU,
            Zone::Width(dot) => toolbar::WIDTH_BASE + WIDTHS[dot] as usize,
            Zone::Slider => {
                let track_left = area.left + 78;
                let value = ((x - track_left).clamp(0, 108) as u32 * 11 + 54) / 108 + 1;
                toolbar::WIDTH_BASE + value.clamp(1, 12) as usize
            }
            Zone::Swatch(swatch) => toolbar::COLOR_BASE + swatch,
        };
        unsafe { PostMessageW(Some(self.target), TOOL_COMMAND, WPARAM(id), LPARAM(0)) }
            .map_err(|e| e.to_string())
    }
}

impl WindowHandler for Options {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        let tool = self.state.borrow().tool;
        let entries: Vec<(RECT, &str)> = zone_rects(tool)
            .into_iter()
            .map(|(zone, rect)| {
                let label = match zone {
                    Zone::ColorDot => "当前颜色",
                    Zone::Effect(kind) => EFFECT_KINDS[kind].1,
                    Zone::SizeMenu => "字号",
                    Zone::Width(dot) => WIDTH_LABELS[dot],
                    Zone::Slider => "效果强度",
                    Zone::Swatch(swatch) => COLOUR_NAMES[swatch],
                };
                (rect, label)
            })
            .collect();
        self.chrome = Some(Chrome::new(hwnd, &entries)?);
        Ok(())
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_LBUTTONUP => {
                let (x, y) = common::point(l);
                if let Some((_, zone, area)) = self.hit(x, y) {
                    self.click(zone, area, x)?;
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_MOUSEMOVE => {
                let (x, y) = common::point(l);
                let hover = self.hit(x, y).map(|(index, _, _)| index);
                if self.hover != hover {
                    self.hover = hover;
                    common::invalidate(hwnd);
                }
                // Dragging the knob updates the strength live.
                if w.0 & 1 != 0 {
                    if let Some((_, Zone::Slider, area)) = self.hit(x, y) {
                        self.click(Zone::Slider, area, x)?;
                    }
                }
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe { TrackMouseEvent(&mut tracking) }.map_err(|e| e.to_string())?;
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

fn effect_kind(kind: usize) -> cap_rust::annotation::Effect {
    match kind {
        0 => cap_rust::annotation::Effect::Pixelate,
        1 => cap_rust::annotation::Effect::Blur,
        _ => cap_rust::annotation::Effect::Block,
    }
}

/// Which tool gets a contextual row in Feishu.
pub fn visible_for(tool: Tool) -> bool {
    !matches!(tool, Tool::Select)
}
