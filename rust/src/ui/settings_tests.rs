use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, SetFocus, SetKeyboardState};

struct KeyboardState([u8; 256]);
impl KeyboardState {
    fn save() -> Self {
        let mut keys = [0; 256];
        unsafe { GetKeyboardState(&mut keys) }.unwrap();
        Self(keys)
    }
    fn modifiers(&self, control: bool, shift: bool) {
        let mut keys = [0; 256];
        keys[0x11] = if control { 0x80 } else { 0 };
        keys[0x10] = if shift { 0x80 } else { 0 };
        unsafe { SetKeyboardState(&keys) }.unwrap();
    }
}
impl Drop for KeyboardState {
    fn drop(&mut self) {
        unsafe { SetKeyboardState(&self.0) }.unwrap();
    }
}

struct OwnedSettings(HWND);
impl Drop for OwnedSettings {
    fn drop(&mut self) {
        runtime::close(self.0);
    }
}
fn window() -> OwnedSettings {
    OwnedSettings(
        show(Rc::new(RefCell::new(Settings::defaults(
            std::env::temp_dir(),
        ))))
        .unwrap(),
    )
}
fn child(hwnd: HWND, id: u16) -> HWND {
    unsafe { GetDlgItem(Some(hwnd), id as i32) }.unwrap()
}
fn bounds(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.unwrap();
    rect
}
fn screen_client(hwnd: HWND) -> RECT {
    let mut rect = common::client(hwnd);
    let mut origin = POINT::default();
    unsafe { ClientToScreen(hwnd, &mut origin) }.ok().unwrap();
    rect.left += origin.x;
    rect.right += origin.x;
    rect.top += origin.y;
    rect.bottom += origin.y;
    rect
}
fn resize(hwnd: HWND, width: i32, height: i32) {
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            20,
            20,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .unwrap();
}

#[test]
fn settings_native_uses_real_harmony_font_and_keeps_labels_inside_controls() {
    let owned = window();
    for id in [1, 101, 102, 103, 300, SAVE, EXIT, BROWSE] {
        let control = child(owned.0, id);
        let font = HFONT(unsafe { SendMessageW(control, WM_GETFONT, None, None) }.0 as *mut _);
        assert!(!font.is_invalid(), "control {id} must have a UI font");
        let dc = unsafe { GetDC(Some(control)) };
        let previous = unsafe { SelectObject(dc, font.into()) };
        let mut face = [0u16; 64];
        let count = unsafe { GetTextFaceW(dc, Some(&mut face)) };
        let mut metrics = TEXTMETRICW::default();
        let measured = unsafe { GetTextMetricsW(dc, &mut metrics) }.as_bool();
        let text: Vec<u16> = common::get_text(control).encode_utf16().collect();
        let mut size = SIZE::default();
        let fits =
            text.is_empty() || unsafe { GetTextExtentPoint32W(dc, &text, &mut size) }.as_bool();
        unsafe {
            SelectObject(dc, previous);
            ReleaseDC(Some(control), dc);
        }
        assert!(count > 1);
        let name = String::from_utf16_lossy(&face[..count as usize - 1]);
        assert_eq!(
            name, "HarmonyOS Sans SC",
            "control {id} fell back to another font"
        );
        assert!(measured && fits);
        let rect = common::client(control);
        assert!(
            metrics.tmHeight <= rect.bottom,
            "control {id} clips text vertically"
        );
        if id != 1 {
            assert!(
                size.cx <= rect.right,
                "control {id} clips its label horizontally"
            );
        }
    }
    assert!(runtime::next_event().is_none());
}

#[test]
fn settings_native_short_window_keeps_footer_reachable_and_enforces_minimum() {
    let owned = window();
    resize(owned.0, 740, 400);
    let client = screen_client(owned.0);
    for id in [SAVE, EXIT] {
        let rect = bounds(child(owned.0, id));
        assert!(
            rect.left >= client.left && rect.right <= client.right,
            "footer width {id}: {rect:?} {client:?}"
        );
        assert!(
            rect.top >= client.top && rect.bottom <= client.bottom,
            "footer height {id}: {rect:?} {client:?}"
        );
    }
    let mut limits = MINMAXINFO::default();
    unsafe {
        SendMessageW(
            owned.0,
            WM_GETMINMAXINFO,
            None,
            Some(LPARAM((&mut limits as *mut MINMAXINFO) as isize)),
        )
    };
    assert!(limits.ptMinTrackSize.x >= 640);
    assert!(limits.ptMinTrackSize.y >= 320);
    assert!(runtime::next_event().is_none());
}

