//! Native tooltip resources using the shared compact UI font.
use crate::ui::{common, fonts};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, PWSTR},
};

pub(super) struct Chrome {
    tooltip: HWND,
    texts: Vec<Vec<u16>>,
}
impl Chrome {
    pub(super) fn new(owner: HWND, entries: &[(RECT, &str)]) -> Result<Self, String> {
        let font = fonts::compact();
        let mut chrome = Self {
            tooltip: HWND::default(),
            texts: entries.iter().map(|(_, text)| common::wide(text)).collect(),
        };
        let init = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        if !unsafe { InitCommonControlsEx(&init) }.as_bool() {
            return Err("初始化工具栏提示控件失败。".into());
        }
        chrome.tooltip = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                None,
                None,
            )
        }
        .map_err(|e| format!("创建工具栏提示失败：{e}"))?;
        unsafe {
            SendMessageW(
                chrome.tooltip,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(0)),
            );
            SendMessageW(
                chrome.tooltip,
                TTM_SETMAXTIPWIDTH,
                Some(WPARAM(0)),
                Some(LPARAM(320)),
            );
            SendMessageW(
                chrome.tooltip,
                TTM_SETDELAYTIME,
                Some(WPARAM(TTDT_INITIAL as usize)),
                Some(LPARAM(350)),
            );
        }
        for (index, (rect, _)) in entries.iter().enumerate() {
            let tool = TTTOOLINFOW {
                // V2 prefix is supported with both classic and v6 controls.
                cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
                uFlags: TTF_SUBCLASS | TTF_TRANSPARENT,
                hwnd: owner,
                uId: index + 1,
                rect: *rect,
                lpszText: PWSTR(chrome.texts[index].as_mut_ptr()),
                ..Default::default()
            };
            if unsafe {
                SendMessageW(
                    chrome.tooltip,
                    TTM_ADDTOOLW,
                    Some(WPARAM(0)),
                    Some(LPARAM(&tool as *const TTTOOLINFOW as isize)),
                )
            }
            .0 == 0
            {
                return Err(format!("注册第 {} 个工具提示失败。", index + 1));
            }
        }
        Ok(chrome)
    }
    /// Mode-bar labels use Feishu's larger 22px face instead of the tooltip font.
    pub(super) fn text_large(&self, dc: HDC, text: &str, mut rect: RECT, color: u32) {
        let text: Vec<u16> = text.encode_utf16().collect();
        if text.is_empty() {
            return;
        }
        let mut text = text;
        unsafe {
            let previous = SelectObject(dc, fonts::title().into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(color));
            DrawTextW(
                dc,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            SelectObject(dc, previous);
        }
    }
}
impl Drop for Chrome {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.tooltip)).as_bool() {
                let _ = DestroyWindow(self.tooltip);
            }
        }
    }
}
pub(super) fn round_window(hwnd: HWND, radius: i32) -> Result<(), String> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect) }.map_err(|e| e.to_string())?;
    if rect.right <= 0 || rect.bottom <= 0 {
        return Ok(());
    }
    let region =
        unsafe { CreateRoundRectRgn(0, 0, rect.right + 1, rect.bottom + 1, radius, radius) };
    if region.is_invalid() {
        return Err(format!(
            "创建工具栏圆角失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = DeleteObject(region.into());
        }
        return Err(format!("应用工具栏圆角失败：{error}"));
    }
    // SetWindowRgn transfers ownership of the region to Windows.
    Ok(())
}
