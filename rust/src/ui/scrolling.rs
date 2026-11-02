//! Native, conservative scrolling capture. The captured content keeps its input.
use super::{
    UiEvent, common,
    runtime::{self, WindowHandler, WindowOptions},
};
use cap_rust::{
    capture,
    geometry::Rect,
    pixels::Frame,
    scroll::{ScrollAppendResult, ScrollStitcher},
    settings::Settings,
};
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::*,
        HiDpi::*,
        Input::KeyboardAndMouse::*,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const AUTO: u16 = 301;
const UNDO: u16 = 302;
const FINISH: u16 = 303;
const CANCEL: u16 = 304;
const TIMER: usize = 1;
const CANCEL_HOTKEY: i32 = 0x5343;
const POLL_MS: u32 = 400;
const FINISH_SETTLE_MS: u32 = 160;
const BAR_WIDTH: i32 = 600;
const BAR_HEIGHT: i32 = 70;

pub fn show(bounds: Rect, settings: Rc<RefCell<Settings>>) -> Result<HWND, String> {
    let _dpi = PhysicalDpi::new()?;
    bounds.validate()?;
    if bounds.width < 64 || bounds.height < 80 {
        return Err("滚动截图区域至少需要 64 × 80 像素。".into());
    }
    physical_edges(bounds)?;
    let target = ScrollTarget::new(bounds)?;
    // Take the initial viewport before creating any of our own windows.
    let state = SessionState::new(capture::screenshot(bounds)?)?;
    let point = POINT {
        x: bounds.right() - 1,
        y: bounds.y,
    };
    let work = work_area(point)?;
    let x = (bounds.right() - BAR_WIDTH).clamp(work.left, (work.right - BAR_WIDTH).max(work.left));
    let mut y = bounds.y.saturating_sub(BAR_HEIGHT + 12);
    if y < work.top {
        y = bounds.bottom().saturating_add(12);
    }
    let y = y.clamp(work.top, (work.bottom - BAR_HEIGHT).max(work.top));
    let hwnd = runtime::create(
        Box::new(Scrolling {
            bounds,
            settings,
            target,
            state,
            hwnd: HWND::default(),
            outline: None,
            controls: vec![],
            status: "滚动鼠标滚轮截取".into(),
            automatic: false,
            unchanged: 0,
            paused: false,
            limit: false,
            retain_on_finish: false,
            finishing: None,
            hotkey: false,
            timer: false,
            stopped: false,
            drag: None,
        }),
        WindowOptions {
            title: "Cap 滚动截图".into(),
            rect: common::rect(x, y, BAR_WIDTH, BAR_HEIGHT),
            style: WS_POPUP | WS_CLIPCHILDREN,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            owner: None,
            visible: false,
        },
    )?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = UpdateWindow(hwnd);
    }
    Ok(hwnd)
}

