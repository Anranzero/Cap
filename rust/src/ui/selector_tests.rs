use super::*;

struct Owned(HWND);
impl Drop for Owned {
    fn drop(&mut self) {
        runtime::close(self.0);
    }
}

struct KeyboardState([u8; 256]);
impl KeyboardState {
    fn save() -> Self {
        let mut keys = [0; 256];
        unsafe { GetKeyboardState(&mut keys) }.unwrap();
        Self(keys)
    }
    fn modifiers(&self, control: bool, shift: bool) {
        let mut keys = [0; 256];
        keys[VK_CONTROL.0 as usize] = if control { 0x80 } else { 0 };
        keys[VK_SHIFT.0 as usize] = if shift { 0x80 } else { 0 };
        unsafe { SetKeyboardState(&keys) }.unwrap();
    }
}
impl Drop for KeyboardState {
    fn drop(&mut self) {
        unsafe { SetKeyboardState(&self.0) }.unwrap();
    }
}

struct EditorHost(Rc<RefCell<Selector>>);
impl WindowHandler for EditorHost {
    fn message(
        &mut self,
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        self.0.borrow_mut().message(hwnd, message, w, l)
    }
}

fn editor() -> (Owned, Rc<RefCell<Selector>>, HWND) {
    while runtime::next_event().is_some() {}
    let frame = Frame::blank(320, 180, [255; 4]).unwrap();
    let mut model =
        SelectionModel::new(Rect::new(0, 0, 320, 180).unwrap(), CaptureMode::Screenshot);
    model.choose(Rect::new(0, 0, 320, 180).unwrap()).unwrap();
    let selector = Rc::new(RefCell::new(Selector {
        document: AnnotationDocument::new(frame.clone()).unwrap(),
        rendered: frame.clone(),
        dimmed: frame,
        back_buffer: None,
        model,
        format: RecordingFormat::Mp4,
        settings: Rc::new(RefCell::new(Settings::default())),
        toolbar: Rc::new(RefCell::new(ToolbarState {
            tool: Tool::Text,
            ..ToolbarState::default()
        })),
        mode_bar: HWND::default(),
        tools_bar: HWND::default(),
        options_bar: HWND::default(),
        magnifier: HWND::default(),
        magnifier_state: Rc::new(RefCell::new(super::super::magnifier::MagnifierState::new(
            Frame::blank(4, 4, [0; 4]).unwrap(),
            cap_rust::settings::Theme::Dark,
        ))),
        cropped: false,
        scroll_start: HWND::default(),
        record_panel: HWND::default(),
        ocr_card: HWND::default(),
        ocr_state: Rc::new(RefCell::new(super::super::ocr_card::OcrState::new(
            cap_rust::settings::Theme::Dark,
        ))),
        ocr_open: false,
        windows: Vec::new(),
        hover: None,
        drag: None,
        paint_start: None,
        points: Vec::new(),
        edit: None,
    }));
    let parent = runtime::create(
        Box::new(EditorHost(selector.clone())),
        WindowOptions {
            title: "Cap owned annotation text test".into(),
            rect: common::rect(80, 80, 320, 180),
            style: WS_POPUP,
            ex_style: WINDOW_EX_STYLE(0),
            owner: None,
            visible: true,
        },
    )
    .unwrap();
    unsafe {
        SendMessageW(
            parent,
            WM_LBUTTONDOWN,
            Some(WPARAM(0)),
            Some(LPARAM(10 | (10 << 16))),
        );
    }
    let edit = selector.borrow().edit.as_ref().unwrap().window;
    assert_eq!(unsafe { GetFocus() }, edit);
    (Owned(parent), selector, edit)
}

fn key(hwnd: HWND, value: u16) -> MSG {
    MSG {
        hwnd,
        message: WM_KEYDOWN,
        wParam: WPARAM(value as usize),
        ..Default::default()
    }
}

fn pump(hwnd: HWND) {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
            DispatchMessageW(&message);
        }
    }
}

