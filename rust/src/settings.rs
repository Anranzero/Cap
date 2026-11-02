use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

mod hotkey_names;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub save_directory: PathBuf,
    pub screenshot_hotkey: String,
    pub recording_hotkey: String,
    pub gif_hotkey: String,
    pub theme: Theme,
    pub run_at_startup: bool,
    pub close_to_tray: bool,
    pub frames_per_second: u32,
    pub gif_frames_per_second: u32,
    pub gif_max_width: u32,
    pub show_cursor: bool,
    pub highlight_clicks: bool,
    pub show_coordinates: bool,
    pub relative_coordinates: bool,
    pub coordinate_font_size: u32,
    pub coordinate_color: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_directory: PathBuf::new(),
            screenshot_hotkey: "Ctrl+Alt+A".into(),
            recording_hotkey: "Ctrl+Alt+V".into(),
            gif_hotkey: "Ctrl+Alt+G".into(),
            theme: Theme::Light,
            run_at_startup: false,
            close_to_tray: true,
            frames_per_second: 60,
            gif_frames_per_second: 15,
            gif_max_width: 960,
            show_cursor: true,
            highlight_clicks: true,
            show_coordinates: true,
            relative_coordinates: false,
            coordinate_font_size: 18,
            coordinate_color: "#FFFFFF".into(),
        }
    }
}

impl Settings {
    pub fn defaults(save_directory: PathBuf) -> Self {
        Self {
            save_directory,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.save_directory.is_absolute() {
            return Err("保存目录必须是完整路径。".into());
        }
        if !matches!(self.frames_per_second, 15 | 24 | 30 | 60) {
            return Err("MP4 帧率请选择 15、24、30 或 60。".into());
        }
        if !(1..=60).contains(&self.gif_frames_per_second)
            || !(64..=7680).contains(&self.gif_max_width)
        {
            return Err("GIF 帧率范围为 1–60，宽度范围为 64–7680。".into());
        }
        if !(10..=72).contains(&self.coordinate_font_size) {
            return Err("坐标字号范围为 10–72。".into());
        }
        if self.coordinate_color.len() != 7
            || !self.coordinate_color.starts_with('#')
            || !self.coordinate_color[1..]
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("坐标颜色请填写 #RRGGBB。".into());
        }
        let screenshot = Hotkey::parse(&self.screenshot_hotkey)?;
        let recording = Hotkey::parse(&self.recording_hotkey)?;
        let gif = Hotkey::parse(&self.gif_hotkey)?;
        if screenshot == recording || screenshot == gif || recording == gif {
            return Err("三个快捷键不能重复。".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: u32,
    pub key: u32,
}

impl Hotkey {
    pub fn is_modifier_key(key: u32) -> bool {
        matches!(key, 0x10..=0x12 | 0x5b..=0x5c | 0xa0..=0xa5)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut modifiers = 0;
        let mut key = 0;
        for token in text.split('+').map(str::trim) {
            let token = token.to_ascii_uppercase();
            match token.as_str() {
                "CTRL" | "CONTROL" => modifiers |= 2,
                "ALT" => modifiers |= 1,
                "SHIFT" => modifiers |= 4,
                "WIN" | "WINDOWS" => modifiers |= 8,
                _ => {
                    if key != 0 {
                        return Err("快捷键只能有一个主按键。".into());
                    }
                    key = hotkey_names::key_code(&token)
                        .ok_or_else(|| format!("无法识别快捷键：{token}"))?;
                }
            }
        }
        if modifiers == 0 || key == 0 || Self::is_modifier_key(key) {
            return Err("快捷键需包含 Ctrl/Alt/Shift/Win 和一个普通按键。".into());
        }
        Ok(Self { modifiers, key })
    }

    pub fn display(self) -> String {
        let mut parts = Vec::new();
        for (flag, label) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
            if self.modifiers & flag != 0 {
                parts.push(label.to_string());
            }
        }
        let key = hotkey_names::key_name(self.key);
        parts.push(key);
        parts.join("+")
    }
}

pub struct SettingsStore {
    pub directory: PathBuf,
    pub default_save_directory: PathBuf,
}

impl SettingsStore {
    pub fn load(&self) -> Result<Settings, String> {
        let path = self.directory.join("settings.json");
        if !path
            .try_exists()
            .map_err(|e| format!("配置检查失败：{e}"))?
        {
            return Ok(Settings::defaults(self.default_save_directory.clone()));
        }
        self.read(&path)
    }

    pub fn read(&self, path: &Path) -> Result<Settings, String> {
        if fs::metadata(path)
            .map_err(|e| format!("配置读取失败：{e}"))?
            .len()
            > 65536
        {
            return Err("配置文件超过 64 KiB，原文件已保留。".into());
        }
        let content = fs::read(path).map_err(|e| format!("配置读取失败：{e}"))?;
        let content = content
            .strip_prefix(&[0xef, 0xbb, 0xbf])
            .unwrap_or(&content);
        let mut object: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(content).map_err(|e| format!("配置读取失败：{e}"))?;
        if !object.contains_key("SaveDirectory") {
            object.insert(
                "SaveDirectory".into(),
                serde_json::to_value(&self.default_save_directory)
                    .map_err(|e| format!("配置默认目录无效：{e}"))?,
            );
        }
        let settings: Settings = serde_json::from_value(serde_json::Value::Object(object))
            .map_err(|e| format!("配置读取失败：{e}"))?;
        settings.validate().map_err(|e| format!("配置无效：{e}"))?;
        Ok(settings)
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        settings.validate()?;
        fs::create_dir_all(&self.directory).map_err(|e| format!("创建配置目录失败：{e}"))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("系统时间错误：{e}"))?
            .as_nanos();
        let path = self.directory.join("settings.json");
        let temporary = self
            .directory
            .join(format!("settings.{}.{nonce}.tmp", std::process::id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|e| format!("创建配置临时文件失败：{e}"))?;
            let json =
                serde_json::to_vec_pretty(settings).map_err(|e| format!("配置序列化失败：{e}"))?;
            file.write_all(&json)
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("写入配置失败：{e}"))?;
            drop(file);
            if path.try_exists().map_err(|e| e.to_string())? {
                let backup = if self.read(&path).is_ok() {
                    self.directory.join("settings.json.bak")
                } else {
                    self.directory
                        .join(format!("settings.corrupt.{nonce}.json"))
                };
                let backup_temp = self.directory.join(format!("settings.backup.{nonce}.tmp"));
                fs::copy(&path, &backup_temp)
                    .map_err(|e| format!("配置备份失败，原配置未替换：{e}"))?;
                atomic_replace(&backup_temp, &backup)?;
            }
            atomic_replace(&temporary, &path)
        })();
        if temporary.exists() {
            if let Err(e) = fs::remove_file(&temporary) {
                return Err(format!(
                    "{}配置临时文件清理失败：{e}",
                    result.err().map(|e| e + "；").unwrap_or_default()
                ));
            }
        }
        result
    }
}

fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| format!("原子保存配置失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn store() -> SettingsStore {
        let directory = std::env::temp_dir().join(format!(
            "cap-rust-settings-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        SettingsStore {
            default_save_directory: directory.join("pictures"),
            directory,
        }
    }

    #[test]
    fn accepts_legacy_pascal_case_settings_and_preserves_custom_hotkeys() {
        let store = store();
        fs::create_dir_all(&store.directory).unwrap();
        let text = format!(
            r##"{{"SaveDirectory":{},"ScreenshotHotkey":"Control+Shift+F10","Theme":"Dark","CoordinateColor":"#aBcD01"}}"##,
            serde_json::to_string(&store.default_save_directory).unwrap()
        );
        fs::write(store.directory.join("settings.json"), text).unwrap();
        let settings = store.load().unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.screenshot_hotkey, "Control+Shift+F10");
        assert_eq!(settings.frames_per_second, 60);
        assert_eq!(settings.coordinate_color, "#aBcD01");
        fs::remove_dir_all(store.directory).unwrap();
    }

    #[test]
    fn saving_preserves_previous_valid_configuration_and_invalid_input_cannot_replace_it() {
        let store = store();
        let original = Settings::defaults(store.default_save_directory.clone());
        store.save(&original).unwrap();
        let mut changed = original.clone();
        changed.theme = Theme::Dark;
        store.save(&changed).unwrap();
        let previous: Settings =
            serde_json::from_slice(&fs::read(store.directory.join("settings.json.bak")).unwrap())
                .unwrap();
        assert_eq!(previous, original);
        changed.recording_hotkey = changed.screenshot_hotkey.clone();
        assert!(store.save(&changed).is_err());
        assert_eq!(store.load().unwrap().theme, Theme::Dark);
        fs::remove_dir_all(store.directory).unwrap();
    }

    #[test]
    fn malformed_configuration_is_reported_and_preserved() {
        let store = store();
        fs::create_dir_all(&store.directory).unwrap();
        fs::write(store.directory.join("settings.json"), b"{broken").unwrap();
        assert!(store.load().unwrap_err().contains("配置"));
        assert_eq!(
            fs::read(store.directory.join("settings.json")).unwrap(),
            b"{broken"
        );
        fs::remove_dir_all(store.directory).unwrap();
    }

    #[test]
    fn hotkeys_reject_multiple_keys_and_modifier_only_and_normalize_aliases() {
        assert_eq!(
            Hotkey::parse("Control+Shift+F10").unwrap(),
            Hotkey {
                modifiers: 6,
                key: 0x79
            }
        );
        assert_eq!(Hotkey::parse("Win+Alt+9").unwrap().display(), "Alt+Win+9");
        for invalid in ["", "Ctrl", "A", "Ctrl+A+B", "Ctrl+Shift", "Ctrl+Bogus"] {
            assert!(Hotkey::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn hotkeys_accept_original_wpf_names_and_roundtrip_captured_keys() {
        for (name, key) in [
            ("NumPad0", 0x60),
            ("NumPad9", 0x69),
            ("Add", 0x6b),
            ("CapsLock", 0x14),
            ("Capital", 0x14),
            ("VolumeMute", 0xad),
            ("BrowserBack", 0xa6),
            ("MediaPlayPause", 0xb3),
            ("LaunchApplication2", 0xb7),
            ("Oem102", 0xe2),
            ("OemBackslash", 0xe2),
            ("Oem8", 0xdf),
            ("KanaMode", 0x15),
            ("HangulMode", 0x15),
            ("KanjiMode", 0x19),
            ("DbeEnterImeConfigureMode", 0xf8),
            ("OemAuto", 0xf3),
            ("D0", 0x30),
            ("Cancel", 0x03),
            ("Clear", 0x0c),
            ("Apps", 0x5d),
            ("Sleep", 0x5f),
        ] {
            let expected = Hotkey { modifiers: 2, key };
            assert_eq!(
                Hotkey::parse(&format!("Ctrl+{name}")),
                Ok(expected),
                "{name}"
            );
            assert_eq!(
                Hotkey::parse(&expected.display()),
                Ok(expected),
                "display of {name}"
            );
        }
    }

    #[test]
    fn all_original_wpf_key_aliases_parse_and_every_capture_display_roundtrips() {
        // Fixture generated independently from PresentationCore's Key enum and
        // KeyInterop.VirtualKeyFromKey; excludes the original modifier-only keys.
        for line in include_str!("settings/wpf_hotkey_names.tsv").lines() {
            let (code, name) = line.split_once(' ').unwrap();
            let key = u32::from_str_radix(code, 16).unwrap();
            for modifiers in 1..=15 {
                let expected = Hotkey { modifiers, key };
                assert_eq!(
                    Hotkey::parse(&expected.display()),
                    Ok(expected),
                    "display of {name}"
                );
            }
            assert_eq!(
                Hotkey::parse(&format!("Ctrl+{}", name.to_lowercase())),
                Ok(Hotkey { modifiers: 2, key }),
                "{name}"
            );
        }
        for modifier in [
            "LeftCtrl",
            "RightCtrl",
            "LeftAlt",
            "RightAlt",
            "LeftShift",
            "RightShift",
            "LWin",
            "RWin",
        ] {
            assert!(Hotkey::parse(&format!("Ctrl+{modifier}")).is_err());
        }
    }

    #[test]
    fn store_preserves_original_wpf_hotkeys_and_all_other_legacy_settings() {
        let store = store();
        fs::create_dir_all(&store.directory).unwrap();
        for keys in [
            ["Ctrl+NumPad0", "Ctrl+Add", "Ctrl+CapsLock"],
            ["Alt+VolumeMute", "Ctrl+BrowserBack", "Shift+OemBackslash"],
            [
                "Ctrl+KanaMode",
                "Ctrl+OemAuto",
                "Ctrl+DbeEnterImeConfigureMode",
            ],
        ] {
            let expected = Settings {
                screenshot_hotkey: keys[0].into(),
                recording_hotkey: keys[1].into(),
                gif_hotkey: keys[2].into(),
                theme: Theme::Dark,
                frames_per_second: 60,
                coordinate_color: "#12AB34".into(),
                ..Settings::defaults(store.default_save_directory.clone())
            };
            fs::write(
                store.directory.join("settings.json"),
                serde_json::to_vec(&expected).unwrap(),
            )
            .unwrap();
            assert_eq!(store.load(), Ok(expected));
        }
        fs::remove_dir_all(store.directory).unwrap();
    }

    #[test]
    fn bad_numeric_color_and_duplicate_hotkey_inputs_are_rejected() {
        let store = store();
        let valid = Settings::defaults(store.default_save_directory);
        assert!(valid.validate().is_ok());
        let mut settings = valid.clone();
        settings.frames_per_second = 29;
        assert!(settings.validate().is_err());
        settings = valid.clone();
        settings.gif_frames_per_second = 0;
        assert!(settings.validate().is_err());
        settings = valid.clone();
        settings.coordinate_color = "#ZZZZZZ".into();
        assert!(settings.validate().is_err());
        settings = valid.clone();
        settings.gif_hotkey = "Control+Alt+A".into();
        assert!(settings.validate().is_err());
    }
}
