//! Native recording control bar. The backend owns the authoritative recording state.
use super::{
    UiEvent, common,
    runtime::{self, WindowHandler, WindowOptions},
};
use cap_rust::{
    capture,
    geometry::Rect,
    recording::{RecordingConfig, RecordingFormat, RecordingState, RecordingStatus},
    settings::Settings,
};
use std::{cell::RefCell, rc::Rc, time::Duration};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::DRAWITEMSTRUCT,
        HiDpi::*,
        Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled},
        WindowsAndMessaging::*,
    },
};

pub use super::record_setup::show_setup;

const PAUSE: u16 = 310;
const STOP: u16 = 311;
const TIMER: usize = 1;
const BAR_WIDTH: i32 = 350;
const BAR_HEIGHT: i32 = 64;

pub fn show_controls(
    config: &RecordingConfig,
    settings: Rc<RefCell<Settings>>,
) -> Result<(HWND, Rc<RefCell<RecordingStatus>>), String> {
    config.validate()?;
    let _dpi = PhysicalDpi::new()?;
    let position = control_position(config.bounds)?;
    let status = Rc::new(RefCell::new(RecordingStatus {
        state: RecordingState::Recording,
        duration: Duration::ZERO,
        frame_count: 0,
        dropped_frames: 0,
        backend: "正在启动".into(),
        failure: None,
    }));
    let hwnd = runtime::create(
        Box::new(ControlBar {
            settings,
            status: status.clone(),
            pause: HWND::default(),
            stop: HWND::default(),
            stopping: false,
            bounds: config.bounds,
            direct_region: config.format == RecordingFormat::Gif && config.window_handle == 0,
            edges: Vec::new(),
            previous_paused: false,
        }),
        WindowOptions {
            title: "cap · 录制中".into(),
            rect: position,
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            owner: None,
            visible: true,
        },
    )?;
    Ok((hwnd, status))
}

