use cap_rust::{
    pixels::Frame,
    settings::{Hotkey, Settings},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Storage::FileSystem::*,
        System::{Com::*, DataExchange::*, Memory::*, Ole::*, Registry::*, Threading::*},
        UI::{Controls::Dialogs::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{BOOL, PCWSTR, PWSTR},
};

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn path_wide(value: &Path) -> Vec<u16> {
    value.as_os_str().encode_wide().chain(Some(0)).collect()
}
pub fn known_folder(id: &windows::core::GUID) -> Result<PathBuf, String> {
    let value = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("读取用户目录失败：{e}"))?;
    let result = unsafe { value.to_string() }
        .map(PathBuf::from)
        .map_err(|e| e.to_string());
    unsafe {
        CoTaskMemFree(Some(value.0.cast()));
    }
    result
}
pub fn local_data() -> Result<PathBuf, String> {
    known_folder(&FOLDERID_LocalAppData)
}
pub fn pictures() -> Result<PathBuf, String> {
    known_folder(&FOLDERID_Pictures)
}

pub fn choose_folder(owner: HWND) -> Result<Option<PathBuf>, String> {
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| e.to_string())?;
    unsafe { dialog.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST) }
        .map_err(|e| e.to_string())?;
    if let Err(error) = unsafe { dialog.Show(Some(owner)) } {
        if error.code().0 == 0x800704c7u32 as i32 {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    let item = unsafe { dialog.GetResult() }.map_err(|e| e.to_string())?;
    let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|e| e.to_string())?;
    let value = unsafe { path.to_string() }
        .map(PathBuf::from)
        .map_err(|e| e.to_string());
    unsafe {
        CoTaskMemFree(Some(path.0.cast()));
    }
    value.map(Some)
}
pub fn nonce() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .map_err(|e| format!("系统时间无效：{e}"))
}

pub struct ComApartment;
impl ComApartment {
    pub fn new() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|e| format!("初始化界面 COM 失败：{e}"))?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub struct InstanceMutex(HANDLE);
impl InstanceMutex {
    pub fn acquire(name: &str) -> Result<Option<Self>, String> {
        let name = wide(name);
        let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }
            .map_err(|e| format!("创建单实例锁失败：{e}"))?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Ok(None);
        }
        Ok(Some(Self(handle)))
    }
}
impl Drop for InstanceMutex {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

pub struct Hotkeys {
    window: HWND,
    current: Vec<(i32, Hotkey)>,
}
impl Hotkeys {
    pub fn new(window: HWND) -> Self {
        Self {
            window,
            current: Vec::new(),
        }
    }
    fn clear(&mut self) {
        for (id, _) in self.current.drain(..) {
            if let Err(e) = unsafe { UnregisterHotKey(Some(self.window), id) } {
                log_error(&format!("释放快捷键失败：{e}"));
            }
        }
    }
    pub fn apply(&mut self, settings: &Settings) -> Result<(), String> {
        settings.validate()?;
        let requested = [
            (1, Hotkey::parse(&settings.screenshot_hotkey)?),
            (2, Hotkey::parse(&settings.recording_hotkey)?),
            (3, Hotkey::parse(&settings.gif_hotkey)?),
        ];
        let previous = self.current.clone();
        self.clear();
        for (id, key) in requested {
            if let Err(e) = unsafe {
                RegisterHotKey(
                    Some(self.window),
                    id,
                    HOT_KEY_MODIFIERS(key.modifiers) | MOD_NOREPEAT,
                    key.key,
                )
            } {
                self.clear();
                let mut failures = Vec::new();
                for (id, old) in previous {
                    match unsafe {
                        RegisterHotKey(
                            Some(self.window),
                            id,
                            HOT_KEY_MODIFIERS(old.modifiers) | MOD_NOREPEAT,
                            old.key,
                        )
                    } {
                        Ok(()) => self.current.push((id, old)),
                        Err(e) => failures.push(e.to_string()),
                    }
                }
                return Err(format!(
                    "快捷键 {} 已被占用或不可用：{e}{}",
                    key.display(),
                    if failures.is_empty() {
                        String::new()
                    } else {
                        format!("；原快捷键未能全部恢复：{}", failures.join("；"))
                    }
                ));
            }
            self.current.push((id, key));
        }
        Ok(())
    }
}
impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.clear()
    }
}
pub fn held_modifiers() -> u32 {
    unsafe {
        (u32::from(GetKeyState(VK_CONTROL.0 as i32) < 0) * 2)
            | u32::from(GetKeyState(VK_MENU.0 as i32) < 0)
            | (u32::from(GetKeyState(VK_SHIFT.0 as i32) < 0) * 4)
            | (u32::from(GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0)
                * 8)
    }
}