#[test]
fn annotation_editor_keeps_enter_and_shift_enter_as_native_newlines() {
    let keyboard = KeyboardState::save();
    let (owner, selector, edit) = editor();
    for (shift, character) in [(false, 'a'), (true, 'b')] {
        keyboard.modifiers(false, shift);
        unsafe { SendMessageW(edit, WM_CHAR, Some(WPARAM(character as usize)), None) };
        assert!(
            !capture_text_key(&key(edit, VK_RETURN.0)),
            "Enter and Shift+Enter must reach the multiline EDIT"
        );
        unsafe { SendMessageW(edit, WM_CHAR, Some(WPARAM(13)), None) };
    }
    assert_eq!(common::get_text(edit), "a\r\nb\r\n");
    let dialog_code = unsafe { SendMessageW(edit, WM_GETDLGCODE, Some(WPARAM(13)), None) }.0 as u32;
    assert_ne!(dialog_code & DLGC_WANTALLKEYS, 0);
    pump(owner.0);
    assert!(selector.borrow().edit.is_some());
    assert!(!selector.borrow().document.can_undo());
}

#[test]
fn annotation_editor_ctrl_enter_commits_and_escape_cancels_only_text() {
    let keyboard = KeyboardState::save();
    for (key_code, control, commits) in [(VK_RETURN.0, true, true), (VK_ESCAPE.0, false, false)] {
        let (owner, selector, edit) = editor();
        keyboard.modifiers(control, false);
        unsafe { SendMessageW(edit, WM_CHAR, Some(WPARAM('x' as usize)), None) };
        assert!(capture_text_key(&key(edit, key_code)));
        pump(owner.0);
        assert!(selector.borrow().edit.is_none());
        assert!(!unsafe { IsWindow(Some(edit)) }.as_bool());
        assert!(unsafe { IsWindow(Some(owner.0)) }.as_bool());
        assert_eq!(selector.borrow().document.can_undo(), commits);
    }
}

