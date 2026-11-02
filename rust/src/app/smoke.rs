//! End-to-end checks run only on an explicit isolated diagnostic profile.
use super::*;
use crate::ui::{runtime::WindowHandler, toolbar::TOOL_COMMAND};
use cap_rust::{media, settings::Theme};
use serde_json::json;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use windows::Win32::Graphics::{Dwm::DwmFlush, Gdi::*};

pub fn run(directory: &Path) -> Result<(), String> {
    if directory.exists()
        && fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("诊断输出目录必须为空，避免覆盖已有文件。".into());
    }
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let result = exercise(directory);
    let report = match &result {
        Ok(checks) => json!({"passed":true,"checks":checks,"user_clipboard":"untouched"}),
        Err(error) => json!({"passed":false,"error":error,"user_clipboard":"untouched"}),
    };
    fs::write(
        directory.join("smoke-result.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    result.map(|_| ())
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取对照配置失败：{error}")),
    }
}
struct Foreground(HWND);
impl Drop for Foreground {
    fn drop(&mut self) {
        if valid(self.0) {
            unsafe {
                let _ = SetForegroundWindow(self.0);
            }
        }
    }
}

fn exercise(directory: &Path) -> Result<Vec<String>, String> {
    let _foreground = Foreground(unsafe { GetForegroundWindow() });
    let local = platform::local_data()?;
    let paths = [
        local.join("cap/settings.json"),
        local.join("cap-rust/settings.json"),
    ];
    let before = [read_optional(&paths[0])?, read_optional(&paths[1])?];
    let startup = platform::startup_value()?;
    let mut app = App::new(Options {
        show_settings: true,
        startup: false,
        settings_directory: Some(directory.join("profile")),
        smoke: true,
    })?
    .ok_or("隔离诊断实例已存在。")?;
    let mut checks = Vec::new();
    let content = runtime::create(
        Box::new(Content),
        WindowOptions {
            title: "Cap Rust owned diagnostic pattern".into(),
            rect: common::rect(110, 100, 320, 200),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST,
            owner: None,
            visible: true,
        },
    )?;
    unsafe {
        let _ = ShowWindow(content, SW_SHOW);
    }
    pump(&mut app, Duration::from_millis(180))?;
    let screen = capture::screenshot(Rect::new(110, 100, 320, 200)?)?;
    let marker = ((80 * screen.width + 70) * 4) as usize;
    if screen.pixels[marker + 2] < 180 || screen.pixels[marker] > 75 {
        media::save_image(&directory.join("desktop-environment.png"), &screen)?;
        return Err(
            "自有测试图案没有出现在桌面捕获中；请解锁并保持当前交互会话可见后重试。".into(),
        );
    }
    selector_check(&mut app, directory)?;
    checks.push(
        "Native selector mouse input, rectangle annotation, pin event pixels and WIC PNG roundtrip"
            .into(),
    );

    unsafe {
        let _ = ShowWindow(content, SW_HIDE);
    }
    app.show_settings()?;
    for (theme, name) in [
        (Theme::Dark, "settings-dark.png"),
        (Theme::Light, "settings-light.png"),
    ] {
        app.event(UiEvent::PreviewTheme(theme))?;
        pump(&mut app, Duration::from_millis(180))?;
        screenshot_window(app.settings_window, &directory.join(name))?;
    }
    let mut settings = app.settings.borrow().clone();
    settings.screenshot_hotkey = "Ctrl+Shift+NumPad0".into();
    settings.run_at_startup = true;
    app.event(UiEvent::SettingsSave(settings))?;
    let saved = app.store.load()?;
    if saved.screenshot_hotkey != "Ctrl+Shift+NumPad0" || !saved.run_at_startup {
        return Err("隔离配置持久化不一致。".into());
    }
    checks.push("App theme routing and settings transaction in isolated profile; no startup or global hotkey mutations".into());
    let bounds = Rect::new(110, 100, 320, 200)?;
    app.pending_recording = Some(app.configuration(bounds, RecordingFormat::Gif)?);
    app.event(UiEvent::SelectionClosed)?;
    if app.pending_recording.is_some() {
        return Err("取消选区后录制参数未清理。".into());
    }
    checks.push("Cancelled selection clears pending recording settings".into());

    for format in [RecordingFormat::Gif, RecordingFormat::Mp4] {
        app.hide_settings();
        unsafe {
            SetWindowPos(
                content,
                Some(HWND_TOPMOST),
                110,
                100,
                320,
                200,
                SWP_SHOWWINDOW,
            )
            .map_err(|e| e.to_string())?;
            let _ = UpdateWindow(content);
        }
        pump(&mut app, Duration::from_millis(180))?;
        let extension = if format == RecordingFormat::Gif {
            "gif"
        } else {
            "mp4"
        };
        let output = directory.join(format!("recording.{extension}"));
        let mut config = RecordingConfig::new(bounds, output.clone(), format);
        config.fps = if format == RecordingFormat::Gif {
            10
        } else {
            15
        };
        config.window_handle = if format == RecordingFormat::Mp4 {
            content.0 as isize
        } else {
            0
        };
        config.overlays.show_cursor = false;
        config.overlays.highlight_clicks = false;
        config.overlays.show_coordinates = false;
        app.event(UiEvent::StartRecording(config))?;
        until(&mut app, Duration::from_secs(10), |a| {
            a.recording
                .as_ref()
                .is_some_and(|r| r.status().frame_count >= 3)
        })?;
        let backend = app
            .recording
            .as_ref()
            .ok_or("录制提前结束。")?
            .status()
            .backend;
        app.event(UiEvent::PauseRecording)?;
        until(&mut app, Duration::from_secs(2), |a| {
            a.recording
                .as_ref()
                .is_some_and(|r| r.status().state == RecordingState::Paused)
        })?;
        pump(&mut app, Duration::from_millis(180))?;
        app.event(UiEvent::PauseRecording)?;
        pump(&mut app, Duration::from_millis(220))?;
        app.event(UiEvent::StopRecording)?;
        until(&mut app, Duration::from_secs(15), |a| a.recording.is_none())?;
        let frame = if format == RecordingFormat::Gif {
            ui::gif_player::GifPlayer::new(&output)?.frame().copy()?
        } else {
            media::VideoDecoder::new(&output)?
                .next_frame()?
                .ok_or("MP4 无视频帧。")?
        };
        media::save_image(&directory.join(format!("decoded-{extension}.png")), &frame)?;
        if frame.width != 320 || frame.height != 200 {
            return Err(format!("{extension} 录制尺寸错误。"));
        }
        let index = ((80 * frame.width + 70) * 4) as usize;
        if frame.pixels[index + 2] < 180 || frame.pixels[index] > 75 {
            return Err(format!("{extension} 自有窗口红色色块未被正确捕获。"));
        }
        checks.push(format!("App {extension} recording start/pause/resume/stop/save/preview and decoded pixel oracle; backend={backend}"));
    }
    app.hide_settings();
    let exit_output = directory.join("quit-during-recording.gif");
    let mut config = RecordingConfig::new(bounds, exit_output.clone(), RecordingFormat::Gif);
    config.overlays.show_cursor = false;
    config.overlays.highlight_clicks = false;
    config.overlays.show_coordinates = false;
    app.start_recording(config)?;
    until(&mut app, Duration::from_secs(10), |a| {
        a.recording
            .as_ref()
            .is_some_and(|r| r.status().frame_count >= 3)
    })?;
    app.event(UiEvent::Quit)?;
    until(&mut app, Duration::from_secs(5), |a| {
        a.recording.is_none() && a.job.is_none()
    })?;
    ui::gif_player::GifPlayer::new(&exit_output)?;
    checks.push(
        "Quit during active recording waits for a valid finalized GIF before shutdown".into(),
    );
    drop(app);
    if before != [read_optional(&paths[0])?, read_optional(&paths[1])?]
        || startup != platform::startup_value()?
    {
        return Err("诊断期间用户配置或自启动发生变化。".into());
    }
    checks.push(
        "Clean shutdown; original and Rust user settings plus startup registry value unchanged"
            .into(),
    );
    Ok(checks)
}

