use super::*;
use windows::Win32::UI::Controls::*;

struct Owned(HWND);
impl Drop for Owned {
    fn drop(&mut self) {
        runtime::close(self.0);
    }
}

struct Target(Rc<RefCell<Vec<usize>>>);
impl WindowHandler for Target {
    fn message(
        &mut self,
        _hwnd: HWND,
        message: u32,
        w: WPARAM,
        _l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        if message == TOOL_COMMAND {
            self.0.borrow_mut().push(w.0);
            return Ok(Some(LRESULT(0)));
        }
        Ok(None)
    }
}
fn owner() -> (Owned, Rc<RefCell<Vec<usize>>>) {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let window = runtime::create(
        Box::new(Target(commands.clone())),
        WindowOptions {
            title: "Cap owned toolbar target".into(),
            rect: common::rect(60, 60, 1, 1),
            style: WS_POPUP,
            ex_style: WS_EX_TOOLWINDOW,
            owner: None,
            visible: false,
        },
    )
    .unwrap();
    (Owned(window), commands)
}
fn tooltip_for(owner: HWND) -> HWND {
    struct Search {
        owner: HWND,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(data.0 as *mut Search) };
        let mut class = [0u16; 128];
        let count = unsafe { GetClassNameW(window, &mut class) };
        if unsafe { GetWindow(window, GW_OWNER) }.ok() == Some(search.owner)
            && String::from_utf16_lossy(&class[..count as usize]) == "tooltips_class32"
        {
            search.found.push(window);
        }
        windows::core::BOOL(1)
    }
    let mut search = Search {
        owner,
        found: Vec::new(),
    };
    unsafe { EnumWindows(Some(collect), LPARAM(&mut search as *mut Search as isize)) }.unwrap();
    assert_eq!(
        search.found.len(),
        1,
        "each toolbar must provide native full-name tooltips"
    );
    search.found[0]
}
fn tooltip_text(tooltip: HWND, bar: HWND, index: usize) -> String {
    let mut buffer = [0u16; 256];
    let mut info = TTTOOLINFOW {
        cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
        hwnd: bar,
        uId: index + 1,
        lpszText: windows::core::PWSTR(buffer.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            tooltip,
            TTM_GETTEXTW,
            Some(WPARAM(buffer.len())),
            Some(LPARAM(&mut info as *mut TTTOOLINFOW as isize)),
        );
    }
    String::from_utf16_lossy(
        &buffer[..buffer
            .iter()
            .position(|&value| value == 0)
            .unwrap_or(buffer.len())],
    )
}
fn pump() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            DispatchMessageW(&message);
        }
    }
}