#[test]
fn settings_native_scroll_reaches_last_recording_fields_and_preserves_edits() {
    let owned = window();
    unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(302)), None) };
    common::set_text(child(owned.0, 10), "60");
    resize(owned.0, 740, 400);
    let save = bounds(child(owned.0, SAVE));
    unsafe {
        SendMessageW(
            owned.0,
            WM_VSCROLL,
            Some(WPARAM(SB_BOTTOM.0 as usize)),
            None,
        )
    };
    let color = child(owned.0, 18);
    let recovery = child(owned.0, RECOVER);
    assert!(unsafe { IsWindowVisible(color) }.as_bool());
    assert!(unsafe { IsWindowVisible(recovery) }.as_bool());
    assert!(bounds(color).bottom < screen_client(owned.0).bottom);
    assert!(bounds(recovery).bottom < screen_client(owned.0).bottom);
    assert!(bounds(color).bottom < save.top);
    assert!(bounds(recovery).bottom < save.top);
    assert_eq!(bounds(child(owned.0, SAVE)), save);
    assert_eq!(common::get_text(child(owned.0, 10)), "60");
    unsafe { SendMessageW(owned.0, WM_VSCROLL, Some(WPARAM(SB_TOP.0 as usize)), None) };
    assert!(unsafe { IsWindowVisible(child(owned.0, 10)) }.as_bool());
    assert!(bounds(child(owned.0, 10)).top >= screen_client(owned.0).top + 82);
    unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(SAVE as usize)), None) };
    match runtime::next_event().unwrap() {
        UiEvent::SettingsSave(settings) => assert_eq!(settings.frames_per_second, 60),
        _ => panic!("save should use the edited values after scrolling"),
    }
    assert!(runtime::next_event().is_none());
}

#[test]
fn settings_native_capture_accepts_extended_keys_and_reports_unmappable_keys() {
    let owned = window();
    unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(301)), None) };
    let field = child(owned.0, HOTKEY_FIRST);
    unsafe { SetFocus(Some(field)) }.unwrap();
    for (key, expected) in [
        (0x60, "Ctrl+NumPad0"),
        (0x6b, "Ctrl+Add"),
        (0xad, "Ctrl+VolumeMute"),
        (0xe2, "Ctrl+OemBackslash"),
    ] {
        unsafe { SendMessageW(owned.0, CAPTURE_KEY, Some(WPARAM(key)), Some(LPARAM(2))) };
        assert_eq!(common::get_text(field), expected);
    }
    resize(owned.0, 740, 400);
    unsafe { SendMessageW(owned.0, CAPTURE_KEY, Some(WPARAM(0xff)), Some(LPARAM(2))) };
    assert_eq!(common::get_text(field), "Ctrl+OemBackslash");
    let status = child(owned.0, KEY_ERROR);
    assert!(!common::get_text(status).is_empty());
    assert!(unsafe { IsWindowVisible(status) }.as_bool());
    assert!(bounds(status).bottom <= bounds(child(owned.0, SAVE)).top);
    assert!(runtime::next_event().is_none());
}

#[test]
fn settings_native_capture_does_not_post_left_or_right_modifier_keys() {
    let owned = window();
    unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(301)), None) };
    let field = child(owned.0, HOTKEY_FIRST);
    unsafe { SetFocus(Some(field)) }.unwrap();
    for key in [
        0x10, 0x11, 0x12, 0x5b, 0x5c, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5,
    ] {
        assert!(capture_message(&MSG {
            hwnd: field,
            message: WM_KEYDOWN,
            wParam: WPARAM(key),
            ..Default::default()
        }));
        let mut pending = MSG::default();
        assert!(
            !unsafe {
                PeekMessageW(
                    &mut pending,
                    Some(owned.0),
                    CAPTURE_KEY,
                    CAPTURE_KEY,
                    PM_REMOVE,
                )
            }
            .as_bool(),
            "modifier {key:x} was posted"
        );
    }
}