// Window creation and hit-testing must use the same physical coordinates as capture.
struct PhysicalDpi(DPI_AWARENESS_CONTEXT);
impl PhysicalDpi {
    fn new() -> Result<Self, String> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err(format!(
                "设置滚动截图 DPI 上下文失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self(previous))
    }
}
impl Drop for PhysicalDpi {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

fn work_area(point: POINT) -> Result<RECT, String> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info) }
        .as_bool()
    {
        return Err(format!(
            "读取滚动截图显示器失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(info.rcWork)
}

fn physical_edges(bounds: Rect) -> Result<[Rect; 4], String> {
    const THICKNESS: u32 = 2;
    let left = bounds
        .x
        .checked_sub(THICKNESS as i32)
        .ok_or("滚动截图边框超出坐标范围。")?;
    let top = bounds
        .y
        .checked_sub(THICKNESS as i32)
        .ok_or("滚动截图边框超出坐标范围。")?;
    let width = bounds
        .width
        .checked_add(THICKNESS * 2)
        .ok_or("滚动截图边框宽度溢出。")?;
    Ok([
        Rect::new(left, top, width, THICKNESS)?,
        Rect::new(left, bounds.bottom(), width, THICKNESS)?,
        Rect::new(left, bounds.y, THICKNESS, bounds.height)?,
        Rect::new(bounds.right(), bounds.y, THICKNESS, bounds.height)?,
    ])
}

struct ScrollTarget {
    point: POINT,
    root: HWND,
}
impl ScrollTarget {
    fn new(bounds: Rect) -> Result<Self, String> {
        let point = POINT {
            x: bounds.x + (bounds.width / 2) as i32,
            y: bounds.y + (bounds.height / 2) as i32,
        };
        let root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
        if root.is_invalid() {
            return Err("选区中没有可滚动的目标窗口。".into());
        }
        Ok(Self { point, root })
    }
    fn validate(&self) -> Result<HWND, String> {
        let target = unsafe { WindowFromPoint(self.point) };
        if !unsafe { IsWindowVisible(self.root) }.as_bool()
            || target.is_invalid()
            || unsafe { GetAncestor(target, GA_ROOT) } != self.root
        {
            return Err("目标窗口已关闭或被遮挡，请移回选区后重试。".into());
        }
        Ok(target)
    }
    fn scroll_down(&self) -> Result<(), String> {
        let target = self.validate()?;
        let (x, y) = (i16::try_from(self.point.x), i16::try_from(self.point.y));
        let (Ok(x), Ok(y)) = (x, y) else {
            return Err("此屏幕坐标不支持自动滚动，请使用鼠标滚轮。".into());
        };
        let position = (x as u16 as u32) | ((y as u16 as u32) << 16);
        let wheel = (-120i16 as u16 as usize) << 16;
        unsafe {
            PostMessageW(
                Some(target),
                WM_MOUSEWHEEL,
                WPARAM(wheel),
                LPARAM(position as isize),
            )
        }
        .map_err(|e| format!("无法自动滚动此窗口，请使用鼠标滚轮：{e}"))
    }
}

struct Outline {
    edges: Vec<HWND>,
}
impl Outline {
    fn new(bounds: Rect, owner: HWND) -> Result<Self, String> {
        let mut outline = Self { edges: vec![] };
        for bounds in physical_edges(bounds)? {
            outline.edges.push(runtime::create(
                Box::new(Edge(bounds)),
                WindowOptions {
                    title: "cap · 滚动截图范围".into(),
                    rect: common::rect(
                        bounds.x,
                        bounds.y,
                        bounds.width as i32,
                        bounds.height as i32,
                    ),
                    style: WS_POPUP,
                    ex_style: WS_EX_TOPMOST
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOOLWINDOW
                        | WS_EX_TRANSPARENT,
                    owner: Some(owner),
                    visible: true,
                },
            )?);
        }
        Ok(outline)
    }
}
impl Drop for Outline {
    fn drop(&mut self) {
        for hwnd in self.edges.drain(..) {
            runtime::close(hwnd);
        }
    }
}
struct Edge(Rect);
impl WindowHandler for Edge {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        capture::exclude_window(hwnd.0 as isize, true)
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        _: WPARAM,
        _: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                common::fill(canvas.dc, common::client(hwnd), common::rgb(8, 125, 255));
            }
            WM_NCHITTEST => return Ok(Some(LRESULT(HTTRANSPARENT as isize))),
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_DPICHANGED => unsafe {
                SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    self.0.x,
                    self.0.y,
                    self.0.width as i32,
                    self.0.height as i32,
                    SWP_NOACTIVATE,
                )
                .map_err(|e| e.to_string())?;
            },
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

unsafe extern "system" fn button_keys(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_KEYDOWN
        && (w.0 == VK_ESCAPE.0 as usize
            || w.0 == VK_RETURN.0 as usize
            || w.0 == b'Z' as usize && unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0)
    {
        if let Ok(parent) = unsafe { GetParent(hwnd) } {
            if let Err(error) = unsafe { PostMessageW(Some(parent), WM_KEYDOWN, w, l) } {
                runtime::emit(UiEvent::Error(format!("发送滚动截图快捷键失败：{error}")));
            }
        }
        return LRESULT(0);
    }
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(button_keys), id);
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, w, l) }
}

