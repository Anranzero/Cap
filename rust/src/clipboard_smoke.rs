//! Explicit child-process checks. This module must run before GUI or COM setup.
use crate::platform;
use cap_rust::pixels::Frame;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{DataExchange::*, Memory::*, Ole::*, StationsAndDesktops::*, Threading::*},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

/// Run only from the dedicated `--clipboard-self-test` process entry point.
/// The process remains on its private window station until it exits.
pub fn run(directory: &Path) -> Result<(), String> {
    if cfg!(test)
        || !std::env::args_os()
            .skip(1)
            .any(|argument| argument == "--clipboard-self-test")
    {
        return Err(
            "Clipboard checks require a dedicated --clipboard-self-test child process.".into(),
        );
    }
    if directory.exists()
        && fs::read_dir(directory)
            .map_err(|e| format!("Cannot inspect clipboard output directory: {e}"))?
            .next()
            .transpose()
            .map_err(|e| format!("Cannot inspect clipboard output directory entry: {e}"))?
            .is_some()
    {
        return Err("Clipboard output directory must be empty to preserve existing files.".into());
    }
    fs::create_dir_all(directory)
        .map_err(|e| format!("Cannot create clipboard output directory: {e}"))?;
    let station_name = format!(
        "CapClipboardSelfTest-{}-{}",
        std::process::id(),
        platform::nonce()?
    );
    let mut checks = Vec::new();
    let result = exercise(directory, &station_name, &mut checks);
    let report = json!({
        "passed": result.is_ok(),
        "isolation": "unique private window station and desktop",
        "window_station": station_name,
        "desktop": "CapClipboardSelfTest",
        "user_clipboard": "untouched",
        "checks": checks,
        "error": result.as_ref().err(),
    });
    let write_result = (|| -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("clipboard-result.json"))
            .map_err(|e| format!("Cannot create clipboard report: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("Cannot write clipboard report: {e}"))
    })();
    finish(result, write_result)
}

// Do not restore the interactive station. Windows releases these handles at
// child-process exit, including failures after switching only the station.
struct PrivateDesktop {
    station: HWINSTA,
    desktop: HDESK,
    station_name: String,
}
impl PrivateDesktop {
    fn create(name: &str) -> Result<Self, String> {
        let wide_name = platform::wide(name);
        let station = unsafe {
            CreateWindowStationW(
                PCWSTR(wide_name.as_ptr()),
                CWF_CREATE_ONLY,
                (WINSTA_CREATEDESKTOP
                    | WINSTA_ACCESSCLIPBOARD
                    | WINSTA_READATTRIBUTES
                    | WINSTA_ACCESSGLOBALATOMS
                    | WINSTA_ENUMDESKTOPS) as u32,
                None,
            )
        }
        .map_err(|e| {
            format!("Cannot create a private clipboard window station; no interactive clipboard fallback is permitted: {e}")
        })?;
        unsafe { SetProcessWindowStation(station) }.map_err(|e| {
            format!("Cannot enter the private clipboard window station; clipboard checks were not run: {e}")
        })?;
        let desktop = unsafe {
            CreateDesktopW(
                w!("CapClipboardSelfTest"),
                PCWSTR::null(),
                None,
                DESKTOP_CONTROL_FLAGS::default(),
                DESKTOP_CREATEWINDOW.0 | DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0,
                None,
            )
        }
        .map_err(|e| format!("Cannot create a private clipboard desktop: {e}"))?;
        unsafe { SetThreadDesktop(desktop) }.map_err(|e| {
            format!("Cannot enter the private clipboard desktop before GUI setup: {e}")
        })?;
        let isolation = Self {
            station,
            desktop,
            station_name: name.into(),
        };
        isolation.verify()?;
        Ok(isolation)
    }