fn pump(app: &mut App, duration: Duration) -> Result<(), String> {
    let deadline = Instant::now() + duration;
    loop {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message != WM_QUIT
                && !ui::settings::capture_message(&message)
                && !ui::selector::capture_text_key(&message)
            {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        while let Some(event) = runtime::next_event() {
            app.event(event)?;
        }
        app.tick()?;
        if !app.smoke_errors.is_empty() {
            return Err(app.smoke_errors.join("；"));
        }
        if Instant::now() >= deadline {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn until(app: &mut App, timeout: Duration, condition: impl Fn(&App) -> bool) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while !condition(app) {
        pump(app, Duration::from_millis(20))?;
        if Instant::now() >= deadline {
            return Err("主应用诊断等待超时。".into());
        }
    }
    Ok(())
}
fn screenshot_window(hwnd: HWND, path: &Path) -> Result<(), String> {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut r).map_err(|e| e.to_string())?;
        let _ = SetForegroundWindow(hwnd);
        DwmFlush().map_err(|e| e.to_string())?;
    }
    // A normal screenshot intentionally excludes Cap's capture-excluded windows.
    // Settings itself is not excluded, so it supplies a visual layout artifact.
    media::save_image(
        path,
        &capture::screenshot(Rect::new(
            r.left,
            r.top,
            (r.right - r.left) as u32,
            (r.bottom - r.top) as u32,
        )?)?,
    )
}
fn mouse(hwnd: HWND, msg: u32, x: i32, y: i32) {
    unsafe {
        SendMessageW(
            hwnd,
            msg,
            Some(WPARAM(if msg == WM_LBUTTONUP { 0 } else { 1 })),
            Some(LPARAM((((y as u32) << 16) | (x as u32 & 0xffff)) as isize)),
        );
    }
}
fn selector_check(app: &mut App, directory: &Path) -> Result<(), String> {
    let frame = Frame::blank(320, 200, [40, 80, 130, 255])?;
    let hwnd = ui::selector::show(
        frame,
        Rect::new(110, 100, 320, 200)?,
        CaptureMode::Screenshot,
        RecordingFormat::Mp4,
        app.settings.clone(),
    )?;
    app.selection_window = hwnd;
    mouse(hwnd, WM_LBUTTONDOWN, 10, 10);
    mouse(hwnd, WM_MOUSEMOVE, 210, 120);
    mouse(hwnd, WM_LBUTTONUP, 210, 120);
    unsafe {
        SendMessageW(hwnd, TOOL_COMMAND, Some(WPARAM(1)), Some(LPARAM(0)));
    }
    mouse(hwnd, WM_LBUTTONDOWN, 30, 30);
    mouse(hwnd, WM_MOUSEMOVE, 140, 70);
    mouse(hwnd, WM_LBUTTONUP, 140, 70);
    unsafe {
        SendMessageW(hwnd, TOOL_COMMAND, Some(WPARAM(12)), Some(LPARAM(0)));
    }
    let mut exported = None;
    while let Some(event) = runtime::next_event() {
        match event {
            UiEvent::Pin(frame) => exported = Some(frame),
            other => app.event(other)?,
        }
    }
    let exported = exported.ok_or("选区未生成贴图像素事件。")?;
    if exported.width != 200 || exported.height != 110 {
        return Err("原生选区尺寸错误。".into());
    }
    let at = ((20 * exported.width + 20) * 4) as usize;
    if exported.pixels[at..at + 3] == [40, 80, 130] {
        return Err("原生批注未进入导出像素。".into());
    }
    let path = directory.join("annotated-selection.png");
    media::save_image(&path, &exported)?;
    if media::load_image(&path)?.pixels != exported.pixels {
        return Err("PNG 像素往返不一致。".into());
    }
    Ok(())
}
struct Content;
impl WindowHandler for Content {
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        _w: WPARAM,
        _l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        if msg == WM_PAINT {
            let c = common::Canvas::begin(hwnd);
            common::fill(c.dc, common::client(hwnd), common::rgb(25, 35, 45));
            common::fill(
                c.dc,
                common::rect(30, 30, 130, 120),
                common::rgb(245, 20, 20),
            );
            common::fill(
                c.dc,
                common::rect(180, 30, 110, 120),
                common::rgb(20, 210, 40),
            );
            common::label(c.dc, "CAP RUST", common::rect(30, 160, 240, 30), 0xffffff);
            return Ok(Some(LRESULT(0)));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod trigger_tests {
    use super::*;

    #[test]
    fn native_hotkey_and_tray_action_show_full_desktop_selector() {
        let _com = platform::ComApartment::new().unwrap();
        let _foreground = Foreground(unsafe { GetForegroundWindow() });
        let profile = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "trigger-test-{}-{}",
                std::process::id(),
                platform::nonce().unwrap()
            ));
        let mut app = App::new(Options {
            show_settings: false,
            startup: false,
            settings_directory: Some(profile),
            smoke: true,
        })
        .unwrap()
        .expect("isolated test instance");
        {
            let mut settings = app.settings.borrow_mut();
            settings.screenshot_hotkey = "Ctrl+Shift+F22".into();
            settings.recording_hotkey = "Ctrl+Shift+F23".into();
            settings.gif_hotkey = "Ctrl+Shift+F24".into();
        }
        // Register distinct temporary keys through the same service as normal
        // startup; the posted message below exercises the root event route.
        app.hotkeys
            .as_mut()
            .unwrap()
            .apply(&app.settings.borrow())
            .unwrap();
        for from_hotkey in [true, false] {
            if from_hotkey {
                unsafe { PostMessageW(Some(app.root), WM_HOTKEY, WPARAM(1), LPARAM(0)) }.unwrap();
            } else {
                runtime::emit(UiEvent::Begin(
                    CaptureMode::Screenshot,
                    RecordingFormat::Mp4,
                ));
            }
            pump(&mut app, Duration::from_millis(120)).unwrap();
            assert!(
                valid(app.selection_window),
                "screenshot command did not create a selector"
            );
            assert!(
                unsafe { IsWindowVisible(app.selection_window) }.as_bool(),
                "selector was created but hidden"
            );
            let mut actual = RECT::default();
            unsafe { GetWindowRect(app.selection_window, &mut actual) }.unwrap();
            let expected = capture::virtual_bounds().unwrap();
            assert_eq!(
                (actual.left, actual.top, actual.right, actual.bottom),
                (expected.x, expected.y, expected.right(), expected.bottom())
            );
            unsafe { PostMessageW(Some(app.selection_window), WM_CLOSE, WPARAM(0), LPARAM(0)) }
                .unwrap();
            pump(&mut app, Duration::from_millis(80)).unwrap();
            assert!(
                !valid(app.selection_window),
                "closed selector remained active"
            );
        }
    }
}
