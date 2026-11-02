//! Native acceptance uses only a private deterministic scrollable test window.
use super::{tests::page, *};
use std::cell::Cell;

struct Page {
    offset: Rc<Cell<u32>>,
}
impl WindowHandler for Page {
    fn message(
        &mut self,
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        _: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        match message {
            WM_PAINT => {
                let canvas = common::Canvas::begin(hwnd);
                common::draw_frame(
                    canvas.dc,
                    &page(self.offset.get(), 240),
                    common::client(hwnd),
                )?;
            }
            WM_MOUSEWHEEL => {
                let delta = ((w.0 >> 16) as u16) as i16;
                self.offset.set(if delta < 0 {
                    self.offset.get().saturating_add(100).min(500)
                } else {
                    self.offset.get().saturating_sub(100)
                });
                common::invalidate(hwnd);
            }
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

struct Owned(HWND);
impl Drop for Owned {
    fn drop(&mut self) {
        runtime::close(self.0);
    }
}

fn pump() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = GdiFlush();
        windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
    }
}

fn setup() -> (Owned, Rc<Cell<u32>>, Rect) {
    let work = work_area(POINT { x: 0, y: 0 }).unwrap();
    let bounds = Rect::new(work.left + 40, work.top + 160, 160, 240).unwrap();
    let offset = Rc::new(Cell::new(0));
    let window = runtime::create(
        Box::new(Page {
            offset: offset.clone(),
        }),
        WindowOptions {
            title: "Cap owned scrolling test content".into(),
            rect: common::rect(
                bounds.x,
                bounds.y,
                bounds.width as i32,
                bounds.height as i32,
            ),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST,
            owner: None,
            visible: true,
        },
    )
    .unwrap();
    pump();
    assert_eq!(capture::screenshot(bounds).unwrap(), page(0, 240));
    (Owned(window), offset, bounds)
}

fn scroll_window(bounds: Rect) -> Owned {
    Owned(show(bounds, Rc::new(RefCell::new(Settings::default()))).unwrap())
}
fn action(hwnd: HWND, id: u16) {
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(id as usize)), None);
    }
}
fn tick(hwnd: HWND) {
    unsafe {
        SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER)), None);
    }
}
fn repaint(hwnd: HWND) {
    common::invalidate(hwnd);
    unsafe {
        let _ = UpdateWindow(hwnd);
    }
    pump();
}
fn preview() -> Frame {
    match runtime::next_event() {
        Some(UiEvent::PreviewImage(frame)) => frame,
        Some(UiEvent::Error(error)) => panic!("Unexpected capture error: {error}"),
        _ => panic!("Expected completed scrolling image"),
    }
}

fn outlines(owner: HWND) -> Vec<HWND> {
    struct Search {
        owner: HWND,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        if unsafe { GetWindow(hwnd, GW_OWNER) }.ok() == Some(search.owner) {
            search.found.push(hwnd);
        }
        windows::core::BOOL(1)
    }
    let mut search = Search {
        owner,
        found: vec![],
    };
    unsafe {
        EnumWindows(Some(collect), LPARAM((&mut search as *mut Search) as isize)).unwrap();
    }
    search.found
}