#[test]
fn selection_and_paint_stop_when_an_owned_window_takes_mouse_capture() {
    let (owner, selector, _) = editor();
    unsafe {
        SendMessageW(owner.0, CANCEL_TEXT, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    let thief = Owned(
        runtime::create(
            Box::new(runtime::RootHandler),
            WindowOptions {
                title: "Cap owned capture-loss test".into(),
                rect: common::rect(0, 0, 1, 1),
                style: WS_POPUP,
                ex_style: WINDOW_EX_STYLE(0),
                owner: None,
                visible: false,
            },
        )
        .unwrap(),
    );
    for tool in [Tool::Select, Tool::Rectangle] {
        selector.borrow_mut().toolbar.borrow_mut().tool = tool;
        unsafe {
            SendMessageW(
                owner.0,
                WM_LBUTTONDOWN,
                Some(WPARAM(1)),
                Some(LPARAM(50 | (50 << 16))),
            );
            SetCapture(thief.0);
        }
        assert!(
            selector.borrow().drag.is_none(),
            "lost capture must stop region movement"
        );
        assert!(
            selector.borrow().paint_start.is_none(),
            "lost capture must discard unfinished strokes"
        );
        let selected = selector.borrow().model.selected;
        unsafe {
            SendMessageW(
                owner.0,
                WM_MOUSEMOVE,
                Some(WPARAM(0)),
                Some(LPARAM(100 | (90 << 16))),
            );
            SendMessageW(
                owner.0,
                WM_LBUTTONUP,
                Some(WPARAM(0)),
                Some(LPARAM(100 | (90 << 16))),
            );
            let _ = ReleaseCapture();
        }
        assert_eq!(selector.borrow().model.selected, selected);
        assert!(!selector.borrow().document.can_undo());
    }
}

#[test]
fn moving_within_the_same_hover_target_does_not_repaint_the_desktop() {
    let (owner, selector, _) = editor();
    unsafe {
        SendMessageW(owner.0, CANCEL_TEXT, Some(WPARAM(0)), Some(LPARAM(0)));
    }
    let bounds = Rect::new(0, 0, 320, 180).unwrap();
    {
        let mut selector = selector.borrow_mut();
        selector.model.selected = None;
        selector.hover = Some(bounds);
        selector.windows = vec![DesktopWindow {
            handle: 0,
            title: "owned hover fixture".into(),
            bounds,
        }];
    }
    unsafe {
        let _ = UpdateWindow(owner.0);
        let _ = ValidateRect(Some(owner.0), None);
    }
    for (x, y) in [(20, 30), (50, 70), (100, 100)] {
        unsafe {
            SendMessageW(
                owner.0,
                WM_MOUSEMOVE,
                Some(WPARAM(0)),
                Some(LPARAM(x | (y << 16))),
            );
        }
        assert!(
            !unsafe { GetUpdateRect(owner.0, None, false) }.as_bool(),
            "unchanged hover must not schedule a full desktop repaint"
        );
    }
}

#[test]
fn frozen_selector_and_toolbars_are_visible_to_remote_capture_without_polluting_export() {
    use windows::Win32::UI::HiDpi::*;
    struct Dpi(DPI_AWARENESS_CONTEXT);
    impl Drop for Dpi {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(!previous.0.is_null());
    let _dpi = Dpi(previous);
    while runtime::next_event().is_some() {}
    let monitor = capture::monitors()
        .unwrap()
        .into_iter()
        .find(|monitor| monitor.bounds.contains(0, 0))
        .expect("primary monitor")
        .bounds;
    let bounds = Rect::new(monitor.x + 80, monitor.y + 80, 320, 200).unwrap();
    let original = [50, 100, 200, 255];
    let owner = Owned(
        show(
            Frame::blank(320, 200, original).unwrap(),
            bounds,
            CaptureMode::Screenshot,
            RecordingFormat::Mp4,
            Rc::new(RefCell::new(Settings::default())),
        )
        .unwrap(),
    );
    let mut affinity = u32::MAX;
    unsafe { GetWindowDisplayAffinity(owner.0, &mut affinity) }.unwrap();
    assert_eq!(
        affinity, 0,
        "frozen selection must remain visible in remote capture"
    );
    unsafe {
        // The test launcher may override the process's first ShowWindow call.
        let _ = ShowWindow(owner.0, SW_SHOW);
        SetWindowPos(
            owner.0,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )
        .unwrap();
        let _ = UpdateWindow(owner.0);
        windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
    }
    pump(owner.0);
    std::thread::sleep(std::time::Duration::from_millis(150));
    unsafe {
        windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
    }
    let mut actual = RECT::default();
    unsafe { GetWindowRect(owner.0, &mut actual) }.unwrap();
    assert!(unsafe { IsWindowVisible(owner.0) }.as_bool());
    assert_eq!(
        (actual.left, actual.top, actual.right, actual.bottom),
        (bounds.x, bounds.y, bounds.right(), bounds.bottom())
    );
    let visible = capture::screenshot(bounds).unwrap();
    let index = ((160 * visible.width + 10) * 4) as usize;
    unsafe {
        SendMessageW(
            owner.0,
            WM_LBUTTONDOWN,
            Some(WPARAM(1)),
            Some(LPARAM(20 | (30 << 16))),
        );
        SendMessageW(
            owner.0,
            WM_MOUSEMOVE,
            Some(WPARAM(1)),
            Some(LPARAM(220 | (130 << 16))),
        );
        SendMessageW(
            owner.0,
            WM_LBUTTONUP,
            Some(WPARAM(0)),
            Some(LPARAM(220 | (130 << 16))),
        );
    }
    struct Search {
        owner: HWND,
        bars: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        if unsafe { GetWindow(window, GW_OWNER) }.ok() == Some(search.owner)
            && matches!(
                common::get_text(window).as_str(),
                "cap · 功能选择" | "cap · 截图批注"
            )
        {
            search.bars.push(window);
        }
        windows::core::BOOL(1)
    }
    let mut search = Search {
        owner: owner.0,
        bars: Vec::new(),
    };
    unsafe { EnumWindows(Some(collect), LPARAM((&mut search as *mut Search) as isize)) }.unwrap();
    assert_eq!(search.bars.len(), 2);
    for bar in search.bars {
        unsafe { GetWindowDisplayAffinity(bar, &mut affinity) }.unwrap();
        assert_eq!(
            affinity, 0,
            "selection toolbars must remain visible in remote capture"
        );
        assert!(unsafe { IsWindowVisible(bar) }.as_bool());
    }
    unsafe {
        SendMessageW(owner.0, TOOL_COMMAND, Some(WPARAM(12)), Some(LPARAM(0)));
    }
    let mut exported = None;
    while let Some(event) = runtime::next_event() {
        match event {
            UiEvent::Pin(frame) => exported = Some(frame),
            UiEvent::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    let exported = exported.expect("selection export");
    assert_eq!((exported.width, exported.height), (200, 100));
    assert!(
        exported
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel == original)
    );
    assert_eq!(
        &visible.pixels[index..index + 4],
        &[24, 48, 96, 255],
        "remote-compatible desktop capture must include the dimmed selector"
    );
}
