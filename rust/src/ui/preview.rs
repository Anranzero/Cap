//! Recording preview with bounded native GIF/Media Foundation playback.
#[cfg(test)]
mod gif_timing_tests;

use super::{
    UiEvent, common,
    gif_player::GifPlayer,
    runtime::{self, WindowHandler, WindowOptions},
};
use cap_rust::{media::VideoDecoder, pixels::Frame, settings::Settings};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};

const PLAY: u16 = 101;
const SAVE: u16 = 102;
const COPY: u16 = 103;
const TIMELINE: u16 = 104;
const TIMER: usize = 1;
const SLIDER_MAX: i64 = 10_000;
// The Windows SDK defines TBM_GETPOS as WM_USER; windows-rs omits the alias.
const TBM_GETPOS: u32 = WM_USER;

pub fn show(path: PathBuf, settings: Rc<RefCell<Settings>>) -> Result<HWND, String> {
    let metadata = std::fs::metadata(&path).map_err(|e| format!("读取录制文件失败：{e}"))?;
    if !metadata.is_file() {
        return Err("预览路径不是文件。".into());
    }
    let player = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "gif" => Player::Gif {
            player: GifPlayer::new(&path)?,
            next: Instant::now(),
            paused_at: None,
        },
        "mp4" => Player::Video(Video::new(&path)?),
        _ => return Err("录制预览支持 GIF 和 MP4 文件。".into()),
    };
    open(player, path, metadata.len(), settings, "cap · 录制预览")
}

pub fn show_image(frame: Frame, settings: Rc<RefCell<Settings>>) -> Result<HWND, String> {
    frame.validate()?;
    open(
        Player::Image(frame),
        PathBuf::new(),
        0,
        settings,
        "cap · 截图预览",
    )
}

