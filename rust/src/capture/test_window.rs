//! Owned, message-pumped windows used only by native acceptance tests.
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR, w},
};
static SERIAL: AtomicU32 = AtomicU32::new(0);
pub(crate) struct TestWindow {
    pub handle: isize,
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_PAINT {
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(window, &mut paint);
            let mut client = RECT::default();
            if GetClientRect(window, &mut client).is_ok() {
                fill_test_rect(dc, client, COLORREF(0x002040e0));
                fill_test_rect(
                    dc,
                    RECT {
                        left: (client.right - 40).max(0),
                        top: 0,
                        right: client.right,
                        bottom: client.bottom,
                    },
                    COLORREF(0x00c8281e),
                );
                fill_test_rect(
                    dc,
                    RECT {
                        left: 0,
                        top: (client.bottom - 32).max(0),
                        right: client.right,
                        bottom: client.bottom,
                    },
                    COLORREF(0x0014b428),
                );
                fill_test_rect(
                    dc,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 16.min(client.right),
                        bottom: 16.min(client.bottom),
                    },
                    COLORREF(0x00ffffff),
                );
            }
            let _ = EndPaint(window, &paint);
        }
        LRESULT(0)
    } else if message == WM_SIZE {
        unsafe {
            let _ = InvalidateRect(Some(window), None, true);
        }
        LRESULT(0)
    } else if message == WM_DESTROY {
        LRESULT(0)
    } else {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
}
unsafe fn fill_test_rect(dc: HDC, rect: RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        if !brush.is_invalid() {
            FillRect(dc, &rect, brush);
            let _ = DeleteObject(brush.into());
        }
    }
}
impl TestWindow {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _dpi = super::DpiScope::new();
            let class = format!(
                "CapRustBackendTest{}\0",
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .collect::<Vec<_>>();
            unsafe {
                let instance = match GetModuleHandleW(None) {
                    Ok(h) => HINSTANCE(h.0),
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let brush = CreateSolidBrush(COLORREF(0x002040e0));
                let atom = RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance,
                    hbrBackground: brush,
                    lpszClassName: PCWSTR(class.as_ptr()),
                    ..Default::default()
                });
                if atom == 0 {
                    let _ = DeleteObject(brush.into());
                    let _ = tx.send(Err(super::last_error("Registering test window")));
                    return;
                }
                let window = match CreateWindowExW(
                    WS_EX_TOPMOST,
                    PCWSTR(class.as_ptr()),
                    w!("Cap Rust owned acceptance window"),
                    WS_POPUP | WS_VISIBLE,
                    x,
                    y,
                    width,
                    height,
                    None,
                    None,
                    Some(instance),
                    None,
                ) {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = UnregisterClassW(PCWSTR(class.as_ptr()), Some(instance));
                        let _ = DeleteObject(brush.into());
                        let _ = tx.send(Err(e.to_string()));
                        return;
                    }
                };
                assert!(UpdateWindow(window).as_bool());
                assert!(GdiFlush().as_bool());
                windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
                if tx
                    .send(Ok((
                        window.0 as isize,
                        windows::Win32::System::Threading::GetCurrentThreadId(),
                    )))
                    .is_err()
                {
                    let _ = DestroyWindow(window);
                } else {
                    let mut message = MSG::default();
                    loop {
                        let result = GetMessageW(&mut message, None, 0, 0).0;
                        if result <= 0 {
                            if result < 0 {
                                eprintln!(
                                    "Acceptance window message pump failed: {}",
                                    std::io::Error::last_os_error()
                                )
                            }
                            break;
                        }
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                let _ = UnregisterClassW(PCWSTR(class.as_ptr()), Some(instance));
                let _ = DeleteObject(brush.into());
            }
        });
        let (handle, thread_id) = rx.recv().map_err(|e| e.to_string())??;
        Ok(Self {
            handle,
            thread_id,
            worker: Some(worker),
        })
    }
    pub fn move_resize(&self, x: i32, y: i32, width: i32, height: i32) -> Result<(), String> {
        unsafe {
            SetWindowPos(
                HWND(self.handle as *mut _),
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|e| e.to_string())?;
            if !UpdateWindow(HWND(self.handle as *mut _)).as_bool() {
                return Err(windows::core::Error::from_win32().to_string());
            }
            windows::Win32::Graphics::Dwm::DwmFlush().map_err(|e| e.to_string())?;
            Ok(())
        }
    }
    pub fn destroy(&self) {
        if super::is_window(self.handle) {
            unsafe {
                PostMessageW(
                    Some(HWND(self.handle as *mut _)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
                .unwrap();
            }
            let start = std::time::Instant::now();
            while super::is_window(self.handle)
                && start.elapsed() < std::time::Duration::from_secs(2)
            {
                thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(!super::is_window(self.handle));
        }
    }
    pub fn close(&mut self) {
        if self.worker.is_none() {
            return;
        }
        self.destroy();
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)).unwrap();
        }
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
impl Drop for TestWindow {
    fn drop(&mut self) {
        self.close()
    }
}
