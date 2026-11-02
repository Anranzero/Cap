use crate::{
    platform,
    ui::{UiEvent, common, runtime},
};
use cap_rust::{recording::RecordingFormat, selection::CaptureMode};
use windows::{
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::PCWSTR,
};
pub const MESSAGE: u32 = WM_APP + 1;
pub struct Tray {
    data: NOTIFYICONDATAW,
}
impl Tray {
    pub fn new(window: HWND) -> Result<Self, String> {
        let instance = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        let icon = unsafe {
            LoadIconW(
                Some(HINSTANCE(instance.0)),
                PCWSTR(std::ptr::without_provenance(1)),
            )
        }
        .map_err(|e| e.to_string())?;
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: MESSAGE,
            hIcon: icon,
            ..Default::default()
        };
        copy(&mut data.szTip, "cap · Rust · 截图 / 录屏");
        if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            return Err(format!(
                "创建系统托盘失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { data })
    }
    pub fn restore(&self) -> Result<(), String> {
        unsafe { SetLastError(ERROR_SUCCESS) };
        if unsafe { Shell_NotifyIconW(NIM_ADD, &self.data) }.as_bool() {
            return Ok(());
        }
        let add_error = unsafe { GetLastError() }.0;
        unsafe { SetLastError(ERROR_SUCCESS) };
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.data) }.as_bool() {
            return Ok(());
        }
        let modify_error = unsafe { GetLastError() }.0;
        Err(format!(
            "恢复系统托盘失败：添加图标返回失败（GetLastError={add_error}）；更新图标返回失败（GetLastError={modify_error}）。"
        ))
    }
    pub fn notify(&self, title: &str, body: &str) {
        let mut data = self.data;
        data.uFlags = NIF_INFO;
        copy(&mut data.szInfoTitle, title);
        copy(&mut data.szInfo, body);
        data.dwInfoFlags = NIIF_INFO;
        if !unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
            platform::log_error("托盘通知未显示。");
        }
    }
    pub fn menu(&self) -> Result<(), String> {
        let menu = unsafe { CreatePopupMenu() }.map_err(|e| e.to_string())?;
        let result = (|| {
            for (id, label) in [(1, "截屏"), (2, "录制"), (3, "设置"), (4, "退出")] {
                let label = common::wide(label);
                unsafe { AppendMenuW(menu, MF_STRING, id, PCWSTR(label.as_ptr())) }
                    .map_err(|e| e.to_string())?;
            }
            let mut cursor = POINT::default();
            unsafe { GetCursorPos(&mut cursor) }.map_err(|e| e.to_string())?;
            unsafe {
                let _ = SetForegroundWindow(self.data.hWnd);
            }
            let command = unsafe {
                TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                    cursor.x,
                    cursor.y,
                    None,
                    self.data.hWnd,
                    None,
                )
            }
            .0;
            match command {
                1 => runtime::emit(UiEvent::Begin(
                    CaptureMode::Screenshot,
                    RecordingFormat::Mp4,
                )),
                2 => runtime::emit(UiEvent::Begin(CaptureMode::Recording, RecordingFormat::Mp4)),
                3 => runtime::emit(UiEvent::ShowSettings),
                4 => runtime::emit(UiEvent::Quit),
                _ => {}
            }
            unsafe {
                let _ = PostMessageW(Some(self.data.hWnd), WM_NULL, WPARAM(0), LPARAM(0));
            }
            Ok(())
        })();
        unsafe {
            let _ = DestroyMenu(menu);
        }
        result
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        if !unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data) }.as_bool() {
            platform::log_error("清理系统托盘图标失败。");
        }
    }
}
fn copy(buffer: &mut [u16], value: &str) {
    buffer.fill(0);
    let limit = buffer.len().saturating_sub(1);
    for (dst, ch) in buffer.iter_mut().take(limit).zip(value.encode_utf16()) {
        *dst = ch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_restore_readds_a_removed_owned_icon_and_tolerates_existing_icon() {
        struct OwnedWindow(HWND);
        impl Drop for OwnedWindow {
            fn drop(&mut self) {
                runtime::close(self.0);
            }
        }
        let window = OwnedWindow(
            runtime::create(
                Box::new(runtime::RootHandler),
                runtime::WindowOptions {
                    title: "Cap owned tray recovery test".into(),
                    rect: common::rect(0, 0, 1, 1),
                    style: WS_POPUP,
                    ex_style: WS_EX_TOOLWINDOW,
                    owner: None,
                    visible: false,
                },
            )
            .unwrap(),
        );
        let tray = Tray {
            data: NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window.0,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: MESSAGE,
                hIcon: unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap(),
                ..Default::default()
            },
        };
        tray.restore().unwrap();
        tray.restore().unwrap();
        assert!(unsafe { Shell_NotifyIconW(NIM_DELETE, &tray.data) }.as_bool());
        tray.restore().unwrap();
        // Dropping the tray removes only this test window's restored icon.
        drop(tray);
    }
}