pub fn startup_value() -> Result<Option<String>, String> {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("cap");
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok().map_err(|e| e.to_string())?;
    let mut value = vec![0u16; (size as usize).div_ceil(2)];
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(value.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    let length = value.iter().position(|&v| v == 0).unwrap_or(value.len());
    Ok(Some(String::from_utf16_lossy(&value[..length])))
}
pub fn set_startup(enabled: bool) -> Result<(), String> {
    // Resolve the command before opening a registry handle so an early path
    // error cannot leak that handle.
    let command = if enabled {
        Some(wide(&format!(
            "\"{}\" --startup",
            std::env::current_exe()
                .map_err(|e| e.to_string())?
                .display()
        )))
    } else {
        None
    };
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("cap");
    let mut handle = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut handle,
            None,
        )
    }
    .ok()
    .map_err(|e| format!("打开自启动设置失败：{e}"))?;
    let result = if let Some(command) = command {
        let bytes =
            unsafe { std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), command.len() * 2) };
        unsafe { RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) }
            .ok()
            .map_err(|e| e.to_string())
    } else {
        let status = unsafe { RegDeleteValueW(handle, PCWSTR(name.as_ptr())) };
        if status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            status.ok().map_err(|e| e.to_string())
        }
    };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    result
}
pub fn restore_startup(value: Option<&str>) -> Result<(), String> {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("cap");
    let mut handle = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut handle,
            None,
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    let status = if let Some(value) = value {
        let value = wide(value);
        let bytes =
            unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
        unsafe { RegSetValueExW(handle, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) }
    } else {
        unsafe { RegDeleteValueW(handle, PCWSTR(name.as_ptr())) }
    };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    if status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        status.ok().map_err(|e| e.to_string())
    }
}

struct Clipboard;
impl Clipboard {
    fn open(owner: HWND) -> Result<Self, String> {
        let mut error = String::new();
        for attempt in 0..4 {
            match unsafe { OpenClipboard(Some(owner)) } {
                Ok(()) => return Ok(Self),
                Err(e) => error = e.to_string(),
            }
            if attempt < 3 {
                thread::sleep(Duration::from_millis(80));
            }
        }
        Err(format!("剪贴板暂时不可用，未复制：{error}"))
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        if let Err(e) = unsafe { CloseClipboard() } {
            log_error(&format!("关闭剪贴板失败：{e}"));
        }
    }
}
struct Memory {
    handle: HGLOBAL,
    transferred: bool,
}
impl Memory {
    fn new(bytes: &[u8]) -> Result<Self, String> {
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
            .map_err(|e| format!("分配剪贴板内存失败：{e}"))?;
        let mut memory = Self {
            handle,
            transferred: false,
        };
        let data = unsafe { GlobalLock(memory.handle) };
        if data.is_null() {
            return Err(format!(
                "锁定剪贴板内存失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len());
            let _ = GlobalUnlock(memory.handle);
        }
        memory.transferred = false;
        Ok(memory)
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        if !self.transferred {
            unsafe {
                let _ = GlobalFree(Some(self.handle));
            }
        }
    }
}
fn set_clipboard(owner: HWND, format: u32, bytes: &[u8]) -> Result<(), String> {
    let mut memory = Memory::new(bytes)?;
    let _clipboard = Clipboard::open(owner)?;
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    unsafe { SetClipboardData(format, Some(HANDLE(memory.handle.0))) }
        .map_err(|e| format!("复制失败：{e}"))?;
    memory.transferred = true;
    Ok(())
}
pub fn copy_text(owner: HWND, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("未识别到文字，请重新框选清晰区域。".into());
    }
    let text = wide(text);
    let bytes = unsafe { std::slice::from_raw_parts(text.as_ptr().cast::<u8>(), text.len() * 2) };
    set_clipboard(owner, CF_UNICODETEXT.0 as u32, bytes)
}
pub fn copy_image(owner: HWND, frame: &Frame) -> Result<(), String> {
    frame.validate()?;
    let header = BITMAPV5HEADER {
        bV5Size: std::mem::size_of::<BITMAPV5HEADER>() as u32,
        bV5Width: frame.width as i32,
        bV5Height: -(frame.height as i32),
        bV5Planes: 1,
        bV5BitCount: 32,
        bV5Compression: BI_BITFIELDS,
        bV5RedMask: 0x00ff0000,
        bV5GreenMask: 0x0000ff00,
        bV5BlueMask: 0x000000ff,
        bV5AlphaMask: 0xff000000,
        bV5CSType: 0x73524742,
        ..Default::default()
    };
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const BITMAPV5HEADER).cast::<u8>(),
            std::mem::size_of_val(&header),
        )
    };
    let mut dib = Vec::with_capacity(bytes.len() + frame.pixels.len());
    dib.extend_from_slice(bytes);
    dib.extend_from_slice(&frame.pixels);
    set_clipboard(owner, CF_DIBV5.0 as u32, &dib)
}
pub fn copy_file(owner: HWND, path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("录制文件不存在，未复制。".into());
    }
    let absolute = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let mut name = path_wide(&absolute);
    name.push(0);
    let header = DROPFILES {
        pFiles: std::mem::size_of::<DROPFILES>() as u32,
        fWide: BOOL(1),
        ..Default::default()
    };
    let mut bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const DROPFILES).cast::<u8>(),
            std::mem::size_of_val(&header),
        )
    }
    .to_vec();
    let names = unsafe { std::slice::from_raw_parts(name.as_ptr().cast::<u8>(), name.len() * 2) };
    bytes.extend_from_slice(names);
    set_clipboard(owner, CF_HDROP.0 as u32, &bytes)
}

