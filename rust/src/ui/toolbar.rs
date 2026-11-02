use super::{
    common::{self, Canvas},
    runtime::{self, WindowHandler, WindowOptions},
    selector::buffer::PaintBuffer,
};
use cap_rust::{selection::CaptureMode, settings::Theme};
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::WM_MOUSELEAVE, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

#[path = "toolbar_icons.rs"]
mod icons;
#[path = "toolbar_native.rs"]
mod native;

/// One cell of the annotation bar: Feishu order first, cap-only actions last.
struct Cell {
    command: usize,
    icon: usize,
    hint: &'static str,
    tool: Option<Tool>,
}
const CELLS: [Cell; 20] = [
    Cell {
        command: 1,
        icon: 1,
        hint: "矩形",
        tool: Some(Tool::Rectangle),
    },
    Cell {
        command: 2,
        icon: 2,
        hint: "椭圆",
        tool: Some(Tool::Ellipse),
    },
    Cell {
        command: 4,
        icon: 4,
        hint: "直线",
        tool: Some(Tool::Line),
    },
    Cell {
        command: 3,
        icon: 3,
        hint: "箭头",
        tool: Some(Tool::Arrow),
    },
    Cell {
        command: 5,
        icon: 5,
        hint: "画笔",
        tool: Some(Tool::Pen),
    },
    Cell {
        command: 6,
        icon: 6,
        hint: "文字：Ctrl+Enter 完成",
        tool: Some(Tool::Text),
    },
    Cell {
        command: 7,
        icon: 7,
        hint: "序号",
        tool: Some(Tool::Number),
    },
    Cell {
        command: CMD_MARKER,
        icon: 19,
        hint: "标注：拖动高亮",
        tool: Some(Tool::Marker),
    },
    Cell {
        command: 8,
        icon: 8,
        hint: "马赛克：拖动框选",
        tool: Some(Tool::Mosaic),
    },
    Cell {
        command: 9,
        icon: 9,
        hint: "模糊：拖动框选",
        tool: Some(Tool::Blur),
    },
    Cell {
        command: CMD_CROP,
        icon: 24,
        hint: "裁剪：收缩到选区",
        tool: None,
    },
    Cell {
        command: CMD_OCR,
        icon: 20,
        hint: "提取文字：识别选区",
        tool: None,
    },
    Cell {
        command: CMD_AUTOSCROLL,
        icon: 21,
        hint: "自动滚动：开始滚动截图",
        tool: None,
    },
    Cell {
        command: 14,
        icon: 23,
        hint: "复制完成 Enter / Ctrl+C",
        tool: None,
    },
    Cell {
        command: 10,
        icon: 10,
        hint: "撤销 Ctrl+Z",
        tool: None,
    },
    Cell {
        command: 13,
        icon: 13,
        hint: "保存 PNG / JPEG · Ctrl+S",
        tool: None,
    },
    Cell {
        command: 15,
        icon: 15,
        hint: "取消 Esc",
        tool: None,
    },
    Cell {
        command: CMD_CONFIRM,
        icon: 14,
        hint: "完成 Enter",
        tool: None,
    },
    Cell {
        command: 12,
        icon: 12,
        hint: "贴图：置顶图片",
        tool: None,
    },
    Cell {
        command: 16,
        icon: 16,
        hint: "录制所选区域",
        tool: None,
    },
];
const TOOL_COUNT: usize = CELLS.len();
const TOOL_SEPARATORS_AFTER: [usize; 4] = [6, 9, 13, 17];
const MODE_LABELS: [&str; 4] = ["截图", "滚动截图", "录屏", "提取文字"];

// ---- Feishu parity metrics measured on the real client at 150% scaling ----
/// Mode bar: 613x69 physical, four 45px-tall tabs (91/145/91/145) with 20px gaps.
const MODE_WIDTH: i32 = 613;
const MODE_HEIGHT: i32 = 69;
const MODE_TAB_Y: i32 = 12;
const MODE_TAB_HEIGHT: i32 = 45;
const MODE_TABS: [(i32, i32); 4] = [(63, 91), (174, 145), (339, 91), (450, 145)];
/// Annotation bar: 60px tall, 24px icons on a 60px pitch, groups split by 17px separators.
const TOOL_HEIGHT: i32 = 60;
const TOOL_PADDING: i32 = 14;
const TOOL_GRIP: i32 = 34;
const TOOL_CELL: i32 = 60;
const TOOL_SEPARATOR: i32 = 17;