// Capture bounds and the two-pixel edge strips always use physical coordinates.
struct PhysicalDpi(DPI_AWARENESS_CONTEXT);
impl PhysicalDpi {
    fn new() -> Result<Self, String> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err(format!(
                "设置录制边框 DPI 上下文失败：{}",
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

fn control_position(bounds: Rect) -> Result<RECT, String> {
    let monitors = capture::monitors()?;
    let monitor = monitors
        .iter()
        .max_by_key(|m| {
            let r = m.bounds.intersect(bounds);
            r.width as u64 * r.height as u64
        })
        .ok_or("没有可用显示器。")?
        .bounds;
    let x = bounds
        .x
        .clamp(monitor.x, (monitor.right() - BAR_WIDTH).max(monitor.x));
    let below = bounds.bottom() as i64 + 8;
    let desired_y = if below + BAR_HEIGHT as i64 <= monitor.bottom() as i64 {
        below
    } else {
        bounds.y as i64 - BAR_HEIGHT as i64 - 8
    };
    let top = monitor.y.saturating_add(8);
    let bottom = monitor.bottom().saturating_sub(BAR_HEIGHT + 8).max(top);
    Ok(common::rect(
        x,
        desired_y.clamp(top as i64, bottom as i64) as i32,
        BAR_WIDTH,
        BAR_HEIGHT,
    ))
}

fn border_strips(bounds: Rect) -> Result<[RECT; 4], String> {
    bounds.validate()?;
    if bounds.is_empty() {
        return Err("录制范围不能为空。".into());
    }
    let left = bounds.x.checked_sub(2).ok_or("录制边框超出坐标范围。")?;
    let top = bounds.y.checked_sub(2).ok_or("录制边框超出坐标范围。")?;
    let right = bounds
        .right()
        .checked_add(2)
        .ok_or("录制边框超出坐标范围。")?;
    let bottom = bounds
        .bottom()
        .checked_add(2)
        .ok_or("录制边框超出坐标范围。")?;
    Ok([
        RECT {
            left,
            top,
            right,
            bottom: bounds.y,
        },
        RECT {
            left,
            top: bounds.bottom(),
            right,
            bottom,
        },
        RECT {
            left,
            top: bounds.y,
            right: bounds.x,
            bottom: bounds.bottom(),
        },
        RECT {
            left: bounds.right(),
            top: bounds.y,
            right,
            bottom: bounds.bottom(),
        },
    ])
}

struct ControlBar {
    settings: Rc<RefCell<Settings>>,
    status: Rc<RefCell<RecordingStatus>>,
    pause: HWND,
    stop: HWND,
    stopping: bool,
    bounds: Rect,
    direct_region: bool,
    edges: Vec<HWND>,
    previous_paused: bool,
}
impl ControlBar {
    fn close_edges(&mut self) {
        for edge in self.edges.drain(..) {
            runtime::close(edge);
        }
    }

    fn request_stop(&mut self, hwnd: HWND) {
        if self.stopping
            || matches!(
                self.status.borrow().state,
                RecordingState::Completed | RecordingState::Failed
            )
        {
            return;
        }
        self.stopping = true;
        common::set_text(hwnd, "cap · 正在保存…");
        unsafe {
            let _ = EnableWindow(self.pause, false);
            let _ = EnableWindow(self.stop, false);
        }
        for edge in &self.edges {
            common::invalidate(*edge);
        }
        common::invalidate(hwnd);
        runtime::emit(UiEvent::StopRecording);
    }

    fn update(&mut self, hwnd: HWND) {
        let status = self.status.borrow();
        let paused = status.state == RecordingState::Paused;
        let saving = self.stopping || status.state == RecordingState::Stopping;
        let active = matches!(
            status.state,
            RecordingState::Recording | RecordingState::Paused
        ) && !saving;
        common::set_text(self.pause, if paused { "继续" } else { "暂停" });
        unsafe {
            let _ = EnableWindow(self.pause, active);
            let _ = EnableWindow(self.stop, active);
        }
        if saving {
            common::set_text(hwnd, "cap · 正在保存…");
        } else if paused {
            common::set_text(hwnd, "cap · 已暂停");
        } else if status.state == RecordingState::Recording {
            common::set_text(hwnd, "cap · 录制中");
        }
        if paused != self.previous_paused || saving {
            for edge in &self.edges {
                common::invalidate(*edge);
            }
        }
        self.previous_paused = paused;
        common::invalidate(hwnd);
    }
}
impl Drop for ControlBar {
    fn drop(&mut self) {
        self.close_edges();
    }
}
impl WindowHandler for ControlBar {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        capture::exclude_window(hwnd.0 as isize, true)?;
        self.pause = common::child(
            hwnd,
            "BUTTON",
            "暂停",
            PAUSE,
            common::rect(222, 11, 54, 42),
            WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        )?;
        self.stop = common::child(
            hwnd,
            "BUTTON",
            "停止",
            STOP,
            common::rect(284, 11, 54, 42),
            WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        )?;
        if self.direct_region {
            for rect in border_strips(self.bounds)? {
                let edge = runtime::create(
                    Box::new(RegionEdge {
                        rect,
                        status: self.status.clone(),
                    }),
                    WindowOptions {
                        title: "cap · 录制范围".into(),
                        rect,
                        style: WS_POPUP,
                        ex_style: WS_EX_TOPMOST
                            | WS_EX_TOOLWINDOW
                            | WS_EX_TRANSPARENT
                            | WS_EX_NOACTIVATE,
                        owner: Some(hwnd),
                        visible: true,
                    },
                )?;
                self.edges.push(edge);
            }
        }
        if unsafe { SetTimer(Some(hwnd), TIMER, 100, None) } == 0 {
            return Err("无法启动录制控制条刷新。".into());
        }
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
            WM_TIMER => {
                self.update(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_COMMAND => {
                match w.0 as u16 {
                    PAUSE
                        if !self.stopping
                            && matches!(
                                self.status.borrow().state,
                                RecordingState::Recording | RecordingState::Paused
                            ) =>
                    {
                        runtime::emit(UiEvent::PauseRecording)
                    }
                    STOP => self.request_stop(hwnd),
                    _ => {}
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_CLOSE => {
                self.request_stop(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_DESTROY => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER);
                }
                self.close_edges();
            }
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_NCHITTEST => {
                let (x, y) = common::point(l);
                let mut point = POINT { x, y };
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut point);
                }
                return Ok(Some(LRESULT(if point.x < 215 {
                    HTCAPTION as isize
                } else {
                    HTCLIENT as isize
                })));
            }
            WM_DRAWITEM => {
                let draw = unsafe { &*(l.0 as *const DRAWITEMSTRUCT) };
                let p = common::palette(self.settings.borrow().theme);
                let enabled = unsafe { IsWindowEnabled(draw.hwndItem) }.as_bool();
                common::button(
                    draw.hDC,
                    &common::get_text(draw.hwndItem),
                    draw.rcItem,
                    if draw.CtlID == STOP as u32 && enabled {
                        common::rgb(233, 83, 104)
                    } else {
                        p.panel
                    },
                    if !enabled {
                        p.muted
                    } else if draw.CtlID == STOP as u32 {
                        common::rgb(255, 255, 255)
                    } else {
                        p.text
                    },
                );
                return Ok(Some(LRESULT(1)));
            }
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                let p = common::palette(self.settings.borrow().theme);
                common::fill(canvas.dc, common::client(hwnd), p.border);
                common::rounded(
                    canvas.dc,
                    common::rect(1, 1, BAR_WIDTH - 2, BAR_HEIGHT - 2),
                    p.bg,
                    28,
                );
                common::label(canvas.dc, "⠿", common::rect(12, 16, 22, 30), p.muted);
                let status = self.status.borrow();
                let paused = status.state == RecordingState::Paused;
                let saving = self.stopping || status.state == RecordingState::Stopping;
                common::rounded(
                    canvas.dc,
                    common::rect(39, 27, 8, 8),
                    if paused || saving {
                        common::rgb(163, 170, 188)
                    } else {
                        common::rgb(246, 107, 121)
                    },
                    8,
                );
                common::label(
                    canvas.dc,
                    &format_duration(status.duration),
                    common::rect(59, 5, 150, 30),
                    p.text,
                );
                let description = if saving {
                    "正在保存…".into()
                } else if paused {
                    "已暂停".into()
                } else if let Some(failure) = &status.failure {
                    format!("保存失败：{failure}")
                } else {
                    format!(
                        "{} · {} 帧{}",
                        status.backend,
                        status.frame_count,
                        if status.dropped_frames > 0 {
                            format!(" · 跳过 {}", status.dropped_frames)
                        } else {
                            String::new()
                        }
                    )
                };
                common::label(
                    canvas.dc,
                    &description,
                    common::rect(59, 33, 152, 22),
                    p.muted,
                );
                common::fill(canvas.dc, common::rect(213, 17, 1, 30), p.border);
                return Ok(Some(LRESULT(0)));
            }
            _ => {}
        }
        Ok(None)
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

struct RegionEdge {
    rect: RECT,
    status: Rc<RefCell<RecordingStatus>>,
}
impl RegionEdge {
    fn place(&self, hwnd: HWND) -> Result<(), String> {
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                self.rect.left,
                self.rect.top,
                self.rect.right - self.rect.left,
                self.rect.bottom - self.rect.top,
                SWP_NOACTIVATE,
            )
        }
        .map_err(|e| format!("定位录制边框失败：{e}"))
    }
}
impl WindowHandler for RegionEdge {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        capture::exclude_window(hwnd.0 as isize, true)?;
        self.place(hwnd)
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        _w: WPARAM,
        _l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match msg {
            WM_NCHITTEST => return Ok(Some(LRESULT(HTTRANSPARENT as isize))),
            WM_MOUSEACTIVATE => return Ok(Some(LRESULT(MA_NOACTIVATE as isize))),
            WM_DPICHANGED => {
                self.place(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                let color = if self.status.borrow().state == RecordingState::Paused {
                    common::rgb(163, 170, 188)
                } else {
                    common::rgb(8, 125, 255)
                };
                common::fill(canvas.dc, common::client(hwnd), color);
                return Ok(Some(LRESULT(0)));
            }
            _ => {}
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_rust::recording::RecordingFormat;

    fn owned_edges(owner: HWND) -> Vec<HWND> {
        struct Search {
            owner: HWND,
            edges: Vec<HWND>,
        }
        unsafe extern "system" fn callback(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
            let search = unsafe { &mut *(data.0 as *mut Search) };
            if unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default() == search.owner
                && common::get_text(hwnd) == "cap · 录制范围"
            {
                search.edges.push(hwnd);
            }
            windows::core::BOOL(1)
        }
        let mut search = Search {
            owner,
            edges: Vec::new(),
        };
        unsafe { EnumWindows(Some(callback), LPARAM(&mut search as *mut Search as isize)) }
            .unwrap();
        search.edges
    }

    struct OwnedTarget;
    struct TestWindow(HWND);
    impl Drop for TestWindow {
        fn drop(&mut self) {
            runtime::close(self.0);
        }
    }
    impl WindowHandler for OwnedTarget {
        fn message(
            &mut self,
            _hwnd: HWND,
            _msg: u32,
            _w: WPARAM,
            _l: LPARAM,
        ) -> Result<Option<LRESULT>, String> {
            Ok(None)
        }
    }

    #[test]
    fn recording_border_is_two_physical_pixels_strictly_outside_capture() {
        let bounds = Rect::new(-110, 70, 20, 10).unwrap();
        let edges = border_strips(bounds).unwrap();
        assert_eq!(
            edges.map(|r| (r.left, r.top, r.right, r.bottom)),
            [
                (-112, 68, -88, 70),
                (-112, 80, -88, 82),
                (-112, 70, -110, 80),
                (-90, 70, -88, 80)
            ]
        );
        assert!(border_strips(Rect::new(i32::MIN, 0, 1, 1).unwrap()).is_err());
    }

    #[test]
    fn recording_native_controls_emit_commands_wait_for_backend_and_clean_edges() {
        while runtime::next_event().is_some() {}
        let _dpi = PhysicalDpi::new().unwrap();
        let target = runtime::create(
            Box::new(OwnedTarget),
            WindowOptions {
                title: "Cap recording owned test target".into(),
                rect: common::rect(100, 100, 160, 100),
                style: WS_POPUP,
                ex_style: WS_EX_TOOLWINDOW,
                owner: None,
                visible: true,
            },
        )
        .unwrap();
        let _target_guard = TestWindow(target);
        let settings = Rc::new(RefCell::new(Settings::defaults(std::env::temp_dir())));
        let config = RecordingConfig::new(
            Rect::new(100, 100, 160, 100).unwrap(),
            std::env::temp_dir().join("cap-ui-test.gif"),
            RecordingFormat::Gif,
        );
        let (hwnd, status) = show_controls(&config, settings).unwrap();
        let _control_guard = TestWindow(hwnd);
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        let mut affinity = 0;
        unsafe {
            GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap();
        }
        assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
        let edges = owned_edges(hwnd);
        assert_eq!(edges.len(), 4);
        let expected = border_strips(config.bounds).unwrap();
        for edge in &edges {
            let mut actual = RECT::default();
            unsafe {
                GetWindowRect(*edge, &mut actual).unwrap();
                GetWindowDisplayAffinity(*edge, &mut affinity).unwrap();
            }
            assert!(expected.iter().any(|r| (r.left, r.top, r.right, r.bottom)
                == (actual.left, actual.top, actual.right, actual.bottom)));
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
            let styles = unsafe { GetWindowLongPtrW(*edge, GWL_EXSTYLE) } as u32;
            assert_eq!(
                styles & (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE).0,
                (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE).0
            );
            assert_eq!(
                unsafe { SendMessageW(*edge, WM_NCHITTEST, None, None) }.0,
                HTTRANSPARENT as isize
            );
        }
        unsafe {
            SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(310)), None);
        }
        assert!(matches!(
            runtime::next_event(),
            Some(UiEvent::PauseRecording)
        ));
        status.borrow_mut().state = RecordingState::Paused;
        status.borrow_mut().duration = Duration::from_secs(3662);
        unsafe {
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(1)), None);
        }
        let pause = unsafe { GetDlgItem(Some(hwnd), 310) }.unwrap();
        assert_eq!(common::get_text(pause), "继续");
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        for edge in &edges {
            unsafe {
                let _ = RedrawWindow(Some(*edge), None, None, RDW_INVALIDATE | RDW_UPDATENOW);
                let _ = GdiFlush();
                let _ = windows::Win32::Graphics::Dwm::DwmFlush();
                let dc = GetDC(Some(*edge));
                assert_eq!(GetPixel(dc, 0, 0).0, common::rgb(163, 170, 188));
                ReleaseDC(Some(*edge), dc);
            }
        }
        unsafe {
            SendMessageW(hwnd, WM_CLOSE, None, None);
        }
        assert!(matches!(
            runtime::next_event(),
            Some(UiEvent::StopRecording)
        ));
        assert!(
            unsafe { IsWindow(Some(hwnd)) }.as_bool(),
            "close must wait for backend save completion"
        );
        unsafe {
            SendMessageW(hwnd, WM_CLOSE, None, None);
        }
        assert!(runtime::next_event().is_none(), "stop must be idempotent");
        assert!(common::get_text(hwnd).contains("正在保存"));
        runtime::close(hwnd);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        assert!(
            edges
                .iter()
                .all(|edge| !unsafe { IsWindow(Some(*edge)) }.as_bool()),
            "control destruction must destroy every region edge"
        );
        runtime::close(target);
        assert!(runtime::next_event().is_none());
    }

    #[test]
    fn recording_wgc_controls_leave_region_border_to_the_system() {
        let settings = Rc::new(RefCell::new(Settings::defaults(std::env::temp_dir())));
        let config = RecordingConfig::new(
            Rect::new(100, 100, 160, 100).unwrap(),
            std::env::temp_dir().join("cap-ui-test.mp4"),
            RecordingFormat::Mp4,
        );
        let (hwnd, _) = show_controls(&config, settings).unwrap();
        assert!(owned_edges(hwnd).is_empty());
        runtime::close(hwnd);
        assert_eq!(format_duration(Duration::from_secs(3662)), "01:01:02");
    }
}