pub fn file_dialog(
    owner: HWND,
    directory: &Path,
    save: bool,
    filter: &str,
    default_name: &str,
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    let mut path = vec![0u16; 32768];
    let name: Vec<u16> = default_name.encode_utf16().take(32766).collect();
    path[..name.len()].copy_from_slice(&name);
    let filter: Vec<u16> = filter.encode_utf16().chain([0, 0]).collect();
    let directory = path_wide(directory);
    let extension = wide(extension);
    let mut data = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        lpstrInitialDir: PCWSTR(directory.as_ptr()),
        lpstrDefExt: PCWSTR(extension.as_ptr()),
        Flags: OFN_EXPLORER
            | OFN_PATHMUSTEXIST
            | OFN_NOCHANGEDIR
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    let result = unsafe {
        if save {
            GetSaveFileNameW(&mut data)
        } else {
            GetOpenFileNameW(&mut data)
        }
    };
    if !result.as_bool() {
        let code = unsafe { CommDlgExtendedError() };
        if code.0 == 0 {
            return Ok(None);
        }
        return Err(format!("文件选择失败：{}", code.0));
    }
    let count = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &path[..count],
    ))))
}
pub fn save_image(owner: HWND, directory: &Path, frame: &Frame) -> Result<bool, String> {
    let name = format!("cap-{}.png", nonce()?);
    let Some(path) = file_dialog(
        owner,
        directory,
        true,
        "PNG 图片\0*.png\0JPEG 图片\0*.jpg;*.jpeg\0",
        &name,
        "png",
    )?
    else {
        return Ok(false);
    };
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
    let parent = path.parent().ok_or("保存目录无效。")?;
    let temp = parent.join(format!(".cap-save-{}.{extension}", nonce()?));
    cap_rust::media::save_image(&temp, frame)?;
    let source = path_wide(&temp);
    let target = path_wide(&path);
    let result = unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| format!("保存失败，原文件已保留：{e}"));
    if result.is_err() {
        if let Err(e) = fs::remove_file(&temp) {
            log_error(&format!("清理截图临时文件失败：{e}"));
        }
    }
    result.map(|_| true)
}
pub fn save_file(owner: HWND, source: &Path) -> Result<bool, String> {
    let directory = source.parent().ok_or("文件目录无效。")?;
    let name = source.file_name().ok_or("文件名无效。")?.to_string_lossy();
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let filter = format!("录制文件\0*.{extension}\0");
    let Some(target) = file_dialog(owner, directory, true, &filter, &name, extension)? else {
        return Ok(false);
    };
    if target == source {
        return Ok(true);
    }
    let temporary = target.with_file_name(format!(".cap-file-save-{}.tmp", nonce()?));
    fs::copy(source, &temporary).map_err(|e| format!("复制文件失败：{e}"))?;
    let from = path_wide(&temporary);
    let to = path_wide(&target);
    match unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } {
        Ok(()) => Ok(true),
        Err(e) => {
            if let Err(cleanup) = fs::remove_file(&temporary) {
                log_error(&format!("清理录制副本失败：{cleanup}"));
            }
            Err(format!("保存录制副本失败：{e}"))
        }
    }
}
pub fn open_folder(owner: HWND, path: &Path) -> Result<(), String> {
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().ok_or("文件目录无效。")?
    };
    let folder = path_wide(folder);
    let open = wide("open");
    let result = unsafe {
        ShellExecuteW(
            Some(owner),
            PCWSTR(open.as_ptr()),
            PCWSTR(folder.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("无法打开所在目录。".into());
    }
    Ok(())
}
pub fn log_error(message: &str) {
    let result = (|| -> Result<(), String> {
        let directory = local_data()?.join("cap-rust/logs");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("cap.log");
        if path
            .metadata()
            .map(|m| m.len() > 1024 * 1024)
            .unwrap_or(false)
        {
            let previous = directory.join("cap.previous.log");
            if previous.exists() {
                fs::remove_file(&previous).map_err(|e| e.to_string())?;
            }
            fs::rename(&path, &previous).map_err(|e| e.to_string())?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{} {message}", nonce()?).map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        eprintln!("{message}; 日志写入失败：{error}");
    }
}