#[test]
fn owned_scrolling_auto_manual_undo_finish_keeps_exact_rows_and_pointer() {
    let (owner, offset, bounds) = setup();
    let foreground = unsafe { GetForegroundWindow() };
    let bar = scroll_window(bounds);
    assert_eq!(
        unsafe { GetForegroundWindow() },
        foreground,
        "Showing the bar must keep the target's focus"
    );
    let edges = outlines(bar.0);
    assert_eq!(edges.len(), 4);
    let expected = physical_edges(bounds).unwrap();
    for edge in &edges {
        let mut actual = RECT::default();
        let mut affinity = 0;
        unsafe {
            GetWindowRect(*edge, &mut actual).unwrap();
            GetWindowDisplayAffinity(*edge, &mut affinity).unwrap();
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
            assert_eq!(
                SendMessageW(*edge, WM_NCHITTEST, None, None).0,
                HTTRANSPARENT as isize
            );
            assert_ne!(
                GetWindowLongPtrW(*edge, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE.0,
                0
            );
        }
        assert!(
            expected
                .iter()
                .any(|r| actual == common::rect(r.x, r.y, r.width as i32, r.height as i32))
        );
    }
    let mut pointer_before = POINT::default();
    let mut pointer_after = POINT::default();
    unsafe {
        GetCursorPos(&mut pointer_before).unwrap();
    }
    action(bar.0, AUTO);
    pump();
    unsafe {
        GetCursorPos(&mut pointer_after).unwrap();
    }
    assert_eq!(
        pointer_before, pointer_after,
        "Automatic scrolling must never move the pointer"
    );
    assert_eq!(
        offset.get(),
        100,
        "The original content receives the wheel message"
    );
    action(bar.0, AUTO); // Pause automatic scrolling while manual capture remains active.
    tick(bar.0);
    tick(bar.0);
    action(bar.0, UNDO);
    offset.set(0);
    repaint(owner.0);
    tick(bar.0);
    tick(bar.0);
    ScrollTarget::new(bounds).unwrap().scroll_down().unwrap();
    pump();
    tick(bar.0);
    tick(bar.0);
    let button = unsafe { GetDlgItem(Some(bar.0), FINISH as i32) }.unwrap();
    unsafe {
        SendMessageW(button, WM_KEYDOWN, Some(WPARAM(VK_RETURN.0 as usize)), None);
    }
    pump();
    tick(bar.0);
    assert_eq!(preview(), page(0, 340));
    assert!(!unsafe { IsWindow(Some(bar.0)) }.as_bool());
    for edge in edges {
        assert!(!unsafe { IsWindow(Some(edge)) }.as_bool());
    }
    assert!(runtime::next_event().is_none());
}

#[test]
fn finishing_an_unsettled_last_viewport_requires_second_explicit_finish() {
    let (owner, offset, bounds) = setup();
    let bar = scroll_window(bounds);
    offset.set(300);
    repaint(owner.0);
    action(bar.0, FINISH);
    offset.set(400);
    repaint(owner.0);
    tick(bar.0);
    assert!(unsafe { IsWindow(Some(bar.0)) }.as_bool());
    assert!(runtime::next_event().is_none());
    action(bar.0, FINISH);
    assert_eq!(preview(), page(0, 240));
    assert!(!unsafe { IsWindow(Some(bar.0)) }.as_bool());
}

#[test]
fn finishing_a_stable_viewport_without_overlap_requires_explicit_retention() {
    let (owner, offset, bounds) = setup();
    let bar = scroll_window(bounds);
    offset.set(300);
    repaint(owner.0);
    action(bar.0, FINISH);
    tick(bar.0);
    assert!(unsafe { IsWindow(Some(bar.0)) }.as_bool());
    assert!(runtime::next_event().is_none());
    action(bar.0, FINISH);
    assert_eq!(preview(), page(0, 240));
}

#[test]
fn automatic_scrolling_stops_at_bottom_and_retains_all_confirmed_rows() {
    let (_owner, offset, bounds) = setup();
    let bar = scroll_window(bounds);
    action(bar.0, AUTO);
    for _ in 0..18 {
        pump();
        tick(bar.0);
    }
    assert_eq!(offset.get(), 500);
    let auto = unsafe { GetDlgItem(Some(bar.0), AUTO as i32) }.unwrap();
    assert_eq!(common::get_text(auto), "自动向下");
    action(bar.0, FINISH);
    tick(bar.0);
    assert_eq!(preview(), page(0, 740));
    assert!(runtime::next_event().is_none());
}

#[test]
fn target_closure_reports_failure_and_can_finish_retained_pixels() {
    let (owner, _, bounds) = setup();
    let bar = scroll_window(bounds);
    drop(owner);
    tick(bar.0);
    match runtime::next_event() {
        Some(UiEvent::Error(message)) => assert!(message.contains("目标窗口")),
        _ => panic!("Expected target closure error"),
    }
    action(bar.0, FINISH);
    assert_eq!(preview(), page(0, 240));
    assert!(runtime::next_event().is_none());
}

#[test]
fn cancel_from_focused_button_cleans_outline_and_temporary_escape_registration() {
    let (_owner, _, bounds) = setup();
    let hotkey_was_free =
        unsafe { RegisterHotKey(None, CANCEL_HOTKEY + 1, MOD_NOREPEAT, VK_ESCAPE.0 as u32) }
            .is_ok();
    if hotkey_was_free {
        unsafe {
            UnregisterHotKey(None, CANCEL_HOTKEY + 1).unwrap();
        }
    }
    let bar = scroll_window(bounds);
    let edges = outlines(bar.0);
    let button = unsafe { GetDlgItem(Some(bar.0), CANCEL as i32) }.unwrap();
    unsafe {
        SendMessageW(button, WM_KEYDOWN, Some(WPARAM(VK_ESCAPE.0 as usize)), None);
    }
    pump();
    assert!(!unsafe { IsWindow(Some(bar.0)) }.as_bool());
    for edge in edges {
        assert!(!unsafe { IsWindow(Some(edge)) }.as_bool());
    }
    if hotkey_was_free {
        unsafe {
            RegisterHotKey(None, CANCEL_HOTKEY + 1, MOD_NOREPEAT, VK_ESCAPE.0 as u32).unwrap();
            UnregisterHotKey(None, CANCEL_HOTKEY + 1).unwrap();
        }
    }
    assert!(runtime::next_event().is_none());
}

#[test]
fn outline_windows_have_physical_dpi_context_and_caller_context_is_restored() {
    let (_owner, _, bounds) = setup();
    let before = unsafe { GetThreadDpiAwarenessContext() };
    let bar = scroll_window(bounds);
    assert!(
        unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), before) }.as_bool()
    );
    for hwnd in outlines(bar.0).into_iter().chain([bar.0]) {
        assert_eq!(
            unsafe { GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(hwnd)) },
            DPI_AWARENESS_PER_MONITOR_AWARE
        );
    }
}