struct Scrolling {
    bounds: Rect,
    settings: Rc<RefCell<Settings>>,
    target: ScrollTarget,
    state: SessionState,
    hwnd: HWND,
    outline: Option<Outline>,
    controls: Vec<(u16, HWND)>,
    status: String,
    automatic: bool,
    unchanged: u32,
    paused: bool,
    limit: bool,
    retain_on_finish: bool,
    finishing: Option<Frame>,
    hotkey: bool,
    timer: bool,
    stopped: bool,
    drag: Option<(POINT, RECT)>,
}
impl Scrolling {
    fn status(&mut self, message: &str) {
        self.status = message.into();
        common::invalidate(self.hwnd);
    }
    fn control(&self, id: u16) -> Option<HWND> {
        self.controls
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, h)| *h)
    }
    fn sync_controls(&self) {
        for &(id, hwnd) in &self.controls {
            let enabled = id == CANCEL
                || self.finishing.is_none()
                    && match id {
                        AUTO => !self.limit,
                        UNDO => self.state.stitcher.can_undo(),
                        _ => true,
                    };
            unsafe {
                let _ = EnableWindow(hwnd, enabled);
            }
            common::invalidate(hwnd);
        }
        if let Some(auto) = self.control(AUTO) {
            common::set_text(
                auto,
                if self.automatic {
                    "暂停自动"
                } else {
                    "自动向下"
                },
            );
        }
    }
    fn start_timer(&mut self, milliseconds: u32) -> Result<(), String> {
        if unsafe { SetTimer(Some(self.hwnd), TIMER, milliseconds, None) } == 0 {
            return Err(format!(
                "启动滚动截图计时器失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        self.timer = true;
        Ok(())
    }
    fn capture(&self) -> Result<Frame, String> {
        self.target.validate()?;
        capture::screenshot(self.bounds)
    }
    fn failed(&mut self, error: String) {
        self.automatic = false;
        self.paused = true;
        self.finishing = None;
        self.state.candidate = None;
        self.status("已暂停，可重试或完成");
        self.sync_controls();
        runtime::emit(UiEvent::Error(error));
    }
    fn append_status(&mut self, result: ScrollAppendResult) {
        match result {
            ScrollAppendResult::NoReliableOverlap => {
                self.automatic = false;
                self.status("未能拼接，请往回滚一点");
            }
            ScrollAppendResult::LimitReached => {
                self.automatic = false;
                self.limit = true;
                self.status("已达长度上限，请完成");
            }
            ScrollAppendResult::Appended | ScrollAppendResult::Unchanged => {
                self.retain_on_finish = false;
                self.status(if self.automatic {
                    "正在自动向下滚动"
                } else {
                    "滚动鼠标滚轮截取"
                });
            }
        }
        self.sync_controls();
    }
    fn scroll_down(&mut self) {
        if let Err(error) = self.target.scroll_down() {
            self.automatic = false;
            self.status("请用鼠标滚轮继续截取");
            self.sync_controls();
            runtime::emit(UiEvent::Error(error));
        }
    }
    fn toggle_auto(&mut self) -> Result<(), String> {
        if self.finishing.is_some() || self.limit {
            return Ok(());
        }
        if self.state.awaiting_retained {
            self.status("先往回滚到保留的位置");
            return Ok(());
        }
        self.automatic = !self.automatic;
        self.unchanged = 0;
        self.state.candidate = None;
        self.retain_on_finish = false;
        self.paused = false;
        self.start_timer(POLL_MS)?;
        self.status(if self.automatic {
            "正在自动向下滚动"
        } else {
            "滚动鼠标滚轮截取"
        });
        self.sync_controls();
        if self.automatic {
            self.scroll_down();
        }
        Ok(())
    }
    fn undo(&mut self) {
        if self.finishing.is_some() {
            return;
        }
        self.automatic = false;
        if self.state.undo() {
            self.paused = false;
            self.limit = false;
            self.retain_on_finish = false;
            self.status("已撤销，请往回滚一点");
        }
        self.sync_controls();
    }
    fn complete(&mut self) -> Result<(), String> {
        let frame = self.state.stitcher.create_frame()?;
        runtime::emit(UiEvent::PreviewImage(frame));
        self.close();
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        if self.finishing.is_some() {
            return Ok(());
        }
        self.automatic = false;
        self.state.candidate = None;
        if self.retain_on_finish || self.state.awaiting_retained || self.paused || self.limit {
            return self.complete();
        }
        match self.capture() {
            Ok(frame) => {
                self.finishing = Some(frame);
                self.status("正在等待末屏停稳…");
                self.sync_controls();
                self.start_timer(FINISH_SETTLE_MS)?;
            }
            Err(error) => self.failed(error),
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        if self.stopped || self.paused || self.limit {
            return Ok(());
        }
        let frame = match self.capture() {
            Ok(frame) => frame,
            Err(error) => {
                self.failed(error);
                return Ok(());
            }
        };
        if let Some(previous) = self.finishing.take() {
            self.start_timer(POLL_MS)?;
            if previous != frame {
                self.retain_on_finish = true;
                self.status("画面未停稳，再点完成保留");
                self.sync_controls();
                return Ok(());
            }
            let result = self.state.stitcher.try_append(&frame)?;
            if result == ScrollAppendResult::NoReliableOverlap {
                self.retain_on_finish = true;
                self.status("末屏未拼接，再点完成保留");
                self.sync_controls();
                return Ok(());
            }
            return self.complete();
        }
        if let Some(result) = self.state.observe(frame)? {
            self.append_status(result);
            if self.automatic {
                self.unchanged = if result == ScrollAppendResult::Unchanged {
                    self.unchanged + 1
                } else {
                    0
                };
                if self.unchanged >= 3 {
                    self.automatic = false;
                    self.status("画面未继续滚动，可完成");
                    self.sync_controls();
                } else {
                    self.scroll_down();
                }
            }
        }
        Ok(())
    }
    fn cleanup(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if self.timer && unsafe { IsWindow(Some(self.hwnd)) }.as_bool() {
            if let Err(error) = unsafe { KillTimer(Some(self.hwnd), TIMER) } {
                runtime::emit(UiEvent::Error(format!("停止滚动截图计时器失败：{error}")));
            }
        }
        self.timer = false;
        if self.hotkey {
            if let Err(error) = unsafe { UnregisterHotKey(Some(self.hwnd), CANCEL_HOTKEY) } {
                runtime::emit(UiEvent::Error(format!(
                    "释放滚动截图取消快捷键失败：{error}"
                )));
            }
            self.hotkey = false;
        }
        self.outline = None;
        self.finishing = None;
        self.state.candidate = None;
    }
    fn close(&mut self) {
        self.cleanup();
        runtime::close(self.hwnd);
    }
}
impl Drop for Scrolling {
    fn drop(&mut self) {
        self.cleanup();
    }
}
impl WindowHandler for Scrolling {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        self.hwnd = hwnd;
        capture::exclude_window(hwnd.0 as isize, true)?;
        for (id, label, x, width) in [
            (AUTO, "自动向下", 300, 92),
            (UNDO, "撤销", 398, 56),
            (FINISH, "完成", 460, 62),
            (CANCEL, "取消", 528, 62),
        ] {
            let child = common::child(
                hwnd,
                "BUTTON",
                label,
                id,
                common::rect(x, 17, width, 36),
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            )?;
            if !unsafe { SetWindowSubclass(child, Some(button_keys), 1, 0) }.as_bool() {
                return Err(format!(
                    "设置滚动截图按键失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            self.controls.push((id, child));
        }
        self.outline = Some(Outline::new(self.bounds, hwnd)?);
        self.hotkey =
            unsafe { RegisterHotKey(Some(hwnd), CANCEL_HOTKEY, MOD_NOREPEAT, VK_ESCAPE.0 as u32) }
                .is_ok();
        if !self.hotkey {
            self.status("滚动截取 · 点击取消结束");
        }
        self.start_timer(POLL_MS)?;
        self.sync_controls();
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
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                let p = common::palette(self.settings.borrow().theme);
                common::fill(canvas.dc, common::client(hwnd), p.border);
                common::rounded(
                    canvas.dc,
                    common::rect(1, 1, BAR_WIDTH - 2, BAR_HEIGHT - 2),
                    p.panel,
                    18,
                );
                common::label(canvas.dc, "⋮", common::rect(11, 10, 18, 48), p.muted);
                common::label(
                    canvas.dc,
                    &self.status,
                    common::rect(36, 8, 252, 24),
                    p.text,
                );
                common::label(
                    canvas.dc,
                    &format!(
                        "{} × {} · {}",
                        self.state.stitcher.width(),
                        self.state.stitcher.height(),
                        if self.hotkey {
                            "Esc 取消"
                        } else {
                            "点击取消"
                        }
                    ),
                    common::rect(36, 34, 252, 24),
                    p.muted,
                );
            }
            WM_DRAWITEM => {
                let item = unsafe { &*(l.0 as *const DRAWITEMSTRUCT) };
                let p = common::palette(self.settings.borrow().theme);
                let label = match item.CtlID as u16 {
                    AUTO => {
                        if self.automatic {
                            "暂停自动"
                        } else {
                            "自动向下"
                        }
                    }
                    UNDO => "撤销",
                    FINISH => "完成",
                    CANCEL => "取消",
                    _ => return Ok(None),
                };
                common::fill(item.hDC, item.rcItem, p.panel);
                common::button(
                    item.hDC,
                    label,
                    item.rcItem,
                    if item.CtlID == FINISH as u32 {
                        p.accent
                    } else {
                        p.bg
                    },
                    if item.itemState.0 & ODS_DISABLED.0 != 0 {
                        p.muted
                    } else if item.CtlID == FINISH as u32 {
                        common::rgb(255, 255, 255)
                    } else {
                        p.text
                    },
                );
                if item.itemState.0 & ODS_FOCUS.0 != 0 {
                    unsafe {
                        let _ = DrawFocusRect(item.hDC, &item.rcItem);
                    }
                }
                return Ok(Some(LRESULT(1)));
            }
            WM_TIMER if w.0 == TIMER => {
                if let Err(error) = self.tick() {
                    self.failed(error);
                }
            }
            WM_COMMAND => match w.0 as u16 {
                AUTO => self.toggle_auto()?,
                UNDO => self.undo(),
                FINISH => self.finish()?,
                CANCEL => self.close(),
                _ => return Ok(None),
            },
            WM_HOTKEY if w.0 == CANCEL_HOTKEY as usize => self.close(),
            WM_KEYDOWN if w.0 == VK_ESCAPE.0 as usize => self.close(),
            WM_KEYDOWN if w.0 == VK_RETURN.0 as usize => self.finish()?,
            WM_KEYDOWN
                if w.0 == b'Z' as usize && unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0 =>
            {
                self.undo()
            }
            WM_LBUTTONDOWN => {
                let mut point = POINT::default();
                let mut rect = RECT::default();
                unsafe {
                    GetCursorPos(&mut point).map_err(|e| e.to_string())?;
                    GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
                    SetCapture(hwnd);
                }
                self.drag = Some((point, rect));
            }
            WM_MOUSEMOVE if self.drag.is_some() => {
                let (start, rect) = self.drag.unwrap();
                let mut point = POINT::default();
                unsafe {
                    GetCursorPos(&mut point).map_err(|e| e.to_string())?;
                    SetWindowPos(
                        hwnd,
                        None,
                        rect.left + point.x - start.x,
                        rect.top + point.y - start.y,
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
                    let mut rect = RECT::default();
                    unsafe {
                        GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
                    }
                    let work = work_area(POINT {
                        x: rect.left,
                        y: rect.top,
                    })?;
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            None,
                            rect.left
                                .clamp(work.left, (work.right - BAR_WIDTH).max(work.left)),
                            rect.top
                                .clamp(work.top, (work.bottom - BAR_HEIGHT).max(work.top)),
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
            }
            WM_CAPTURECHANGED => self.drag = None,
            WM_CLOSE => self.close(),
            WM_DESTROY => self.cleanup(),
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

struct SessionState {
    stitcher: ScrollStitcher,
    candidate: Option<Frame>,
    awaiting_retained: bool,
}
impl SessionState {
    fn new(first: Frame) -> Result<Self, String> {
        Ok(Self {
            stitcher: ScrollStitcher::new(first)?,
            candidate: None,
            awaiting_retained: false,
        })
    }
    fn observe(&mut self, frame: Frame) -> Result<Option<ScrollAppendResult>, String> {
        frame.validate()?;
        if !self
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate == &frame)
        {
            self.candidate = Some(frame);
            return Ok(None);
        }
        self.candidate = None;
        if self.awaiting_retained {
            if !self.stitcher.matches_current_viewport(&frame) {
                return Ok(None);
            }
            self.awaiting_retained = false;
        }
        self.stitcher.try_append(&frame).map(Some)
    }
    fn undo(&mut self) -> bool {
        self.candidate = None;
        if !self.stitcher.undo() {
            return false;
        }
        self.awaiting_retained = true;
        true
    }
}

#[cfg(test)]
#[path = "scrolling_native_tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn page(start: u32, height: u32) -> Frame {
        let mut frame = Frame::blank(160, height, [0, 0, 0, 255]).unwrap();
        for y in 0..height {
            for x in 0u32..160 {
                let mut value = (y + start).wrapping_mul(73856093) ^ x.wrapping_mul(19349663);
                value ^= value >> 13;
                value = value.wrapping_mul(1274126177);
                value ^= value >> 16;
                let offset = ((y * 160 + x) * 4) as usize;
                frame.pixels[offset..offset + 3].copy_from_slice(&value.to_le_bytes()[..3]);
            }
        }
        frame
    }

    #[test]
    fn only_stable_forward_viewports_append_and_undo_requires_retained_position() {
        let mut state = SessionState::new(page(0, 240)).unwrap();
        assert_eq!(state.observe(page(100, 240)).unwrap(), None);
        assert_eq!(
            state.observe(page(100, 240)).unwrap(),
            Some(ScrollAppendResult::Appended)
        );
        assert_eq!(state.stitcher.create_frame().unwrap(), page(0, 340));
        assert!(state.undo());
        assert!(state.awaiting_retained);
        assert_eq!(state.observe(page(100, 240)).unwrap(), None);
        assert_eq!(state.observe(page(100, 240)).unwrap(), None);
        assert_eq!(state.stitcher.height(), 240);
        assert_eq!(state.observe(page(0, 240)).unwrap(), None);
        assert_eq!(
            state.observe(page(0, 240)).unwrap(),
            Some(ScrollAppendResult::Unchanged)
        );
        assert!(!state.awaiting_retained);
    }

    #[test]
    fn moving_frames_do_not_append_and_reversed_or_missing_overlap_preserves_pixels() {
        let mut state = SessionState::new(page(0, 240)).unwrap();
        for row in [10, 20, 30, 40] {
            assert_eq!(state.observe(page(row, 240)).unwrap(), None);
        }
        assert_eq!(state.stitcher.height(), 240);
        state.observe(page(300, 240)).unwrap();
        assert_eq!(
            state.observe(page(300, 240)).unwrap(),
            Some(ScrollAppendResult::NoReliableOverlap)
        );
        assert_eq!(state.stitcher.create_frame().unwrap(), page(0, 240));
    }

    #[test]
    fn outlines_preserve_physical_negative_coordinates_and_never_cover_selected_pixels() {
        let bounds = Rect::new(-1920, -200, 160, 240).unwrap();
        let edges = physical_edges(bounds).unwrap();
        assert_eq!(edges[0], Rect::new(-1922, -202, 164, 2).unwrap());
        for edge in edges {
            assert!(edge.intersect(bounds).is_empty());
        }
        assert!(physical_edges(Rect::new(i32::MIN, 0, 160, 240).unwrap()).is_err());
        assert!(physical_edges(Rect::new(i32::MAX - 160, 0, 160, 240).unwrap()).is_err());
    }

    #[test]
    fn a_changed_pixel_outside_overlap_sampling_prevents_unsettled_append() {
        let mut state = SessionState::new(page(0, 240)).unwrap();
        let mut next = page(100, 240);
        state.observe(next.clone()).unwrap();
        next.pixels[0] ^= 1;
        assert_eq!(state.observe(next.clone()).unwrap(), None);
        assert_eq!(state.stitcher.height(), 240);
        assert_eq!(
            state.observe(next).unwrap(),
            Some(ScrollAppendResult::Appended)
        );
    }

    #[test]
    fn length_limit_and_reversed_motion_never_commit_additional_pixels() {
        let mut state = SessionState::new(page(0, 240)).unwrap();
        state.stitcher = ScrollStitcher::with_limits(page(0, 240), 160 * 300, 300).unwrap();
        state.observe(page(100, 240)).unwrap();
        assert_eq!(
            state.observe(page(100, 240)).unwrap(),
            Some(ScrollAppendResult::LimitReached)
        );
        assert_eq!(state.stitcher.create_frame().unwrap(), page(0, 240));
        let mut state = SessionState::new(page(0, 240)).unwrap();
        state.observe(page(100, 240)).unwrap();
        state.observe(page(100, 240)).unwrap();
        state.observe(page(0, 240)).unwrap();
        assert_eq!(
            state.observe(page(0, 240)).unwrap(),
            Some(ScrollAppendResult::NoReliableOverlap)
        );
        assert_eq!(state.stitcher.create_frame().unwrap(), page(0, 340));
    }
}
