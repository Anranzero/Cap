pub mod common;
mod fonts;
pub mod gif_player;
pub mod magnifier;
pub mod ocr_card;
pub mod options_bar;
pub mod pin;
pub mod preview;
pub mod record_panel;
pub mod record_setup;
pub mod record_ui;
pub mod runtime;
pub mod scroll_start;
pub mod scrolling;
pub mod selector;
pub mod settings;
pub mod toolbar;

use cap_rust::{
    geometry::Rect,
    pixels::Frame,
    recording::{RecordingConfig, RecordingFormat},
    selection::CaptureMode,
    settings::{Settings, Theme},
};
use std::path::PathBuf;

pub enum UiEvent {
    Tick,
    Hotkey(i32),
    Tray(isize),
    ShellRestart,
    Begin(CaptureMode, RecordingFormat),
    CopyImage(Frame),
    SaveImage(Frame),
    Pin(Frame),
    Recognize(Frame),
    BeginScroll(Rect),
    StartRecording(RecordingConfig),
    RecordRegion(Rect, RecordingFormat),
    PickRecordingRegion(RecordingConfig),
    SelectionClosed,
    ConfigureRecording,
    PauseRecording,
    StopRecording,
    SettingsSave(Settings),
    PreviewTheme(Theme),
    Recover(PathBuf),
    PreviewImage(Frame),
    CopyFile(PathBuf),
    SaveFile(PathBuf),
    OpenFolder(PathBuf),
    ShowSettings,
    Error(String),
    Quit,
}
