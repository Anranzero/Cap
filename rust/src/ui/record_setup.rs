//! Native recording options, kept separate from recording session ownership.
use super::{
    UiEvent, common,
    runtime::{self, WindowHandler, WindowOptions},
};
use cap_rust::{
    capture,
    geometry::Rect,
    overlays::OverlayOptions,
    recording::{RecordingConfig, RecordingFormat},
    settings::Settings,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::{DRAWITEMSTRUCT, Dialogs::*, SetScrollInfo},
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::*,
    },
};

const MP4: u16 = 300;
const GIF: u16 = 301;
const CURSOR: u16 = 302;
const COORDINATES: u16 = 303;
const ADVANCED: u16 = 304;
const TARGET: u16 = 305;
const MONITOR: u16 = 306;
const WINDOW: u16 = 307;
const FPS: u16 = 308;
const WIDTH: u16 = 309;
const CLICKS: u16 = 312;
const COORDINATE_MODE: u16 = 313;
const FONT_SIZE: u16 = 314;
const COLOR: u16 = 315;
const PICK_COLOR: u16 = 316;
const REFRESH: u16 = 317;
const START: u16 = 320;
const CANCEL: u16 = 321;
const ACCEPT_DIALOG: u16 = IDOK.0 as u16;
const CANCEL_DIALOG: u16 = IDCANCEL.0 as u16;
const SETUP_WIDTH: i32 = 450;

pub fn show_setup(settings: Rc<RefCell<Settings>>) -> Result<HWND, String> {
    show_setup_format(settings, RecordingFormat::Mp4)
}

pub fn show_setup_format(
    settings: Rc<RefCell<Settings>>,
    format: RecordingFormat,
) -> Result<HWND, String> {
    let draft = settings.borrow().clone();
    let monitors = capture::monitors()?;
    let windows = available_windows()?;
    let brush = unsafe { CreateSolidBrush(COLORREF(common::palette(draft.theme).panel)) };
    if brush.is_invalid() {
        return Err("创建录制设置画刷失败。".into());
    }
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        unsafe {
            let _ = DeleteObject(brush.into());
        }
        return Err("读取录制设置窗口工作区失败。".into());
    }
    let height = 350;
    runtime::create(
        Box::new(Setup {
            settings,
            draft,
            format,
            expanded: false,
            controls: HashMap::new(),
            monitors,
            windows,
            brush,
            work: info.rcWork,
            positions: HashMap::new(),
            scroll: 0,
        }),
        WindowOptions {
            title: "cap · 录制".into(),
            rect: common::rect(
                info.rcWork.left + (info.rcWork.right - info.rcWork.left - SETUP_WIDTH) / 2,
                info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - height) / 2,
                SETUP_WIDTH,
                height,
            ),
            style: WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VSCROLL,
            ex_style: WS_EX_DLGMODALFRAME,
            owner: None,
            visible: true,
        },
    )
}

fn available_windows() -> Result<Vec<capture::DesktopWindow>, String> {
    Ok(capture::windows()?
        .into_iter()
        .filter(|window| !window.title.starts_with("cap ·"))
        .collect())
}