    fn verify(&self) -> Result<(), String> {
        let station = unsafe { GetProcessWindowStation() }
            .map_err(|e| format!("Cannot verify clipboard window station: {e}"))?;
        let desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) }
            .map_err(|e| format!("Cannot verify clipboard desktop: {e}"))?;
        if station != self.station
            || desktop != self.desktop
            || object_name(HANDLE(station.0))? != self.station_name
            || object_name(HANDLE(desktop.0))? != "CapClipboardSelfTest"
            || self.station_name.eq_ignore_ascii_case("WinSta0")
        {
            return Err(
                "Clipboard isolation verification failed; clipboard access was refused.".into(),
            );
        }
        Ok(())
    }

    fn read<T>(
        &self,
        owner: HWND,
        consume: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.verify()?;
        unsafe { OpenClipboard(Some(owner)) }
            .map_err(|e| format!("Cannot open the isolated clipboard consumer: {e}"))?;
        let result = consume();
        let close = unsafe { CloseClipboard() }
            .map_err(|e| format!("Cannot close the isolated clipboard consumer: {e}"));
        finish(result, close)
    }
}

fn object_name(handle: HANDLE) -> Result<String, String> {
    let mut name = [0u16; 256];
    unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            std::mem::size_of_val(&name) as u32,
            None,
        )
    }
    .map_err(|e| format!("Cannot read private clipboard object name: {e}"))?;
    let count = name
        .iter()
        .position(|&value| value == 0)
        .ok_or("Private clipboard object name is not terminated.")?;
    String::from_utf16(&name[..count]).map_err(|e| e.to_string())
}

fn exercise(directory: &Path, station_name: &str, checks: &mut Vec<Value>) -> Result<(), String> {
    let isolation = PrivateDesktop::create(station_name)?;
    // A hidden built-in window supplies a real owner for EmptyClipboard.
    let owner = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Cap isolated clipboard owner"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
    }
    .map_err(|e| format!("Cannot create the hidden private clipboard owner: {e}"))?;
    checks.push(json!({"check":"private_window_station_and_desktop", "passed":true}));
    let result = (|| -> Result<(), String> {
        let frame = Frame::new(
            3,
            2,
            vec![
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 20, 80, 140, 255, 240, 160, 30,
                255, 255, 255, 255, 255,
            ],
        )?;
        isolation.verify()?;
        platform::copy_image(owner, &frame)?;
        let image = isolation.read(owner, || consume_bitmap(&frame))?;
        checks.push(image);

        let text = "Cap 剪贴板验收\r\nUnicode: 中文 😀";
        isolation.verify()?;
        platform::copy_text(owner, text)?;
        let actual = isolation.read(owner, consume_text)?;
        if actual != text {
            return Err("Windows CF_UNICODETEXT consumer returned different text.".into());
        }
        checks.push(json!({"check":"CF_UNICODETEXT_GlobalLock", "passed":true, "text":actual}));

        let source = directory.join("clipboard-文件.txt");
        let contents = "Cap isolated clipboard file fixture\r\n中文 😀\r\n";
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&source)
            .and_then(|mut file| file.write_all(contents.as_bytes()))
            .map_err(|e| format!("Cannot write clipboard file fixture: {e}"))?;
        isolation.verify()?;
        platform::copy_file(owner, &source)?;
        let actual = isolation.read(owner, consume_file)?;
        let expected = fs::canonicalize(&source).map_err(|e| e.to_string())?;
        if actual != expected
            || fs::read(&actual).map_err(|e| e.to_string())? != contents.as_bytes()
        {
            return Err(
                "Windows CF_HDROP consumer returned a different file path or content.".into(),
            );
        }
        checks.push(json!({"check":"CF_HDROP_DragQueryFileW", "passed":true, "file":actual}));
        Ok(())
    })();
    let clear = isolation.read(owner, || {
        unsafe { EmptyClipboard() }
            .map_err(|e| format!("Cannot clear the private test clipboard: {e}"))
    });
    let destroy = unsafe { DestroyWindow(owner) }
        .map_err(|e| format!("Cannot destroy the private clipboard owner: {e}"));
    finish(finish(result, clear), destroy)
}