pub const TOOL_COMMAND: u32 = WM_APP + 81;
/// Command ids shared with the selector: marker tool, OCR, auto-scroll and confirm.
pub const CMD_MARKER: usize = 19;
pub const CMD_CROP: usize = 20;
pub const CMD_OCR: usize = 21;
pub const CMD_AUTOSCROLL: usize = 22;
pub const CMD_CONFIRM: usize = 23;
pub const COLOR_BASE: usize = 100;
pub const WIDTH_BASE: usize = 200;
/// Feishu's seven-colour swatches, stored BGRA like the rest of the UI layer.
pub const COLORS: [[u8; 4]; 7] = [
    [0x56, 0x5B, 0xF0, 0xFF], // #F05B56 红
    [0x46, 0xCB, 0xFB, 0xFF], // #FBCB46 黄
    [0x43, 0xBA, 0x51, 0xFF], // #51BA43 绿
    [0xFF, 0x88, 0x4C, 0xFF], // #4C88FF 蓝
    [0x29, 0x29, 0x29, 0xFF], // #292929 深灰
    [0x75, 0x75, 0x75, 0xFF], // #757575 灰
    [0xFF, 0xFF, 0xFF, 0xFF], // #FFFFFF 白
];
/// Feishu light-theme swatches: brighter ramp with a pure-black dark swatch.
pub const COLORS_LIGHT: [[u8; 4]; 7] = [
    [0x45, 0x4A, 0xF5, 0xFF], // #F54A45 红
    [0x0A, 0xC6, 0xFF, 0xFF], // #FFC60A 黄
    [0x4B, 0xBD, 0x35, 0xFF], // #35BD4B 绿
    [0xF4, 0x6D, 0x33, 0xFF], // #336DF4 蓝
    [0x00, 0x00, 0x00, 0xFF], // #000000 黑
    [0x9E, 0x95, 0x8F, 0xFF], // #8F959E 灰
    [0xFF, 0xFF, 0xFF, 0xFF], // #FFFFFF 白
];
/// Palette for the active theme.
pub fn colors(theme: Theme) -> &'static [[u8; 4]; 7] {
    match theme {
        Theme::Light => &COLORS_LIGHT,
        Theme::Dark => &COLORS,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Rectangle,
    Ellipse,
    Arrow,
    Line,
    Pen,
    Text,
    Number,
    Mosaic,
    Blur,
    Marker,
}
pub struct ToolbarState {
    pub tool: Tool,
    pub mode: CaptureMode,
    pub theme: Theme,
    pub color: [u8; 4],
    pub width: u32,
    pub can_undo: bool,
    pub can_redo: bool,
}
impl Default for ToolbarState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            mode: CaptureMode::Screenshot,
            theme: Theme::Dark,
            color: COLORS[0],
            width: 3,
            can_undo: false,
            can_redo: false,
        }
    }
}

/// Window size for the mode bar or the annotation bar.
pub fn bar_size(modes: bool) -> (i32, i32) {
    if modes {
        (MODE_WIDTH, MODE_HEIGHT)
    } else {
        (
            TOOL_PADDING * 2
                + TOOL_GRIP
                + TOOL_CELL * TOOL_COUNT as i32
                + TOOL_SEPARATOR * TOOL_SEPARATORS_AFTER.len() as i32,
            TOOL_HEIGHT,
        )
    }
}

/// Horizontal centre of a tool cell, used to align the contextual row's caret.
pub fn tool_center_x(tool: Tool) -> i32 {
    CELLS
        .iter()
        .position(|cell| cell.tool == Some(tool))
        .map(|index| tool_offset(index) + TOOL_CELL / 2)
        .unwrap_or(TOOL_CELL)
}

fn tool_offset(index: usize) -> i32 {
    let separators = TOOL_SEPARATORS_AFTER
        .iter()
        .filter(|&&after| after < index)
        .count() as i32;
    TOOL_PADDING + TOOL_GRIP + index as i32 * TOOL_CELL + separators * TOOL_SEPARATOR
}