#[test]
fn settings_placement_fits_short_negative_origin_workareas_and_actual_monitor() {
    for work in [
        common::rect(-1280, -200, 800, 500),
        common::rect(-1920, 40, 1920, 1040),
    ] {
        let rect = layout::placement(work);
        assert!(rect.left >= work.left && rect.right <= work.right);
        assert!(rect.top >= work.top && rect.bottom <= work.bottom);
        assert_eq!(rect.right - rect.left, 900.min(work.right - work.left));
        assert_eq!(rect.bottom - rect.top, 680.min(work.bottom - work.top));
    }
    let owned = window();
    let work = layout::work_area(owned.0).unwrap();
    let rect = bounds(owned.0);
    assert!(rect.left >= work.left && rect.right <= work.right);
    assert!(rect.top >= work.top && rect.bottom <= work.bottom);
}

#[test]
fn settings_native_every_tab_retains_visible_navigation_and_footer_when_short() {
    let owned = window();
    resize(owned.0, 740, 360);
    let client = screen_client(owned.0);
    for tab in 300..305 {
        unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(tab)), None) };
        for id in [SAVE, EXIT, 300, 301, 302, 303, 304] {
            let control = child(owned.0, id);
            let rect = bounds(control);
            assert!(unsafe { IsWindowVisible(control) }.as_bool());
            assert!(rect.left >= client.left && rect.right <= client.right);
            assert!(rect.top >= client.top && rect.bottom <= client.bottom);
        }
        unsafe {
            SendMessageW(
                owned.0,
                WM_VSCROLL,
                Some(WPARAM(SB_BOTTOM.0 as usize)),
                None,
            )
        };
    }
    assert!(runtime::next_event().is_none());
}

#[test]
fn settings_native_tab_and_shift_tab_leave_hotkey_fields_but_control_tab_is_captured() {
    let owned = window();
    unsafe { SendMessageW(owned.0, WM_COMMAND, Some(WPARAM(301)), None) };
    let keys = KeyboardState::save();
    let first = child(owned.0, HOTKEY_FIRST);
    let second = child(owned.0, HOTKEY_FIRST + 1);
    for (start, shift, expected) in [(first, false, second), (second, true, first)] {
        unsafe { SetFocus(Some(start)) }.unwrap();
        keys.modifiers(false, shift);
        let message = MSG {
            hwnd: start,
            message: WM_KEYDOWN,
            wParam: WPARAM(0x09),
            ..Default::default()
        };
        assert!(
            !capture_message(&message),
            "Tab navigation must reach IsDialogMessage"
        );
        let mut pending = MSG::default();
        assert!(
            !unsafe {
                PeekMessageW(
                    &mut pending,
                    Some(owned.0),
                    CAPTURE_KEY,
                    CAPTURE_KEY,
                    PM_REMOVE,
                )
            }
            .as_bool()
        );
        assert!(unsafe { IsDialogMessageW(owned.0, &message) }.as_bool());
        assert_eq!(unsafe { GetFocus() }, expected);
    }
    unsafe { SetFocus(Some(first)) }.unwrap();
    keys.modifiers(true, false);
    assert!(capture_message(&MSG {
        hwnd: first,
        message: WM_KEYDOWN,
        wParam: WPARAM(0x09),
        ..Default::default()
    }));
    let mut pending = MSG::default();
    assert!(
        unsafe {
            PeekMessageW(
                &mut pending,
                Some(owned.0),
                CAPTURE_KEY,
                CAPTURE_KEY,
                PM_REMOVE,
            )
        }
        .as_bool()
    );
    assert_eq!(pending.wParam.0, 0x09);
    assert_eq!(pending.lParam.0, 2);
    unsafe { DispatchMessageW(&pending) };
    assert_eq!(common::get_text(first), "Ctrl+Tab");
    assert!(runtime::next_event().is_none());
}