struct Setup {
    settings: Rc<RefCell<Settings>>,
    draft: Settings,
    format: RecordingFormat,
    expanded: bool,
    controls: HashMap<u16, HWND>,
    monitors: Vec<capture::Monitor>,
    windows: Vec<capture::DesktopWindow>,
    brush: HBRUSH,
    work: RECT,
    positions: HashMap<u16, RECT>,
    scroll: i32,
}
impl Drop for Setup {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.brush.into());
        }
    }
}
impl Setup {
    fn add(
        &mut self,
        parent: HWND,
        id: u16,
        class: &str,
        text: &str,
        rect: RECT,
        style: WINDOW_STYLE,
    ) -> Result<HWND, String> {
        let hwnd = common::child(parent, class, text, id, rect, style)?;
        self.controls.insert(id, hwnd);
        self.positions.insert(id, rect);
        Ok(hwnd)
    }
    fn control(&self, id: u16) -> HWND {
        self.controls[&id]
    }
    fn label(&mut self, parent: HWND, id: u16, text: &str, rect: RECT) -> Result<(), String> {
        self.add(parent, id, "STATIC", text, rect, WINDOW_STYLE(0))?;
        Ok(())
    }
    fn choice(
        &mut self,
        parent: HWND,
        id: u16,
        rect: RECT,
        values: &[String],
    ) -> Result<(), String> {
        let hwnd = self.add(
            parent,
            id,
            "COMBOBOX",
            "",
            rect,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        )?;
        for value in values {
            add_choice(hwnd, value);
        }
        select(hwnd, 0);
        Ok(())
    }
    fn check(
        &mut self,
        parent: HWND,
        id: u16,
        text: &str,
        y: i32,
        checked: bool,
    ) -> Result<(), String> {
        let hwnd = self.add(
            parent,
            id,
            "BUTTON",
            text,
            common::rect(24, y, 395, 29),
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        )?;
        unsafe {
            SendMessageW(hwnd, BM_SETCHECK, Some(WPARAM(usize::from(checked))), None);
        }
        Ok(())
    }
    fn checked(&self, id: u16) -> bool {
        unsafe { SendMessageW(self.control(id), BM_GETCHECK, None, None).0 == 1 }
    }
    fn fps_values(&self) -> Vec<String> {
        let mut values: Vec<String> = if self.format == RecordingFormat::Mp4 {
            vec![15, 24, 30, 60]
        } else {
            vec![5, 10, 15, 24, 30, 60]
        }
        .into_iter()
        .map(|value| value.to_string())
        .collect();
        let selected = if self.format == RecordingFormat::Mp4 {
            self.draft.frames_per_second
        } else {
            self.draft.gif_frames_per_second
        }
        .to_string();
        if !values.contains(&selected) {
            values.push(selected);
        }
        values
    }
    fn change_format(&mut self, hwnd: HWND, format: RecordingFormat) {
        self.format = format;
        let fps = self.control(FPS);
        unsafe {
            SendMessageW(fps, CB_RESETCONTENT, None, None);
        }
        let values = self.fps_values();
        for value in &values {
            add_choice(fps, value);
        }
        let default_fps = if format == RecordingFormat::Mp4 {
            self.draft.frames_per_second
        } else {
            self.draft.gif_frames_per_second
        }
        .to_string();
        select(
            fps,
            values
                .iter()
                .position(|value| *value == default_fps)
                .unwrap_or(0),
        );
        common::set_text(
            self.control(WIDTH),
            &if format == RecordingFormat::Gif {
                self.draft.gif_max_width.to_string()
            } else {
                "0".into()
            },
        );
        common::invalidate(self.control(MP4));
        common::invalidate(self.control(GIF));
        common::invalidate(hwnd);
    }
    fn update_target(&self) {
        let target = selected(self.control(TARGET));
        for (id, visible) in [
            (MONITOR, target == 1),
            (WINDOW, target == 2),
            (REFRESH, target == 2),
        ] {
            unsafe {
                let _ = ShowWindow(
                    self.control(id),
                    if self.expanded && visible {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
    }
    fn update_coordinates(&self) {
        let enabled = self.checked(COORDINATES);
        for id in [COORDINATE_MODE, FONT_SIZE, COLOR, PICK_COLOR] {
            unsafe {
                let _ = EnableWindow(self.control(id), enabled);
            }
        }
    }
    fn resize(&mut self, hwnd: HWND) -> Result<(), String> {
        let height = if self.expanded { 640 } else { 350 };
        let mut current = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut current) }
            .map_err(|e| format!("读取录制设置位置失败：{e}"))?;
        let height = height.min(self.work.bottom - self.work.top);
        let y = current.top.clamp(
            self.work.top,
            (self.work.bottom - height).max(self.work.top),
        );
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                current.left,
                y,
                SETUP_WIDTH,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .map_err(|e| format!("调整录制设置窗口失败：{e}"))?;
        for id in [
            TARGET,
            MONITOR,
            WINDOW,
            FPS,
            WIDTH,
            CLICKS,
            COORDINATE_MODE,
            FONT_SIZE,
            COLOR,
            PICK_COLOR,
            REFRESH,
            405,
            408,
            409,
            413,
            414,
            415,
            416,
        ] {
            unsafe {
                let _ = ShowWindow(
                    self.control(id),
                    if self.expanded { SW_SHOW } else { SW_HIDE },
                );
            }
        }
        self.update_target();
        common::set_text(
            self.control(ADVANCED),
            if self.expanded {
                "收起高级设置 ▴"
            } else {
                "高级设置 ▾"
            },
        );
        self.layout_footer(hwnd)?;
        self.layout_body(hwnd)?;
        common::invalidate(hwnd);
        Ok(())
    }
    fn layout_body(&mut self, hwnd: HWND) -> Result<(), String> {
        let visible_height = (common::client(hwnd).bottom - 90).max(1);
        let content_height = if self.expanded { 505 } else { 200 };
        self.scroll = self
            .scroll
            .clamp(0, (content_height - visible_height).max(0));
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: content_height - 1,
            nPage: visible_height as u32,
            nPos: self.scroll,
            ..Default::default()
        };
        unsafe {
            SetScrollInfo(hwnd, SB_VERT, &info, true);
        }
        let target = selected(self.control(TARGET));
        for (&id, original) in &self.positions {
            if matches!(id, START | CANCEL | 420) {
                continue;
            }
            let advanced = !matches!(id, MP4 | GIF | CURSOR | COORDINATES | ADVANCED);
            let target_visible = match id {
                MONITOR => target == 1,
                WINDOW | REFRESH => target == 2,
                _ => true,
            };
            let height = if matches!(id, TARGET | MONITOR | WINDOW | FPS | COORDINATE_MODE) {
                28
            } else {
                original.bottom - original.top
            };
            let top = original.top - self.scroll;
            let visible = (!advanced || self.expanded)
                && target_visible
                && top >= 0
                && top + height <= visible_height;
            unsafe {
                SetWindowPos(
                    self.control(id),
                    None,
                    original.left,
                    top,
                    original.right - original.left,
                    original.bottom - original.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .map_err(|e| format!("滚动录制设置失败：{e}"))?;
                let _ = ShowWindow(self.control(id), if visible { SW_SHOW } else { SW_HIDE });
            }
        }
        common::invalidate(hwnd);
        Ok(())
    }
    fn layout_footer(&self, hwnd: HWND) -> Result<(), String> {
        let client = common::client(hwnd);
        let top = client.bottom - 90;
        for (id, x, width) in [(START, 24, 290), (CANCEL, 325, 85)] {
            unsafe {
                SetWindowPos(
                    self.control(id),
                    None,
                    x,
                    top + 12,
                    width,
                    38,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .map_err(|e| format!("定位录制设置按钮失败：{e}"))?;
        }
        unsafe {
            SetWindowPos(
                self.control(420),
                None,
                24,
                top + 58,
                385,
                22,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .map_err(|e| format!("定位保存目录失败：{e}"))
    }
    fn refresh_windows(&mut self) -> Result<(), String> {
        self.windows = available_windows()?;
        let hwnd = self.control(WINDOW);
        unsafe {
            SendMessageW(hwnd, CB_RESETCONTENT, None, None);
        }
        if self.windows.is_empty() {
            add_choice(hwnd, "没有可录制窗口");
        }
        for window in &self.windows {
            add_choice(hwnd, &window.title);
        }
        select(hwnd, 0);
        Ok(())
    }
    fn config(&self) -> Result<(bool, RecordingConfig), String> {
        let width = parse_number(&common::get_text(self.control(WIDTH)), "最大宽度")?;
        if width != 0 && !(64..=7680).contains(&width) {
            return Err("宽度请选择 0 或 64–7680。".into());
        }
        let size = parse_number(&common::get_text(self.control(FONT_SIZE)), "坐标字号")?;
        if !(10..=72).contains(&size) {
            return Err("坐标字号范围为 10–72。".into());
        }
        let target = selected(self.control(TARGET));
        let (bounds, handle) = match target {
            0 => (capture::virtual_bounds()?, 0),
            1 => {
                let index = selected(self.control(MONITOR));
                if index == self.monitors.len() {
                    (capture::virtual_bounds()?, 0)
                } else {
                    (self.monitors.get(index).ok_or("请选择显示器。")?.bounds, 0)
                }
            }
            2 => {
                let window = self
                    .windows
                    .get(selected(self.control(WINDOW)))
                    .ok_or("没有可录制窗口，请刷新后选择。")?;
                (
                    capture::window_bounds(window.handle)
                        .map_err(|e| format!("选中的窗口已关闭或不可用，请刷新后重新选择：{e}"))?,
                    window.handle,
                )
            }
            _ => return Err("请选择录制范围。".into()),
        };
        let path = unique_output(&self.settings.borrow().save_directory, self.format)?;
        let mut config = RecordingConfig::new(bounds, path, self.format);
        config.window_handle = handle;
        config.fps = parse_number(&common::get_text(self.control(FPS)), "帧率")?;
        config.output_width = width;
        config.overlays = OverlayOptions {
            show_cursor: self.checked(CURSOR),
            highlight_clicks: self.checked(CLICKS),
            show_coordinates: self.checked(COORDINATES),
            relative_coordinates: selected(self.control(COORDINATE_MODE)) == 1,
            coordinate_font_size: size,
            coordinate_color: common::get_text(self.control(COLOR)).trim().to_string(),
        };
        if target == 0 {
            // The selector supplies final dimensions; validating against an entire large
            // virtual desktop here would reject a small valid region prematurely.
            let mut validation = config.clone();
            validation.bounds = Rect::new(0, 0, 64, 64)?;
            validation.validate()?;
        } else {
            config.validate()?;
        }
        Ok((target == 0, config))
    }
    fn pick_color(&self, hwnd: HWND) -> Result<(), String> {
        let text = common::get_text(self.control(COLOR));
        let rgb = text
            .strip_prefix('#')
            .and_then(|value| u32::from_str_radix(value, 16).ok())
            .ok_or("颜色请填写 #RRGGBB。")?;
        let mut custom = [COLORREF(0); 16];
        let mut picker = CHOOSECOLORW {
            lStructSize: size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: hwnd,
            rgbResult: COLORREF(common::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_FULLOPEN | CC_RGBINIT,
            ..Default::default()
        };
        if unsafe { ChooseColorW(&mut picker) }.as_bool() {
            let color = picker.rgbResult.0;
            common::set_text(
                self.control(COLOR),
                &format!(
                    "#{:02X}{:02X}{:02X}",
                    color & 255,
                    color >> 8 & 255,
                    color >> 16 & 255
                ),
            );
        } else {
            let code = unsafe { CommDlgExtendedError() };
            if code.0 != 0 {
                return Err(format!("打开颜色选择器失败：{}", code.0));
            }
        }
        Ok(())
    }
}

impl WindowHandler for Setup {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        capture::exclude_window(hwnd.0 as isize, true)?;
        for (id, text, x) in [(MP4, "视频 MP4", 24), (GIF, "动图 GIF", 224)] {
            self.add(
                hwnd,
                id,
                "BUTTON",
                text,
                common::rect(x, 18, 186, 38),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            )?;
        }
        self.check(hwnd, CURSOR, "录制鼠标指针", 76, self.draft.show_cursor)?;
        self.check(
            hwnd,
            COORDINATES,
            "显示鼠标 X、Y 坐标",
            111,
            self.draft.show_coordinates,
        )?;
        self.add(
            hwnd,
            ADVANCED,
            "BUTTON",
            "高级设置 ▾",
            common::rect(24, 156, 386, 32),
            WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        )?;
        self.label(hwnd, 405, "录制范围", common::rect(24, 199, 90, 24))?;
        self.choice(
            hwnd,
            TARGET,
            common::rect(119, 196, 291, 180),
            &[
                "已选区域".into(),
                "显示器 / 全桌面".into(),
                "指定窗口".into(),
            ],
        )?;
        let monitors = self
            .monitors
            .iter()
            .enumerate()
            .map(|(index, m)| {
                format!(
                    "显示器 {} · {} × {} ({}, {})",
                    index + 1,
                    m.bounds.width,
                    m.bounds.height,
                    m.bounds.x,
                    m.bounds.y
                )
            })
            .chain(Some("全部显示器".into()))
            .collect::<Vec<_>>();
        self.choice(hwnd, MONITOR, common::rect(24, 235, 386, 220), &monitors)?;
        let windows = if self.windows.is_empty() {
            vec!["没有可录制窗口".into()]
        } else {
            self.windows
                .iter()
                .map(|window| window.title.clone())
                .collect()
        };
        self.choice(hwnd, WINDOW, common::rect(24, 235, 308, 260), &windows)?;
        self.add(
            hwnd,
            REFRESH,
            "BUTTON",
            "刷新",
            common::rect(340, 234, 70, 28),
            WS_TABSTOP,
        )?;
        self.label(hwnd, 408, "帧率 · FPS", common::rect(24, 277, 180, 22))?;
        self.label(hwnd, 409, "最大宽度 · px", common::rect(224, 277, 186, 22))?;
        self.choice(
            hwnd,
            FPS,
            common::rect(24, 302, 180, 180),
            &self.fps_values(),
        )?;
        self.add(
            hwnd,
            WIDTH,
            "EDIT",
            "0",
            common::rect(224, 301, 186, 28),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        )?;
        self.label(
            hwnd,
            416,
            "宽度填 0 保留原始尺寸。",
            common::rect(24, 335, 386, 22),
        )?;
        self.check(hwnd, CLICKS, "点击高亮", 363, self.draft.highlight_clicks)?;
        self.label(hwnd, 413, "坐标方式", common::rect(24, 403, 90, 24))?;
        self.choice(
            hwnd,
            COORDINATE_MODE,
            common::rect(119, 400, 291, 150),
            &["屏幕绝对坐标".into(), "录制区域相对坐标".into()],
        )?;
        select(
            self.control(COORDINATE_MODE),
            usize::from(self.draft.relative_coordinates),
        );
        self.label(hwnd, 414, "字号 · 10–72", common::rect(24, 441, 150, 22))?;
        self.label(hwnd, 415, "颜色 · #RRGGBB", common::rect(224, 441, 186, 22))?;
        self.add(
            hwnd,
            FONT_SIZE,
            "EDIT",
            &self.draft.coordinate_font_size.to_string(),
            common::rect(24, 466, 180, 28),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        )?;
        self.add(
            hwnd,
            COLOR,
            "EDIT",
            &self.draft.coordinate_color.clone(),
            common::rect(224, 466, 125, 28),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        )?;
        self.add(
            hwnd,
            PICK_COLOR,
            "BUTTON",
            "选择",
            common::rect(357, 466, 53, 28),
            WS_TABSTOP,
        )?;
        self.add(
            hwnd,
            START,
            "BUTTON",
            "开始录制",
            common::rect(24, 520, 290, 38),
            WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        )?;
        self.add(
            hwnd,
            CANCEL,
            "BUTTON",
            "取消",
            common::rect(325, 520, 85, 38),
            WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        )?;
        self.label(
            hwnd,
            420,
            &format!("保存到 {}", self.draft.save_directory.display()),
            common::rect(24, 565, 386, 22),
        )?;
        self.change_format(hwnd, self.format);
        self.update_coordinates();
        self.resize(hwnd)
    }

    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_COMMAND => {
                let id = w.0 as u16;
                match id {
                    MP4 => self.change_format(hwnd, RecordingFormat::Mp4),
                    GIF => self.change_format(hwnd, RecordingFormat::Gif),
                    ADVANCED => {
                        self.expanded = !self.expanded;
                        self.resize(hwnd)?;
                    }
                    TARGET if (w.0 >> 16) as u32 == CBN_SELCHANGE => self.layout_body(hwnd)?,
                    COORDINATES => self.update_coordinates(),
                    REFRESH => {
                        if let Err(error) = self.refresh_windows() {
                            common::error(hwnd, &error);
                        }
                    }
                    PICK_COLOR => {
                        if let Err(error) = self.pick_color(hwnd) {
                            common::error(hwnd, &error);
                        }
                    }
                    START | ACCEPT_DIALOG => match self.config() {
                        Ok((region, config)) => {
                            runtime::emit(if region {
                                UiEvent::PickRecordingRegion(config)
                            } else {
                                UiEvent::StartRecording(config)
                            });
                            runtime::close(hwnd);
                        }
                        Err(error) => common::error(hwnd, &error),
                    },
                    CANCEL | CANCEL_DIALOG => runtime::close(hwnd),
                    _ => {}
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_CLOSE => {
                runtime::close(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_SIZE if self.controls.contains_key(&START) => {
                self.layout_footer(hwnd)?;
                self.layout_body(hwnd)?;
            }
            WM_VSCROLL => {
                let page = (common::client(hwnd).bottom - 110).max(24);
                let action = w.0 as u16;
                self.scroll += match action {
                    value if value == SB_LINEUP.0 as u16 => -28,
                    value if value == SB_LINEDOWN.0 as u16 => 28,
                    value if value == SB_PAGEUP.0 as u16 => -page,
                    value if value == SB_PAGEDOWN.0 as u16 => page,
                    value
                        if value == SB_THUMBPOSITION.0 as u16
                            || value == SB_THUMBTRACK.0 as u16 =>
                    {
                        (w.0 >> 16) as i32 - self.scroll
                    }
                    value if value == SB_TOP.0 as u16 => -self.scroll,
                    value if value == SB_BOTTOM.0 as u16 => 505,
                    _ => 0,
                };
                self.layout_body(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_MOUSEWHEEL => {
                self.scroll -= ((w.0 >> 16) as u16 as i16 as i32) / 120 * 56;
                self.layout_body(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_DRAWITEM => {
                let draw = unsafe { &*(l.0 as *const DRAWITEMSTRUCT) };
                let p = common::palette(self.draft.theme);
                let active = draw.CtlID == START as u32
                    || draw.CtlID
                        == if self.format == RecordingFormat::Mp4 {
                            MP4 as u32
                        } else {
                            GIF as u32
                        };
                common::button(
                    draw.hDC,
                    &common::get_text(draw.hwndItem),
                    draw.rcItem,
                    if active { p.accent } else { p.panel },
                    if active {
                        common::rgb(255, 255, 255)
                    } else {
                        p.text
                    },
                );
                return Ok(Some(LRESULT(1)));
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let dc = HDC(w.0 as *mut _);
                let p = common::palette(self.draft.theme);
                unsafe {
                    SetTextColor(dc, COLORREF(p.text));
                    SetBkColor(dc, COLORREF(p.panel));
                }
                return Ok(Some(LRESULT(self.brush.0 as isize)));
            }
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                let p = common::palette(self.draft.theme);
                let r = common::client(hwnd);
                common::fill(canvas.dc, r, p.panel);
                common::fill(
                    canvas.dc,
                    common::rect(0, r.bottom - 90, r.right, 1),
                    p.border,
                );
                return Ok(Some(LRESULT(0)));
            }
            _ => {}
        }
        Ok(None)
    }
}

fn select(hwnd: HWND, index: usize) {
    unsafe {
        SendMessageW(hwnd, CB_SETCURSEL, Some(WPARAM(index)), None);
    }
}
fn selected(hwnd: HWND) -> usize {
    unsafe { SendMessageW(hwnd, CB_GETCURSEL, None, None).0 as usize }
}
fn add_choice(hwnd: HWND, value: &str) {
    let value = common::wide(value);
    unsafe {
        SendMessageW(
            hwnd,
            CB_ADDSTRING,
            None,
            Some(LPARAM(value.as_ptr() as isize)),
        );
    }
}
fn parse_number(value: &str, label: &str) -> Result<u32, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{label}请填写有效的整数。"))
}
fn unique_output(directory: &Path, format: RecordingFormat) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if !directory.is_absolute() {
        return Err("保存目录必须是完整路径，请先在设置中选择保存目录。".into());
    }
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("读取系统时间失败：{e}"))?
        .as_nanos();
    let extension = if format == RecordingFormat::Mp4 {
        "mp4"
    } else {
        "gif"
    };
    for _ in 0..100 {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "cap-{time}-{}-{sequence}.{extension}",
            std::process::id()
        ));
        if !path
            .try_exists()
            .map_err(|e| format!("检查录制保存位置失败：{e}"))?
        {
            return Ok(path);
        }
    }
    Err("无法生成未占用的录制文件名。".into())
}

#[cfg(test)]
#[path = "record_setup_tests.rs"]
mod tests;