/// Click/hover rectangle of one bar cell.
pub fn button_rect_at(modes: bool, index: usize) -> RECT {
    if modes {
        let (x, width) = MODE_TABS[index.min(MODE_TABS.len() - 1)];
        common::rect(x, MODE_TAB_Y, width, MODE_TAB_HEIGHT)
    } else {
        common::rect(tool_offset(index), 0, TOOL_CELL, TOOL_HEIGHT)
    }
}

pub fn show(target: HWND, state: Rc<RefCell<ToolbarState>>, modes: bool) -> Result<HWND, String> {
    let (width, height) = bar_size(modes);
    runtime::create(
        Box::new(Bar {
            target,
            state,
            modes,
            hover: None,
            renderer: None,
            chrome: None,
            back_buffer: None,
        }),
        WindowOptions {
            title: if modes {
                "cap · 功能选择"
            } else {
                "cap · 截图批注"
            }
            .into(),
            rect: common::rect(20, 20, width, height),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: Some(target),
            visible: false,
        },
    )
}
struct Bar {
    target: HWND,
    state: Rc<RefCell<ToolbarState>>,
    modes: bool,
    hover: Option<usize>,
    renderer: Option<icons::Renderer>,
    chrome: Option<native::Chrome>,
    back_buffer: Option<PaintBuffer>,
}
impl Bar {
    fn count(&self) -> usize {
        if self.modes {
            MODE_LABELS.len()
        } else {
            TOOL_COUNT
        }
    }
    fn button_rect(&self, index: usize) -> RECT {
        button_rect_at(self.modes, index)
    }
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.count()).find(|&index| {
            let rect = self.button_rect(index);
            x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
        })
    }
    fn grip(&self, dc: HDC) {
        // Feishu drag grip: six 3px dots, two columns by three rows.
        let dot = if self.state.borrow().theme == Theme::Light {
            common::rgb(0xB0, 0xB0, 0xB0)
        } else {
            common::rgb(0x6B, 0x6B, 0x6B)
        };
        let left = TOOL_PADDING + 6;
        let top = (self.window_height() - 26) / 2;
        for row in 0..3 {
            for column in 0..2 {
                common::fill(
                    dc,
                    common::rect(left + column * 7, top + row * 10, 3, 3),
                    dot,
                );
            }
        }
    }
    fn window_height(&self) -> i32 {
        if self.modes { MODE_HEIGHT } else { TOOL_HEIGHT }
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
        let c = self.back_buffer.as_ref().ok_or("工具栏绘图缓存未创建。")?;
        let chrome = self.chrome.as_ref().ok_or("工具栏字体未创建。")?;
        let state = self.state.borrow();
        if self.modes {
            common::rounded(c.dc, rect, common::rgb(0x0D, 0x0D, 0x0D), 18);
            self.grip(c.dc);
            let modes = [
                CaptureMode::Screenshot,
                CaptureMode::Scrolling,
                CaptureMode::Recording,
                CaptureMode::Text,
            ];
            for (index, label) in MODE_LABELS.iter().enumerate() {
                let button = self.button_rect(index);
                let selected = state.mode == modes[index];
                if selected || self.hover == Some(index) {
                    common::rounded(
                        c.dc,
                        button,
                        if selected {
                            common::rgb(0xFF, 0xFF, 0xFF)
                        } else {
                            common::rgb(0x1C, 0x1C, 0x1C)
                        },
                        12,
                    );
                }
                chrome.text_large(
                    c.dc,
                    label,
                    button,
                    if selected {
                        common::rgb(0x17, 0x18, 0x1B)
                    } else {
                        common::rgb(0xF2, 0xF2, 0xF2)
                    },
                );
            }
        } else {
            let light = state.theme == Theme::Light;
            let surface = if light {
                common::rgb(0xFF, 0xFF, 0xFF)
            } else {
                common::rgb(0x29, 0x29, 0x29)
            };
            let active_back = if light {
                common::rgb(0xD8, 0xDF, 0xEF)
            } else {
                common::rgb(0x3A, 0x43, 0x55)
            };
            let hover_back = if light {
                common::rgb(0xF0, 0xED, 0xF2)
            } else {
                common::rgb(0x35, 0x35, 0x37)
            };
            let separator = if light {
                common::rgb(0xCA, 0xCB, 0xCD)
            } else {
                common::rgb(0x3F, 0x3F, 0x3F)
            };
            let icon = if light {
                common::rgb(0x2B, 0x2F, 0x36)
            } else {
                common::rgb(0xE8, 0xE8, 0xE8)
            };
            let active_icon = if light {
                common::rgb(0x14, 0x56, 0xF0)
            } else {
                icon
            };
            let disabled_icon = if light {
                common::rgb(0x9A, 0xA0, 0xA6)
            } else {
                common::rgb(0x6B, 0x6B, 0x6B)
            };
            common::rounded(c.dc, rect, surface, 14);
            self.grip(c.dc);
            for (index, entry) in CELLS.iter().enumerate() {
                let cell = self.button_rect(index);
                let disabled = entry.command == 10 && !state.can_undo;
                let active = entry.tool == Some(state.tool);
                if active || self.hover == Some(index) && !disabled {
                    let pill = common::rect(
                        cell.left + (cell.right - cell.left - 42) / 2,
                        cell.top + (cell.bottom - cell.top - 42) / 2,
                        42,
                        42,
                    );
                    common::rounded(
                        c.dc,
                        pill,
                        if active { active_back } else { hover_back },
                        10,
                    );
                }
            }
            for after in TOOL_SEPARATORS_AFTER {
                let x = tool_offset(after + 1) - TOOL_SEPARATOR / 2;
                common::fill(
                    c.dc,
                    common::rect(x, (TOOL_HEIGHT - 28) / 2, 1, 28),
                    separator,
                );
            }
            let renderer = self.renderer.as_ref().ok_or("工具图标未初始化。")?;
            let graphics = renderer.frame(c.dc)?;
            for (index, entry) in CELLS.iter().enumerate() {
                let cell = self.button_rect(index);
                let icon_rect = common::rect(
                    cell.left + (cell.right - cell.left - 24) / 2,
                    cell.top + (cell.bottom - cell.top - 24) / 2,
                    24,
                    24,
                );
                let active = entry.tool == Some(state.tool);
                let color = if entry.command == 10 && !state.can_undo {
                    disabled_icon
                } else if entry.command == 15 {
                    common::rgb(0xF0, 0x5B, 0x56)
                } else if active {
                    active_icon
                } else {
                    icon
                };
                graphics.icon(
                    entry.icon,
                    icon_rect,
                    color,
                    common::rgb(state.color[2], state.color[1], state.color[0]),
                    state.width,
                )?;
            }
            graphics.finish()?;
        }
        c.present(canvas.dc)
    }
    fn click(&self, _hwnd: HWND, index: usize) -> Result<(), String> {
        let command = if self.modes {
            1000 + index
        } else {
            let cell = &CELLS[index];
            match cell.tool {
                // Clicking the active tool returns to plain selection, like Feishu.
                Some(tool) if self.state.borrow().tool == tool => 0,
                Some(_) => cell.command,
                None => cell.command,
            }
        };
        unsafe { PostMessageW(Some(self.target), TOOL_COMMAND, WPARAM(command), LPARAM(0)) }
            .map_err(|e| e.to_string())
    }
}

impl WindowHandler for Bar {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        self.renderer = Some(icons::Renderer::new()?);
        let entries: Vec<(RECT, &str)> = if self.modes {
            MODE_LABELS
                .iter()
                .enumerate()
                .map(|(index, label)| (button_rect_at(true, index), *label))
                .collect()
        } else {
            CELLS
                .iter()
                .enumerate()
                .map(|(index, cell)| (button_rect_at(false, index), cell.hint))
                .collect()
        };
        self.chrome = Some(native::Chrome::new(hwnd, &entries)?);
        native::round_window(hwnd, if self.modes { 18 } else { 14 })
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
                    self.click(hwnd, index)?;
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
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe { TrackMouseEvent(&mut tracking) }.map_err(|e| e.to_string())?;
                return Ok(Some(LRESULT(0)));
            }
            WM_MOUSELEAVE => {
                if self.hover.take().is_some() {
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
            WM_SIZE => {
                native::round_window(hwnd, if self.modes { 18 } else { 14 })?;
                return Ok(Some(LRESULT(0)));
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

#[cfg(test)]
#[path = "toolbar_tests.rs"]
mod tests;
