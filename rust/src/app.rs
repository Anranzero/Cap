use crate::{
    platform::{self, Hotkeys, InstanceMutex},
    tray::Tray,
    ui::{
        self, UiEvent, common,
        runtime::{self, WindowOptions},
    },
};
use cap_rust::{
    capture,
    geometry::Rect,
    overlays::OverlayOptions,
    pixels::Frame,
    recording::{
        RecordingConfig, RecordingFormat, RecordingResult, RecordingSession, RecordingState,
        RecordingStatus,
    },
    recovery,
    selection::CaptureMode,
    settings::{Settings, SettingsStore},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

pub mod smoke;

pub struct Options {
    pub show_settings: bool,
    pub startup: bool,
    pub settings_directory: Option<PathBuf>,
    pub smoke: bool,
}
enum JobResult {
    Ocr(Result<String, String>),
    Recovery(Result<RecordingResult, String>),
}
struct Job {
    receiver: Receiver<JobResult>,
    worker: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}
pub struct App {
    root: HWND,
    settings: Rc<RefCell<Settings>>,
    store: SettingsStore,
    hotkeys: Option<Hotkeys>,
    tray: Option<Tray>,
    settings_window: HWND,
    selection_window: HWND,
    scroll_window: HWND,
    setup_window: HWND,
    recording: Option<RecordingSession>,
    controls: HWND,
    control_status: Option<Rc<RefCell<RecordingStatus>>>,
    pending_recording: Option<RecordingConfig>,
    job: Option<Job>,
    exiting: bool,
    restore_settings: bool,
    isolated: bool,
    smoke: bool,
    smoke_errors: Vec<String>,
    _instance: InstanceMutex,
}
impl App {
    pub fn run(options: Options) -> Result<(), String> {
        if let Some(mut app) = Self::new(options)? {
            app.message_loop()?;
        }
        Ok(())
    }
    fn new(options: Options) -> Result<Option<Self>, String> {
        let isolated = options.settings_directory.is_some();
        let instance_name = if let Some(directory) = &options.settings_directory {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            directory.hash(&mut hash);
            format!("Local\\cap.rust.profile.{:x}", hash.finish())
        } else {
            "Local\\cap.desktop.8E306F42".into()
        };
        let Some(instance) = InstanceMutex::acquire(&instance_name)? else {
            if !options.startup {
                common::error(
                    HWND::default(),
                    "cap 已在后台运行，请先从托盘退出旧版再启动 Rust 版。",
                );
            }
            return Ok(None);
        };
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let default_save = if let Some(directory) = &options.settings_directory {
            directory.join("captures")
        } else {
            platform::pictures()?.join("cap")
        };
        let local = platform::local_data()?;
        let directory = options
            .settings_directory
            .unwrap_or_else(|| local.join("cap-rust"));
        let store = SettingsStore {
            directory,
            default_save_directory: default_save.clone(),
        };
        let defaults = || {
            let mut settings = Settings::defaults(default_save.clone());
            if isolated {
                settings.screenshot_hotkey = "Ctrl+Shift+F10".into();
                settings.recording_hotkey = "Ctrl+Shift+F11".into();
                settings.gif_hotkey = "Ctrl+Shift+F12".into();
            }
            settings
        };
        let mut warning = None;
        let settings = if !isolated
            && !store.directory.join("settings.json").exists()
            && local.join("cap/settings.json").exists()
        {
            match store.read(&local.join("cap/settings.json")) {
                Ok(settings) => settings,
                Err(error) => {
                    warning = Some(error);
                    defaults()
                }
            }
        } else if !store.directory.join("settings.json").exists() {
            defaults()
        } else {
            match store.load() {
                Ok(settings) => settings,
                Err(error) => {
                    warning = Some(error);
                    defaults()
                }
            }
        };
        let root = runtime::create(
            Box::new(runtime::RootHandler),
            WindowOptions {
                title: "cap · Rust 后台".into(),
                rect: common::rect(0, 0, 1, 1),
                style: WS_OVERLAPPED,
                ex_style: WS_EX_TOOLWINDOW,
                owner: None,
                visible: false,
            },
        )?;
        let mut app = Self {
            root,
            settings: Rc::new(RefCell::new(settings)),
            store,
            hotkeys: Some(Hotkeys::new(root)),
            tray: Some(Tray::new(root)?),
            settings_window: HWND::default(),
            selection_window: HWND::default(),
            scroll_window: HWND::default(),
            setup_window: HWND::default(),
            recording: None,
            controls: HWND::default(),
            control_status: None,
            pending_recording: None,
            job: None,
            exiting: false,
            restore_settings: false,
            isolated,
            smoke: options.smoke,
            smoke_errors: Vec::new(),
            _instance: instance,
        };
        if !options.smoke {
            if let Err(error) = app
                .hotkeys
                .as_mut()
                .ok_or("快捷键服务未创建。")?
                .apply(&app.settings.borrow())
            {
                platform::log_error(&error);
                app.notify("快捷键注册失败", &error);
            }
        }
        if let Some(error) = warning {
            app.notify(
                "设置读取失败",
                &format!("已使用默认值，原文件保留。{error}"),
            );
        }
        unsafe {
            if SetTimer(Some(root), 1, 100, None) == 0 {
                return Err("无法创建后台状态计时器。".into());
            }
        }
        if options.show_settings {
            app.show_settings()?;
        }
        Ok(Some(app))
    }
    fn message_loop(&mut self) -> Result<(), String> {
        loop {
            while let Some(event) = runtime::next_event() {
                if let Err(error) = self.event(event) {
                    self.fail(error);
                }
            }
            if self.exiting && self.recording.is_none() && self.job.is_none() {
                break;
            }
            let mut message = MSG::default();
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if result == -1 {
                return Err(format!(
                    "Windows 消息循环失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            if result == 0 {
                self.exiting = true;
                continue;
            }
            if ui::settings::capture_message(&message) || ui::selector::capture_text_key(&message) {
                continue;
            }
            if message.message == WM_KEYDOWN && message.wParam.0 == 0x09 {
                let owner = unsafe { GetAncestor(message.hwnd, GA_ROOT) };
                if unsafe { IsDialogMessageW(owner, &message) }.as_bool() {
                    continue;
                }
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
    fn busy(&self) -> bool {
        self.recording.is_some()
            || self.job.is_some()
            || valid(self.selection_window)
            || valid(self.scroll_window)
    }
    fn hide_settings(&mut self) {
        if valid(self.settings_window) && unsafe { IsWindowVisible(self.settings_window) }.as_bool()
        {
            self.restore_settings = true;
            unsafe {
                let _ = ShowWindow(self.settings_window, SW_HIDE);
                let _ = windows::Win32::Graphics::Dwm::DwmFlush();
            }
        }
    }
    fn restore(&mut self) {
        if self.restore_settings && !self.busy() && !self.exiting {
            self.restore_settings = false;
            if valid(self.settings_window) {
                unsafe {
                    let _ = ShowWindow(self.settings_window, SW_SHOW);
                }
            }
        }
    }
    fn show_settings(&mut self) -> Result<(), String> {
        if self.exiting {
            return Ok(());
        }
        if valid(self.settings_window) {
            unsafe {
                let _ = ShowWindow(self.settings_window, SW_RESTORE);
                let _ = SetForegroundWindow(self.settings_window);
            }
        } else {
            self.settings_window = ui::settings::show(self.settings.clone())?;
        }
        Ok(())
    }
    fn begin(&mut self, mode: CaptureMode, format: RecordingFormat) -> Result<(), String> {
        if self.exiting {
            return Ok(());
        }
        if let Some(recording) = &self.recording {
            if mode == CaptureMode::Recording {
                recording.request_stop();
            }
            return Ok(());
        }
        if self.busy() {
            return Ok(());
        }
        self.hide_settings();
        let result = (|| {
            let bounds = capture::virtual_bounds()?;
            let frame = capture::screenshot(bounds)?;
            ui::selector::show(frame, bounds, mode, format, self.settings.clone())
        })();
        match result {
            Ok(window) => {
                self.selection_window = window;
                Ok(())
            }
            Err(error) => {
                self.restore();
                Err(error)
            }
        }
    }
    fn configuration(
        &self,
        bounds: Rect,
        format: RecordingFormat,
    ) -> Result<RecordingConfig, String> {
        let s = self.settings.borrow();
        let mut config = RecordingConfig::new(
            bounds,
            s.save_directory.join(format!(
                "cap-{}.{}",
                platform::nonce()?,
                if format == RecordingFormat::Mp4 {
                    "mp4"
                } else {
                    "gif"
                }
            )),
            format,
        );
        config.fps = if format == RecordingFormat::Mp4 {
            s.frames_per_second
        } else {
            s.gif_frames_per_second
        };
        config.output_width = if format == RecordingFormat::Gif {
            s.gif_max_width
        } else {
            0
        };
        config.overlays = OverlayOptions {
            show_cursor: s.show_cursor,
            highlight_clicks: s.highlight_clicks,
            show_coordinates: s.show_coordinates,
            relative_coordinates: s.relative_coordinates,
            coordinate_font_size: s.coordinate_font_size,
            coordinate_color: s.coordinate_color.clone(),
        };
        Ok(config)
    }
    fn start_recording(&mut self, config: RecordingConfig) -> Result<(), String> {
        if self.exiting || self.recording.is_some() || self.job.is_some() {
            return Ok(());
        }
        config.validate()?;
        let parent = config.output_path.parent().ok_or("录制输出目录无效。")?;
        std::fs::create_dir_all(parent).map_err(|e| format!("创建录制目录失败：{e}"))?;
        self.hide_settings();
        let session = RecordingSession::start(config.clone())?;
        match ui::record_ui::show_controls(&config, self.settings.clone()) {
            Ok((window, status)) => {
                self.recording = Some(session);
                self.controls = window;
                self.control_status = Some(status);
                Ok(())
            }
            Err(error) => {
                session.request_stop();
                let saved = session.stop();
                self.restore();
                Err(format!(
                    "无法显示录制控制条：{error}；保存结果：{}",
                    match saved {
                        Ok(result) => result.file_path.display().to_string(),
                        Err(error) => error,
                    }
                ))
            }
        }
    }
    fn event(&mut self, event: UiEvent) -> Result<(), String> {
        match event {
            UiEvent::Tick => self.tick()?,
            UiEvent::ShowSettings => self.show_settings()?,
            UiEvent::Begin(mode, format) => self.begin(mode, format)?,
            UiEvent::Hotkey(id) => {
                if !ui::settings::capture_registered(&self.settings.borrow(), id) {
                    let (mode, format) = match id {
                        1 => (CaptureMode::Screenshot, RecordingFormat::Mp4),
                        2 => (CaptureMode::Recording, RecordingFormat::Mp4),
                        _ => (CaptureMode::Recording, RecordingFormat::Gif),
                    };
                    self.begin(mode, format)?;
                }
            }
            UiEvent::Tray(message) => {
                if message == WM_RBUTTONUP as isize || message == WM_CONTEXTMENU as isize {
                    if let Some(tray) = &self.tray {
                        tray.menu()?;
                    }
                } else if message == WM_LBUTTONDBLCLK as isize {
                    self.begin(CaptureMode::Screenshot, RecordingFormat::Mp4)?;
                }
            }
            UiEvent::ShellRestart => {
                if let Some(tray) = &self.tray {
                    tray.restore()?;
                }
            }
            UiEvent::SelectionClosed => {
                self.selection_window = HWND::default();
                self.pending_recording = None;
                self.restore();
            }
            UiEvent::CopyImage(frame) => platform::copy_image(self.root, &frame)?,
            UiEvent::SaveImage(frame) => {
                platform::save_image(self.root, &self.settings.borrow().save_directory, &frame)?;
            }
            UiEvent::Pin(frame) => {
                ui::pin::show(frame, self.settings.clone())?;
            }
            UiEvent::Recognize(frame) => self.ocr(frame)?,
            UiEvent::BeginScroll(bounds) => {
                self.scroll_window = ui::scrolling::show(bounds, self.settings.clone())?;
            }
            UiEvent::PreviewImage(frame) => {
                ui::preview::show_image(frame, self.settings.clone())?;
                self.restore();
            }
            UiEvent::ConfigureRecording => {
                if !self.busy() && !valid(self.setup_window) {
                    self.setup_window = ui::record_ui::show_setup(self.settings.clone())?;
                }
            }
            UiEvent::PickRecordingRegion(config) => {
                let format = config.format;
                self.pending_recording = Some(config);
                self.begin(CaptureMode::Recording, format)?;
            }
            UiEvent::RecordRegion(bounds, format) => {
                let config = if let Some(mut pending) = self.pending_recording.take() {
                    pending.bounds = bounds;
                    pending.window_handle = 0;
                    pending
                } else {
                    self.configuration(bounds, format)?
                };
                self.start_recording(config)?;
            }
            UiEvent::StartRecording(config) => self.start_recording(config)?,
            UiEvent::PauseRecording => {
                if let Some(recording) = &self.recording {
                    if recording.status().state == RecordingState::Paused {
                        recording.resume();
                    } else {
                        recording.pause();
                    }
                }
            }
            UiEvent::StopRecording => {
                if let Some(recording) = &self.recording {
                    recording.request_stop();
                }
            }
            UiEvent::SettingsSave(settings) => self.save_settings(settings)?,
            UiEvent::PreviewTheme(theme) => {
                let mut settings = self.settings.borrow().clone();
                settings.theme = theme;
                self.store.save(&settings)?;
                self.settings.borrow_mut().theme = theme;
            }
            UiEvent::Recover(path) => self.recover(path)?,
            UiEvent::CopyFile(path) => platform::copy_file(self.root, &path)?,
            UiEvent::SaveFile(path) => {
                platform::save_file(self.root, &path)?;
            }
            UiEvent::OpenFolder(path) => platform::open_folder(self.root, &path)?,
            UiEvent::Error(error) => self.fail(error),
            UiEvent::Quit => {
                self.exiting = true;
                if let Some(recording) = &self.recording {
                    recording.request_stop();
                }
                if let Some(job) = &self.job {
                    job.cancel.store(true, Ordering::Release);
                }
                if !self.busy() {
                    runtime::close_all();
                }
            }
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        if let Some(recording) = &self.recording {
            if let Some(status) = &self.control_status {
                *status.borrow_mut() = recording.status();
            }
            if recording.result().is_some() {
                let result = self.recording.take().ok_or("录制会话已释放。")?.stop();
                runtime::close(self.controls);
                self.controls = HWND::default();
                self.control_status = None;
                match result {
                    Ok(result) => {
                        if let Some(warning) = result.warning {
                            platform::log_error(&warning);
                            self.notify("录制已保存", &warning);
                        }
                        if !self.exiting {
                            ui::preview::show(result.file_path, self.settings.clone())?;
                        }
                    }
                    Err(error) => self.fail(error),
                }
                self.restore();
            }
        }
        let completed = if let Some(job) = &self.job {
            match job.receiver.try_recv() {
                Ok(value) => Some(Ok(value)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("后台任务异常结束。".to_string())),
            }
        } else {
            None
        };
        if let Some(completed) = completed {
            let mut job = self.job.take().ok_or("后台任务已释放。")?;
            if let Some(worker) = job.worker.take() {
                if worker.join().is_err() {
                    return Err("后台工作线程异常结束。".into());
                }
            }
            if !self.exiting {
                match completed? {
                    JobResult::Ocr(text) => {
                        let text = text?;
                        platform::copy_text(self.root, &text)?;
                        self.notify("文字已复制", "可直接粘贴使用。");
                    }
                    JobResult::Recovery(result) => {
                        let result = result?;
                        ui::preview::show(result.file_path, self.settings.clone())?;
                    }
                }
            }
            self.restore();
        }
        if !valid(self.scroll_window) {
            self.scroll_window = HWND::default();
            self.restore();
        }
        Ok(())
    }
    fn save_settings(&mut self, settings: Settings) -> Result<(), String> {
        settings.validate()?;
        let previous = self.settings.borrow().clone();
        let previous_startup = if self.isolated {
            None
        } else {
            platform::startup_value()?
        };
        if !self.smoke {
            self.hotkeys
                .as_mut()
                .ok_or("快捷键服务未创建。")?
                .apply(&settings)?;
        }
        if let Err(error) = (if self.isolated {
            Ok(())
        } else {
            platform::set_startup(settings.run_at_startup)
        })
        .and_then(|_| self.store.save(&settings))
        {
            let rollback_keys = if self.smoke {
                Ok(())
            } else {
                self.hotkeys
                    .as_mut()
                    .ok_or("快捷键服务未创建。")?
                    .apply(&previous)
            };
            let rollback_startup = if self.isolated {
                Ok(())
            } else {
                platform::restore_startup(previous_startup.as_deref())
            };
            return Err(format!(
                "设置未保存：{error}{}{}",
                rollback_keys
                    .err()
                    .map(|e| format!("；快捷键回退失败：{e}"))
                    .unwrap_or_default(),
                rollback_startup
                    .err()
                    .map(|e| format!("；自启动回退失败：{e}"))
                    .unwrap_or_default()
            ));
        }
        *self.settings.borrow_mut() = settings;
        self.notify("设置已保存", "快捷键和录制选项已更新。");
        Ok(())
    }
    fn ocr(&mut self, frame: Frame) -> Result<(), String> {
        if self.job.is_some() {
            return Err("请等待当前任务结束。".into());
        }
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = thread::Builder::new()
            .name("Cap OCR".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cap_rust::media::ocr(&frame)
                }))
                .unwrap_or_else(|_| Err("文字识别线程异常。".into()));
                let _ = tx.send(JobResult::Ocr(result));
            })
            .map_err(|e| e.to_string())?;
        self.job = Some(Job {
            receiver,
            worker: Some(worker),
            cancel,
        });
        Ok(())
    }
    fn recover(&mut self, path: PathBuf) -> Result<(), String> {
        if self.recording.is_some() || self.job.is_some() {
            return Err("请先结束录制或等待当前任务完成。".into());
        }
        let header = recovery::inspect(&path)?;
        let extension = if header.format == RecordingFormat::Mp4 {
            "mp4"
        } else {
            "gif"
        };
        let directory = self.settings.borrow().save_directory.clone();
        let name = format!("cap-recovered-{}.{}", platform::nonce()?, extension);
        let Some(output) = platform::file_dialog(
            self.root,
            &directory,
            true,
            &format!("恢复的录制\0*.{extension}\0"),
            &name,
            extension,
        )?
        else {
            return Ok(());
        };
        let (tx, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = thread::Builder::new()
            .name("Cap recovery".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recovery::recover_with_cancellation(&path, &output, &token)
                }))
                .unwrap_or_else(|_| Err("录制恢复线程异常。".into()));
                let _ = tx.send(JobResult::Recovery(result));
            })
            .map_err(|e| e.to_string())?;
        self.job = Some(Job {
            receiver,
            worker: Some(worker),
            cancel,
        });
        self.notify("正在恢复录制", "完成后将显示预览；退出程序会取消恢复。");
        Ok(())
    }
    fn notify(&self, title: &str, body: &str) {
        if self.smoke {
            return;
        }
        if let Some(tray) = &self.tray {
            tray.notify(title, body);
        }
    }
    fn fail(&mut self, error: String) {
        platform::log_error(&error);
        if self.smoke {
            self.smoke_errors.push(error);
        } else if !self.exiting {
            common::error(self.root, &error);
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Release);
        }
        if let Some(recording) = &self.recording {
            recording.request_stop();
        }
        self.hotkeys.take();
        self.tray.take();
        runtime::close_all();
    }
}
fn valid(window: HWND) -> bool {
    !window.is_invalid() && unsafe { IsWindow(Some(window)) }.as_bool()
}