fn open(
    player: Player,
    path: PathBuf,
    bytes: u64,
    settings: Rc<RefCell<Settings>>,
    title: &str,
) -> Result<HWND, String> {
    let width = 880;
    let height = 640;
    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor).map_err(|e| e.to_string())?;
    }
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe {
        GetMonitorInfoW(
            MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
    }
    .as_bool()
    {
        return Err(format!(
            "读取预览显示器失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let work = monitor.rcWork;
    let width = width.min(work.right - work.left);
    let height = height.min(work.bottom - work.top);
    let last_theme = settings.borrow().theme;
    // Result windows must reach the foreground: a plain show can stay behind the
    // settings window, and a later taskbar click then raises the wrong window.
    let window = runtime::create(
        Box::new(Preview {
            path,
            bytes,
            settings,
            player,
            play: HWND::default(),
            save: HWND::default(),
            copy: HWND::default(),
            slider: HWND::default(),
            seeking: false,
            last_label: String::new(),
            last_theme,
        }),
        WindowOptions {
            title: title.into(),
            rect: common::rect(
                work.left + (work.right - work.left - width) / 2,
                work.top + (work.bottom - work.top - height) / 2,
                width,
                height,
            ),
            style: WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            ex_style: WINDOW_EX_STYLE(0),
            owner: None,
            visible: true,
        },
    )?;
    unsafe {
        let _ = SetForegroundWindow(window);
    }
    Ok(window)
}

struct PlaybackClock {
    offset: i64,
    start: Instant,
    playing: bool,
}
impl PlaybackClock {
    fn new(now: Instant) -> Self {
        Self {
            offset: 0,
            start: now,
            playing: true,
        }
    }
    fn position(&self, now: Instant) -> i64 {
        let elapsed = if self.playing {
            (now.saturating_duration_since(self.start).as_nanos() / 100).min(i64::MAX as u128)
                as i64
        } else {
            0
        };
        self.offset.saturating_add(elapsed)
    }
    fn seek(&mut self, position: i64, now: Instant) {
        self.offset = position.max(0);
        self.start = now;
    }
    fn toggle(&mut self, now: Instant) {
        self.offset = self.position(now);
        self.start = now;
        self.playing = !self.playing;
    }
}

struct Sample {
    frame: Frame,
    time: i64,
}
struct Video {
    decoder: VideoDecoder,
    frame: Frame,
    pending: Option<Sample>,
    duration: i64,
    decoded_end: i64,
    clock: PlaybackClock,
}
impl Video {
    fn new(path: &Path) -> Result<Self, String> {
        let mut decoder = VideoDecoder::new(path)?;
        let frame = decoder.next_frame()?.ok_or("MP4 中没有视频帧。")?;
        let duration = decoder.duration_ticks;
        let decoded_end = decoder
            .last_time
            .saturating_add(decoder.last_duration.max(1));
        let mut video = Self {
            decoder,
            frame,
            pending: None,
            duration,
            decoded_end,
            clock: PlaybackClock::new(Instant::now()),
        };
        video.read_next()?;
        Ok(video)
    }
    fn read_next(&mut self) -> Result<(), String> {
        self.pending = self.decoder.next_frame()?.map(|frame| Sample {
            frame,
            time: self.decoder.last_time,
        });
        self.decoded_end = self.decoded_end.max(
            self.decoder
                .last_time
                .saturating_add(self.decoder.last_duration.max(1)),
        );
        if self.pending.is_none() {
            self.duration = self.duration.max(self.decoded_end);
        }
        Ok(())
    }
    fn seek(&mut self, position: i64, now: Instant) -> Result<(), String> {
        let target = position.max(0).min(if self.duration > 0 {
            self.duration - 1
        } else {
            i64::MAX
        });
        self.decoder.seek(target)?;
        self.frame = self
            .decoder
            .next_frame()?
            .ok_or("目标位置没有可解码的视频帧。")?;
        self.read_next()?;
        self.clock.seek(target, now);
        Ok(())
    }
    fn tick(&mut self, now: Instant) -> Result<bool, String> {
        if !self.clock.playing {
            return Ok(false);
        }
        let position = self.clock.position(now);
        if self.duration > 0 && position >= self.duration {
            self.seek(position % self.duration, now)?;
            return Ok(true);
        }
        let mut changed = false;
        // Keep event processing responsive when the decoder briefly falls behind.
        for _ in 0..8 {
            if self
                .pending
                .as_ref()
                .is_none_or(|sample| sample.time > position)
            {
                break;
            }
            self.frame = self.pending.take().unwrap().frame;
            self.read_next()?;
            changed = true;
        }
        Ok(changed)
    }
}

enum Player {
    Image(Frame),
    Gif {
        player: GifPlayer,
        next: Instant,
        paused_at: Option<Instant>,
    },
    Video(Video),
}
impl Player {
    fn frame(&self) -> &Frame {
        match self {
            Self::Image(frame) => frame,
            Self::Gif { player, .. } => player.frame(),
            Self::Video(video) => &video.frame,
        }
    }
    fn tick(&mut self, now: Instant) -> Result<bool, String> {
        match self {
            Self::Image(_) => Ok(false),
            Self::Video(video) => video.tick(now),
            Self::Gif {
                player,
                next,
                paused_at,
            } => {
                if paused_at.is_some() {
                    return Ok(false);
                }
                let mut changed = false;
                // Preserve recorded timing without blocking the window during a stall.
                for _ in 0..8 {
                    if now < *next {
                        break;
                    }
                    player.advance()?;
                    *next += std::time::Duration::from_millis(player.delay() as u64);
                    changed = true;
                }
                Ok(changed)
            }
        }
    }
    fn playing(&self) -> bool {
        match self {
            Self::Image(_) => false,
            Self::Video(v) => v.clock.playing,
            Self::Gif { paused_at, .. } => paused_at.is_none(),
        }
    }
    fn toggle(&mut self) {
        self.toggle_at(Instant::now());
    }
    fn toggle_at(&mut self, now: Instant) {
        match self {
            Self::Image(_) => {}
            Self::Video(v) => v.clock.toggle(now),
            Self::Gif {
                paused_at, next, ..
            } => {
                if let Some(paused) = paused_at.take() {
                    *next += now.saturating_duration_since(paused);
                } else {
                    *paused_at = Some(now);
                }
            }
        }
    }
    fn time(&self) -> String {
        match self {
            Self::Image(_) => String::new(),
            Self::Gif { player, .. } => format_time((player.duration_ms() / 1000) as i64),
            Self::Video(v) => format!(
                "{} / {}",
                format_time(v.clock.position(Instant::now()).min(v.duration) / 10_000_000),
                format_time(v.duration / 10_000_000)
            ),
        }
    }
}

fn format_time(seconds: i64) -> String {
    let seconds = seconds.max(0);
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

struct Preview {
    path: PathBuf,
    bytes: u64,
    settings: Rc<RefCell<Settings>>,
    player: Player,
    play: HWND,
    save: HWND,
    copy: HWND,
    slider: HWND,
    seeking: bool,
    last_label: String,
    last_theme: cap_rust::settings::Theme,
}

impl Preview {
    fn layout(&self, hwnd: HWND) -> Result<(), String> {
        let rect = common::client(hwnd);
        let bottom = rect.bottom;
        let right = rect.right;
        for (control, r) in [
            (self.play, common::rect(20, bottom - 108, 40, 34)),
            (self.save, common::rect(right - 272, bottom - 60, 80, 36)),
            (self.copy, common::rect(right - 180, bottom - 60, 160, 36)),
            (
                self.slider,
                common::rect(72, bottom - 106, (right - 258).max(1), 30),
            ),
        ] {
            if !control.is_invalid() {
                unsafe {
                    MoveWindow(
                        control,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        true,
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        common::invalidate(hwnd);
        Ok(())
    }

    fn update_position(&self) {
        if !self.seeking {
            if let Player::Video(video) = &self.player {
                let position = video.clock.position(Instant::now());
                let value = if video.duration > 0 {
                    (position as i128 * SLIDER_MAX as i128 / video.duration as i128)
                        .clamp(0, SLIDER_MAX as i128) as i64
                } else {
                    0
                };
                unsafe {
                    if SendMessageW(self.slider, TBM_GETPOS, None, None).0 != value as isize {
                        SendMessageW(
                            self.slider,
                            TBM_SETPOS,
                            Some(WPARAM(1)),
                            Some(LPARAM(value as isize)),
                        );
                    }
                }
            }
        }
    }

    fn paint(&self, hwnd: HWND) -> Result<(), String> {
        let canvas = common::Canvas::begin(hwnd);
        let rect = common::client(hwnd);
        let palette = common::palette(self.settings.borrow().theme);
        common::fill(canvas.dc, rect, palette.bg);
        let available = common::rect(
            20,
            16,
            (rect.right - 40).max(1),
            (rect.bottom
                - if matches!(self.player, Player::Image(_)) {
                    100
                } else {
                    140
                })
            .max(1),
        );
        common::rounded(canvas.dc, available, common::rgb(19, 19, 21), 16);
        let inside = RECT {
            left: available.left + 6,
            top: available.top + 6,
            right: available.right - 6,
            bottom: available.bottom - 6,
        };
        common::draw_frame(
            canvas.dc,
            self.player.frame(),
            common::fit(self.player.frame(), inside),
        )?;
        if matches!(self.player, Player::Gif { .. }) {
            common::label(
                canvas.dc,
                "GIF 动画 · 自动循环",
                common::rect(20, rect.bottom - 108, rect.right - 220, 34),
                palette.muted,
            );
        }
        common::label(
            canvas.dc,
            &self.player.time(),
            common::rect(rect.right - 180, rect.bottom - 108, 160, 34),
            palette.muted,
        );
        let metadata = if let Player::Image(frame) = &self.player {
            format!("{} × {}", frame.width, frame.height)
        } else {
            format!(
                "{}  ·  {:.2} MB",
                self.path.file_name().unwrap_or_default().to_string_lossy(),
                self.bytes as f64 / 1_048_576.0
            )
        };
        common::label(
            canvas.dc,
            &metadata,
            common::rect(20, rect.bottom - 60, (rect.right - 308).max(1), 36),
            palette.muted,
        );
        Ok(())
    }

    fn toggle(&mut self) {
        self.player.toggle();
        if !self.play.is_invalid() {
            common::set_text(
                self.play,
                if self.player.playing() {
                    "暂停"
                } else {
                    "播放"
                },
            );
            common::invalidate(self.play);
        }
    }

    fn save(&self) -> Result<(), String> {
        runtime::emit(if let Player::Image(frame) = &self.player {
            UiEvent::SaveImage(frame.copy()?)
        } else {
            UiEvent::SaveFile(self.path.clone())
        });
        Ok(())
    }

    fn copy(&self) -> Result<(), String> {
        runtime::emit(if let Player::Image(frame) = &self.player {
            UiEvent::CopyImage(frame.copy()?)
        } else {
            UiEvent::CopyFile(self.path.clone())
        });
        Ok(())
    }

    fn folder_menu(&self, hwnd: HWND, point: POINT) -> Result<(), String> {
        let menu = unsafe { CreatePopupMenu() }.map_err(|e| e.to_string())?;
        let image = matches!(self.player, Player::Image(_));
        let label = common::wide(if image {
            "复制图片"
        } else {
            "打开所在文件夹"
        });
        let result = (|| {
            unsafe {
                AppendMenuW(menu, MF_STRING, 1, PCWSTR(label.as_ptr()))
                    .map_err(|e| e.to_string())?;
                if TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    hwnd,
                    None,
                )
                .0 == 1
                {
                    if image {
                        self.copy()?;
                    } else {
                        runtime::emit(UiEvent::OpenFolder(self.path.clone()));
                    }
                }
            }
            Ok(())
        })();
        unsafe {
            DestroyMenu(menu).map_err(|e| e.to_string())?;
        }
        result
    }
}

impl WindowHandler for Preview {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        let owner_draw = WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32);
        let empty = common::rect(0, 0, 1, 1);
        self.save = common::child(hwnd, "BUTTON", "保存", SAVE, empty, owner_draw)?;
        self.copy = common::child(hwnd, "BUTTON", "复制到剪贴板", COPY, empty, owner_draw)?;
        if let Player::Gif { next, player, .. } = &mut self.player {
            *next = Instant::now() + std::time::Duration::from_millis(player.delay() as u64);
        } else if matches!(self.player, Player::Video(_)) {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            };
            if !unsafe { InitCommonControlsEx(&controls) }.as_bool() {
                return Err("初始化播放进度条失败。".into());
            }
            self.play = common::child(hwnd, "BUTTON", "暂停", PLAY, empty, owner_draw)?;
            self.slider = common::child(
                hwnd,
                "msctls_trackbar32",
                "播放位置",
                TIMELINE,
                empty,
                WS_TABSTOP | WINDOW_STYLE(TBS_NOTICKS),
            )?;
            unsafe {
                SendMessageW(
                    self.slider,
                    TBM_SETRANGEMIN,
                    Some(WPARAM(0)),
                    Some(LPARAM(0)),
                );
                SendMessageW(
                    self.slider,
                    TBM_SETRANGEMAX,
                    Some(WPARAM(1)),
                    Some(LPARAM(SLIDER_MAX as isize)),
                );
            }
            if let Player::Video(video) = &mut self.player {
                video.clock.seek(0, Instant::now());
            }
        }
        self.layout(hwnd)?;
        if unsafe { SetTimer(Some(hwnd), TIMER, 15, None) } == 0 {
            return Err(format!(
                "启动播放计时器失败：{}",
                std::io::Error::last_os_error()
            ));
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
            WM_PAINT => self.paint(hwnd)?,
            WM_SIZE => self.layout(hwnd)?,
            WM_TIMER if w.0 == TIMER => {
                if !self.seeking {
                    let changed = match self.player.tick(Instant::now()) {
                        Ok(changed) => changed,
                        Err(error) => {
                            unsafe {
                                KillTimer(Some(hwnd), TIMER).map_err(|e| e.to_string())?;
                            }
                            if self.player.playing() {
                                self.toggle();
                            }
                            return Err(error);
                        }
                    };
                    self.update_position();
                    let label = self.player.time();
                    let theme = self.settings.borrow().theme;
                    if changed || label != self.last_label || theme != self.last_theme {
                        self.last_label = label;
                        self.last_theme = theme;
                        common::invalidate(hwnd);
                        common::invalidate(self.save);
                        common::invalidate(self.copy);
                        if !self.play.is_invalid() {
                            common::invalidate(self.play);
                        }
                    }
                }
            }
            WM_COMMAND => match w.0 as u16 {
                PLAY => {
                    self.toggle();
                    common::invalidate(hwnd);
                }
                SAVE => self.save()?,
                COPY => self.copy()?,
                _ => return Ok(None),
            },
            WM_HSCROLL if l.0 == self.slider.0 as isize => {
                let command = w.0 as u16 as u32;
                self.seeking = command == TB_THUMBTRACK;
                let value = unsafe { SendMessageW(self.slider, TBM_GETPOS, None, None) }
                    .0
                    .max(0) as i64;
                if let Player::Video(video) = &mut self.player {
                    let position = (value as i128 * video.duration as i128 / SLIDER_MAX as i128)
                        .min(i64::MAX as i128) as i64;
                    video.seek(position, Instant::now())?;
                }
                common::invalidate(hwnd);
            }
            WM_DRAWITEM => {
                let item = unsafe { &*(l.0 as *const DRAWITEMSTRUCT) };
                let palette = common::palette(self.settings.borrow().theme);
                let accent = item.CtlID == SAVE as u32;
                let label = match item.CtlID as u16 {
                    SAVE => "保存",
                    COPY => "复制到剪贴板",
                    PLAY => {
                        if self.player.playing() {
                            "暂停"
                        } else {
                            "播放"
                        }
                    }
                    _ => return Ok(None),
                };
                common::fill(item.hDC, item.rcItem, palette.bg);
                common::button(
                    item.hDC,
                    label,
                    item.rcItem,
                    if accent {
                        palette.accent
                    } else {
                        palette.panel
                    },
                    if accent {
                        common::rgb(255, 255, 255)
                    } else {
                        palette.text
                    },
                );
                if item.itemState.0 & ODS_FOCUS.0 != 0 {
                    unsafe {
                        let _ = DrawFocusRect(item.hDC, &item.rcItem);
                    }
                }
                return Ok(Some(LRESULT(1)));
            }
            WM_CONTEXTMENU => {
                let (x, y) = common::point(l);
                let mut point = POINT { x, y };
                if x == -1 && y == -1 {
                    unsafe {
                        GetCursorPos(&mut point).map_err(|e| e.to_string())?;
                    }
                }
                self.folder_menu(hwnd, point)?;
            }
            WM_KEYDOWN if w.0 == 0x20 => {
                self.toggle();
                common::invalidate(hwnd);
            }
            WM_KEYDOWN if w.0 == 0x1b => runtime::close(hwnd),
            WM_GETMINMAXINFO => {
                let info = unsafe { &mut *(l.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize = POINT { x: 640, y: 420 };
            }
            WM_CLOSE => runtime::close(hwnd),
            WM_DESTROY => unsafe {
                KillTimer(Some(hwnd), TIMER).map_err(|e| e.to_string())?;
            },
            _ => return Ok(None),
        }
        Ok(Some(LRESULT(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cap-preview-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let red = Frame::blank(160, 96, [0, 0, 255, 255]).unwrap();
        let blue = Frame::blank(160, 96, [255, 0, 0, 255]).unwrap();
        let mut writer = cap_rust::media::Mp4Writer::new(&path, 160, 96, 10).unwrap();
        for index in 0..10 {
            writer
                .write(
                    if index < 5 { &red } else { &blue },
                    index * 1_000_000,
                    1_000_000,
                )
                .unwrap();
        }
        writer.finish().unwrap();
        path
    }

    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            runtime::close(self.0);
        }
    }

    #[test]
    fn native_mp4_preview_decodes_seek_pause_and_loop() {
        let path = movie();
        let mut video = Video::new(&path).unwrap();
        let now = Instant::now();
        video.clock.seek(0, now);
        assert!(video.frame.pixels[2] > 220);
        assert!(
            video
                .tick(now + std::time::Duration::from_millis(650))
                .unwrap()
        );
        assert!(video.frame.pixels[0] > 220);
        video
            .clock
            .toggle(now + std::time::Duration::from_millis(650));
        assert!(!video.tick(now + std::time::Duration::from_secs(8)).unwrap());
        assert!(video.frame.pixels[0] > 220);
        video.seek(1_000_000, now).unwrap();
        assert!(video.frame.pixels[2] > 220);
        assert!(!video.clock.playing);
        video.clock.toggle(now);
        assert!(
            video
                .tick(now + std::time::Duration::from_millis(1050))
                .unwrap()
        );
        assert!(video.frame.pixels[2] > 220);
        drop(video);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn native_image_preview_routes_actual_pixels_to_copy_and_save() {
        let frame = Frame::blank(160, 96, [50, 100, 200, 255]).unwrap();
        let window = OwnedWindow(
            show_image(frame.clone(), Rc::new(RefCell::new(Settings::default()))).unwrap(),
        );
        unsafe {
            assert!(GetDlgItem(Some(window.0), SAVE as i32).is_ok());
            assert!(GetDlgItem(Some(window.0), COPY as i32).is_ok());
            SendMessageW(window.0, WM_COMMAND, Some(WPARAM(COPY as usize)), None);
            SendMessageW(window.0, WM_COMMAND, Some(WPARAM(SAVE as usize)), None);
        }
        match runtime::next_event() {
            Some(UiEvent::CopyImage(actual)) => assert_eq!(actual, frame),
            _ => panic!("Expected image clipboard event"),
        }
        match runtime::next_event() {
            Some(UiEvent::SaveImage(actual)) => assert_eq!(actual, frame),
            _ => panic!("Expected image save event"),
        }
        assert!(runtime::next_event().is_none());
        unsafe {
            SendMessageW(window.0, WM_KEYDOWN, Some(WPARAM(0x1b)), None);
        }
        assert!(!unsafe { IsWindow(Some(window.0)) }.as_bool());
    }

    #[test]
    fn native_recording_window_has_working_pause_seek_and_file_actions() {
        let path = movie();
        let window =
            OwnedWindow(show(path.clone(), Rc::new(RefCell::new(Settings::default()))).unwrap());
        unsafe {
            let play = GetDlgItem(Some(window.0), PLAY as i32).unwrap();
            let slider = GetDlgItem(Some(window.0), TIMELINE as i32).unwrap();
            assert_eq!(common::get_text(play), "暂停");
            SendMessageW(window.0, WM_COMMAND, Some(WPARAM(PLAY as usize)), None);
            assert_eq!(common::get_text(play), "播放");
            SendMessageW(slider, TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(6500)));
            SendMessageW(
                window.0,
                WM_HSCROLL,
                Some(WPARAM(TB_THUMBPOSITION as usize)),
                Some(LPARAM(slider.0 as isize)),
            );
            SendMessageW(window.0, WM_COMMAND, Some(WPARAM(COPY as usize)), None);
            SendMessageW(window.0, WM_COMMAND, Some(WPARAM(SAVE as usize)), None);
        }
        match runtime::next_event() {
            Some(UiEvent::CopyFile(actual)) => assert_eq!(actual, path),
            Some(UiEvent::Error(error)) => panic!("{error}"),
            _ => panic!("Expected file clipboard event"),
        }
        match runtime::next_event() {
            Some(UiEvent::SaveFile(actual)) => assert_eq!(actual, path),
            _ => panic!("Expected file save event"),
        }
        assert!(runtime::next_event().is_none());
        drop(window);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn playback_clock_preserves_pause_position_seek_and_resume() {
        let now = Instant::now();
        let mut clock = PlaybackClock::new(now);
        let later = now + std::time::Duration::from_millis(1500);
        assert_eq!(clock.position(later), 15_000_000);
        clock.toggle(later);
        assert_eq!(
            clock.position(later + std::time::Duration::from_secs(20)),
            15_000_000
        );
        clock.seek(50_000_000, later);
        assert_eq!(clock.position(later), 50_000_000);
        clock.toggle(later);
        assert_eq!(
            clock.position(later + std::time::Duration::from_secs(1)),
            60_000_000
        );
    }

    #[test]
    fn displayed_time_handles_hours_and_negative_values() {
        assert_eq!(format_time(-1), "00:00");
        assert_eq!(format_time(61), "01:01");
        assert_eq!(format_time(3661), "01:01:01");
        assert_eq!(format_time(86400), "24:00:00");
    }
}
