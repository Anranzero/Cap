use super::*;

#[test]
fn recording_setup_cancel_does_not_start_and_region_uses_selected_options() {
    while runtime::next_event().is_some() {}
    let settings = Rc::new(RefCell::new(Settings::defaults(std::env::temp_dir())));
    let hwnd = show_setup(settings.clone()).unwrap();
    unsafe {
        SendMessageW(hwnd, WM_CLOSE, None, None);
    }
    assert!(runtime::next_event().is_none());
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    let hwnd = show_setup(settings).unwrap();
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(301)), None);
    }
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(320)), None);
    }
    match runtime::next_event().unwrap() {
        UiEvent::PickRecordingRegion(config) => {
            assert_eq!(config.format, RecordingFormat::Gif);
            assert_eq!(config.fps, 15);
            assert_eq!(config.output_width, 960);
            assert!(config.output_path.is_absolute());
            assert!(!config.output_path.exists());
        }
        _ => panic!("selected area must request the region picker"),
    }
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
}

#[test]
fn recording_setup_monitor_and_desktop_targets_emit_real_physical_bounds() {
    let settings = Rc::new(RefCell::new(Settings::defaults(std::env::temp_dir())));
    let monitors = capture::monitors().unwrap();
    assert!(!monitors.is_empty());
    for (index, expected) in [
        (0, monitors[0].bounds),
        (monitors.len(), capture::virtual_bounds().unwrap()),
    ] {
        let hwnd = show_setup(settings.clone()).unwrap();
        let target = unsafe { GetDlgItem(Some(hwnd), TARGET as i32) }.unwrap();
        let monitor = unsafe { GetDlgItem(Some(hwnd), MONITOR as i32) }.unwrap();
        common::set_text(
            unsafe { GetDlgItem(Some(hwnd), WIDTH as i32) }.unwrap(),
            "640",
        );
        select(target, 1);
        select(monitor, index);
        unsafe {
            SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(START as usize)), None);
        }
        match runtime::next_event().unwrap() {
            UiEvent::StartRecording(config) => {
                assert_eq!(config.bounds, expected);
                assert_eq!(config.window_handle, 0);
                assert_eq!(config.format, RecordingFormat::Mp4);
            }
            _ => panic!("monitor targets must start directly"),
        }
    }
}

#[test]
fn recording_setup_window_target_preserves_owned_handle_and_all_overlay_options() {
    struct OwnedTarget;
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
    let title = "Cap recording UI owned window target";
    let owned = runtime::create(
        Box::new(OwnedTarget),
        WindowOptions {
            title: title.into(),
            rect: common::rect(160, 140, 320, 200),
            style: WS_OVERLAPPEDWINDOW,
            ex_style: WINDOW_EX_STYLE(0),
            owner: None,
            visible: true,
        },
    )
    .unwrap();
    let mut values = Settings::defaults(std::env::temp_dir());
    values.relative_coordinates = true;
    values.coordinate_font_size = 32;
    values.coordinate_color = "#12AB34".into();
    values.show_cursor = false;
    values.highlight_clicks = false;
    let hwnd = show_setup(Rc::new(RefCell::new(values))).unwrap();
    let target = unsafe { GetDlgItem(Some(hwnd), TARGET as i32) }.unwrap();
    let window = unsafe { GetDlgItem(Some(hwnd), WINDOW as i32) }.unwrap();
    let title = common::wide(title);
    let index = unsafe {
        SendMessageW(
            window,
            CB_FINDSTRINGEXACT,
            Some(WPARAM(usize::MAX)),
            Some(LPARAM(title.as_ptr() as isize)),
        )
    }
    .0;
    assert!(index >= 0);
    select(target, 2);
    select(window, index as usize);
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(ADVANCED as usize)), None);
    }
    assert!(unsafe { IsWindowVisible(window) }.as_bool());
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(START as usize)), None);
    }
    match runtime::next_event().unwrap() {
        UiEvent::StartRecording(config) => {
            assert_eq!(config.window_handle, owned.0 as isize);
            assert_eq!(
                config.bounds,
                capture::window_bounds(owned.0 as isize).unwrap()
            );
            assert!(!config.overlays.show_cursor);
            assert!(!config.overlays.highlight_clicks);
            assert!(config.overlays.show_coordinates);
            assert!(config.overlays.relative_coordinates);
            assert_eq!(config.overlays.coordinate_font_size, 32);
            assert_eq!(config.overlays.coordinate_color, "#12AB34");
        }
        _ => panic!("window target must start directly"),
    }
    runtime::close(owned);
}

#[test]
fn recording_output_names_are_unique_and_numeric_input_rejects_invalid_values() {
    let first = unique_output(&std::env::temp_dir(), RecordingFormat::Gif).unwrap();
    let second = unique_output(&std::env::temp_dir(), RecordingFormat::Gif).unwrap();
    assert_ne!(first, second);
    assert!(unique_output(Path::new("relative"), RecordingFormat::Mp4).is_err());
    for invalid in ["", "-1", "1.5", "NaN", "4294967296"] {
        assert!(parse_number(invalid, "测试").is_err());
    }
    assert_eq!(parse_number(" 60 ", "帧率").unwrap(), 60);
}

#[test]
fn recording_setup_advanced_fields_remain_accessible_in_a_short_window() {
    let hwnd = show_setup(Rc::new(RefCell::new(Settings::defaults(
        std::env::temp_dir(),
    ))))
    .unwrap();
    unsafe {
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(ADVANCED as usize)), None);
        SetWindowPos(
            hwnd,
            None,
            20,
            20,
            SETUP_WIDTH,
            400,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .unwrap();
        SendMessageW(hwnd, WM_VSCROLL, Some(WPARAM(SB_BOTTOM.0 as usize)), None);
    }
    let font = unsafe { GetDlgItem(Some(hwnd), FONT_SIZE as i32) }.unwrap();
    let start = unsafe { GetDlgItem(Some(hwnd), START as i32) }.unwrap();
    let mut font_rect = RECT::default();
    let mut start_rect = RECT::default();
    unsafe {
        GetWindowRect(font, &mut font_rect).unwrap();
        GetWindowRect(start, &mut start_rect).unwrap();
    }
    assert!(unsafe { IsWindowVisible(font) }.as_bool());
    assert!(font_rect.bottom < start_rect.top);
    runtime::close(hwnd);
    assert!(runtime::next_event().is_none());
}