#[test]
fn native_toolbar_exposes_full_tool_names_and_preserves_command_routes() {
    let (owner, commands) = owner();
    for modes in [false, true] {
        let bar = Owned(
            show(
                owner.0,
                Rc::new(RefCell::new(ToolbarState::default())),
                modes,
            )
            .unwrap(),
        );
        let tooltip = tooltip_for(bar.0);
        let font = HFONT(unsafe { SendMessageW(tooltip, WM_GETFONT, None, None) }.0 as *mut _);
        assert!(!font.is_invalid());
        let dc = unsafe { GetDC(Some(tooltip)) };
        let previous = unsafe { SelectObject(dc, font.into()) };
        let mut face = [0u16; 64];
        let count = unsafe { GetTextFaceW(dc, Some(&mut face)) };
        unsafe {
            SelectObject(dc, previous);
            ReleaseDC(Some(tooltip), dc);
        }
        assert!(count > 1);
        assert_eq!(
            String::from_utf16_lossy(&face[..count as usize - 1]),
            "HarmonyOS Sans SC",
            "toolbar tooltip must use the actual embedded UI font"
        );
        let count = unsafe { SendMessageW(tooltip, TTM_GETTOOLCOUNT, None, None) }.0;
        assert_eq!(
            count,
            if modes {
                4
            } else {
                super::CELLS.len() as isize
            }
        );
        assert_eq!(
            tooltip_text(tooltip, bar.0, 1),
            if modes { "滚动截图" } else { "椭圆" }
        );
        if !modes {
            assert_eq!(tooltip_text(tooltip, bar.0, 0), "矩形");
            assert!(tooltip_text(tooltip, bar.0, 13).contains("复制"));
        }
        let mut affinity = u32::MAX;
        unsafe { GetWindowDisplayAffinity(bar.0, &mut affinity) }.unwrap();
        assert_eq!(
            affinity, 0,
            "toolbar must remain visible in remote sessions"
        );
        commands.borrow_mut().clear();
        for index in 0..if modes { 4 } else { super::CELLS.len() } {
            let cell = super::button_rect_at(modes, index);
            let x = cell.left + (cell.right - cell.left) / 2;
            let y = cell.top + (cell.bottom - cell.top) / 2;
            unsafe {
                SendMessageW(
                    bar.0,
                    WM_LBUTTONUP,
                    Some(WPARAM(0)),
                    Some(LPARAM((x | (y << 16)) as isize)),
                );
            }
            pump();
        }
        let expected: Vec<usize> = if modes {
            (0..4).map(|index| 1000 + index).collect()
        } else {
            super::CELLS.iter().map(|cell| cell.command).collect()
        };
        assert_eq!(*commands.borrow(), expected);
        unsafe {
            let first = super::button_rect_at(modes, 0);
            let left = first.left + (first.right - first.left) / 2;
            SendMessageW(
                bar.0,
                WM_MOUSEMOVE,
                Some(WPARAM(0)),
                Some(LPARAM((left | (20 << 16)) as isize)),
            );
            let _ = ValidateRect(Some(bar.0), None);
            SendMessageW(
                bar.0,
                WM_MOUSEMOVE,
                Some(WPARAM(0)),
                Some(LPARAM(((left + 2) | (20 << 16)) as isize)),
            );
        }
        assert!(
            !unsafe { GetUpdateRect(bar.0, None, false) }.as_bool(),
            "moving inside one tool cell must not repaint the bar repeatedly"
        );
        pump();
    }
}

#[test]
fn native_toolbar_visual_artifacts_are_nonblank_and_have_distinct_finish_cancel_colors() {
    let (owner, _) = owner();
    let foreground = unsafe { GetForegroundWindow() };
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!(
            "toolbar-visual-{}",
            crate::platform::nonce().unwrap()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    for modes in [false, true] {
        let state = Rc::new(RefCell::new(ToolbarState {
            tool: Tool::Rectangle,
            can_undo: true,
            ..ToolbarState::default()
        }));
        let bar = Owned(show(owner.0, state, modes).unwrap());
        let (width, height) = super::bar_size(modes);
        unsafe {
            let _ = ShowWindow(bar.0, SW_SHOWNOACTIVATE);
            SetWindowPos(
                bar.0,
                Some(HWND_TOPMOST),
                100,
                120,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .unwrap();
            let _ = UpdateWindow(bar.0);
            windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
        }
        pump();
        let mut bounds = RECT::default();
        unsafe { GetWindowRect(bar.0, &mut bounds) }.unwrap();
        let image = cap_rust::capture::screenshot(
            cap_rust::geometry::Rect::new(
                bounds.left,
                bounds.top,
                (bounds.right - bounds.left) as u32,
                (bounds.bottom - bounds.top) as u32,
            )
            .unwrap(),
        )
        .unwrap();
        if !modes {
            // Feishu keeps 取消 (✗) red; the copy icon is neutral now.
            for (index, red) in [(16usize, true)] {
                let cell = super::button_rect_at(modes, index);
                let mut found = 0;
                for y in cell.top as u32..cell.bottom as u32 {
                    for x in cell.left as u32 + 6..cell.right as u32 - 6 {
                        let pixel = &image.pixels[((y * image.width + x) * 4) as usize..][..4];
                        if if red {
                            pixel[2] > 140 && u16::from(pixel[2]) > u16::from(pixel[1]) + 20
                        } else {
                            pixel[1] > 140 && u16::from(pixel[1]) > u16::from(pixel[2]) + 20
                        } {
                            found += 1;
                        }
                    }
                }
                assert!(
                    found > 8,
                    "cancel must stay a visibly distinct red action icon"
                );
            }
        }
        let path = directory.join(if modes { "modes.png" } else { "tools.png" });
        cap_rust::media::save_image(&path, &image).unwrap();
        eprintln!("Toolbar visual: {}", path.display());
    }
    if unsafe { IsWindow(Some(foreground)) }.as_bool() {
        unsafe {
            let _ = SetForegroundWindow(foreground);
        }
    }
}