fn consume_bitmap(expected: &Frame) -> Result<Value, String> {
    let handle = unsafe { GetClipboardData(CF_BITMAP.0 as u32) }.map_err(|e| {
        format!("Windows could not synthesize CF_BITMAP from the copied image: {e}")
    })?;
    let bitmap = HBITMAP(handle.0);
    let mut object = BITMAP::default();
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut object as *mut BITMAP).cast()),
        )
    };
    if copied != std::mem::size_of::<BITMAP>() as i32
        || object.bmWidth != expected.width as i32
        || object.bmHeight != expected.height as i32
    {
        return Err(format!(
            "Windows CF_BITMAP has incorrect dimensions: {}x{}.",
            object.bmWidth, object.bmHeight
        ));
    }
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return Err(format!(
            "Cannot create the CF_BITMAP consumer DC: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: expected.width as i32,
            biHeight: -(expected.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; expected.pixels.len()];
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            expected.height,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    let result = if rows != expected.height as i32 {
        Err(format!(
            "Windows CF_BITMAP consumer returned {rows} pixel rows."
        ))
    } else if pixels
        .chunks_exact(4)
        .zip(expected.pixels.chunks_exact(4))
        .any(|(actual, wanted)| actual[..3] != wanted[..3])
    {
        Err("Windows CF_BITMAP consumer returned different RGB pixels or row order.".into())
    } else {
        Ok(json!({
            "check":"CF_BITMAP_GetObjectW_GetDIBits", "passed":true,
            "width":expected.width, "height":expected.height,
            "rgb_pixels_checked":expected.width * expected.height,
            "alpha":"CF_BITMAP does not guarantee alpha; RGB channels were compared",
        }))
    };
    let delete = unsafe { DeleteDC(dc) }
        .ok()
        .map_err(|e| format!("Cannot release the CF_BITMAP consumer DC: {e}"));
    // The clipboard owns the synthesized HBITMAP; never delete that object.
    finish(result, delete)
}

fn consume_text() -> Result<String, String> {
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) }
        .map_err(|e| format!("Cannot read Windows CF_UNICODETEXT: {e}"))?;
    let memory = HGLOBAL(handle.0);
    let bytes = unsafe { GlobalSize(memory) };
    if bytes < 2 || bytes % 2 != 0 {
        return Err("Windows CF_UNICODETEXT has an invalid memory size.".into());
    }
    let pointer = unsafe { GlobalLock(memory) };
    if pointer.is_null() {
        return Err(format!(
            "Cannot lock Windows CF_UNICODETEXT: {}",
            std::io::Error::last_os_error()
        ));
    }
    let result = (|| -> Result<String, String> {
        let text = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), bytes / 2) };
        let count = text
            .iter()
            .position(|&value| value == 0)
            .ok_or("Windows CF_UNICODETEXT is not terminated.")?;
        String::from_utf16(&text[..count])
            .map_err(|e| format!("Windows CF_UNICODETEXT is invalid UTF-16: {e}"))
    })();
    unsafe { SetLastError(ERROR_SUCCESS) };
    let unlock = unsafe { GlobalUnlock(memory) };
    // A zero lock count is a successful unlock with a false return value.
    let unlock = match unlock {
        Err(error) if unsafe { GetLastError() } != ERROR_SUCCESS => {
            Err(format!("Cannot unlock Windows CF_UNICODETEXT: {error}"))
        }
        _ => Ok(()),
    };
    finish(result, unlock)
}

fn consume_file() -> Result<PathBuf, String> {
    let handle = unsafe { GetClipboardData(CF_HDROP.0 as u32) }
        .map_err(|e| format!("Cannot read Windows CF_HDROP: {e}"))?;
    let drop = HDROP(handle.0);
    if unsafe { DragQueryFileW(drop, u32::MAX, None) } != 1 {
        return Err("Windows CF_HDROP did not contain exactly one file.".into());
    }
    let count = unsafe { DragQueryFileW(drop, 0, None) };
    if count == 0 {
        return Err("Windows CF_HDROP contained an empty file path.".into());
    }
    let mut path = vec![0u16; count as usize + 1];
    if unsafe { DragQueryFileW(drop, 0, Some(&mut path)) } != count || path[count as usize] != 0 {
        return Err("Windows CF_HDROP file path could not be read completely.".into());
    }
    Ok(PathBuf::from(OsString::from_wide(&path[..count as usize])))
}

fn finish<T>(result: Result<T, String>, cleanup: Result<(), String>) -> Result<T, String> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}; cleanup also failed: {cleanup}")),
    }
}
