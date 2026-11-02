use super::{UiEvent, common};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::UpdateWindow, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::PCWSTR,
};

pub trait WindowHandler {
    fn created(&mut self, _hwnd: HWND) -> Result<(), String> {
        Ok(())
    }
    fn message(
        &mut self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<Option<LRESULT>, String>;
}

pub struct WindowOptions {
    pub title: String,
    pub rect: RECT,
    pub style: WINDOW_STYLE,
    pub ex_style: WINDOW_EX_STYLE,
    pub owner: Option<HWND>,
    pub visible: bool,
}

type Handler = Rc<RefCell<Box<dyn WindowHandler>>>;
thread_local! {
    static WINDOWS: RefCell<HashMap<usize,Handler>>=RefCell::new(HashMap::new());
    static EVENTS: RefCell<VecDeque<UiEvent>>=const {RefCell::new(VecDeque::new())};
    static CLASS_READY: Cell<bool>=const {Cell::new(false)};
    static TASKBAR_CREATED: Cell<u32>=const {Cell::new(0)};
}

pub fn emit(event: UiEvent) {
    EVENTS.with(|q| q.borrow_mut().push_back(event));
}
pub fn next_event() -> Option<UiEvent> {
    EVENTS.with(|q| q.borrow_mut().pop_front())
}

pub fn create(handler: Box<dyn WindowHandler>, options: WindowOptions) -> Result<HWND, String> {
    super::fonts::initialize()?;
    let instance = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
    let class = common::wide("CapRust.NativeWindow");
    if !CLASS_READY.with(Cell::get) {
        let icon = unsafe {
            LoadIconW(
                Some(HINSTANCE(instance.0)),
                PCWSTR(std::ptr::without_provenance(1)),
            )
        }
        .unwrap_or_default();
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.map_err(|e| e.to_string())?;
        let definition = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            hCursor: cursor,
            hIcon: icon,
            hIconSm: icon,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&definition) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(format!(
                "注册原生窗口失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        CLASS_READY.with(|ready| ready.set(true));
    }
    let title = common::wide(&options.title);
    let r = options.rect;
    let hwnd = unsafe {
        CreateWindowExW(
            options.ex_style,
            PCWSTR(class.as_ptr()),
            PCWSTR(title.as_ptr()),
            options.style & !WS_VISIBLE,
            r.left,
            r.top,
            (r.right - r.left).max(1),
            (r.bottom - r.top).max(1),
            options.owner,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )
    }
    .map_err(|e| format!("创建窗口失败：{e}"))?;
    let handler = Rc::new(RefCell::new(handler));
    WINDOWS.with(|windows| {
        windows
            .borrow_mut()
            .insert(hwnd.0 as usize, handler.clone())
    });
    if let Err(error) = handler.borrow_mut().created(hwnd) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    if options.visible {
        unsafe {
            let _ = ShowWindow(
                hwnd,
                if options.ex_style.contains(WS_EX_NOACTIVATE) {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_SHOW
                },
            );
            let _ = UpdateWindow(hwnd);
        }
    }
    Ok(hwnd)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    let handler = WINDOWS.with(|windows| windows.borrow().get(&(hwnd.0 as usize)).cloned());
    let result = handler.as_ref().and_then(|handler| {
        let Ok(mut view) = handler.try_borrow_mut() else {
            return None;
        };
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            view.message(hwnd, msg, wparam, lparam)
        })) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                emit(UiEvent::Error(error));
                Some(LRESULT(0))
            }
            Err(_) => {
                emit(UiEvent::Error(
                    "界面事件发生异常，请保存工作后重启 cap。".into(),
                ));
                Some(LRESULT(0))
            }
        }
    });
    if msg == WM_NCDESTROY {
        WINDOWS.with(|windows| {
            windows.borrow_mut().remove(&(hwnd.0 as usize));
        });
    }
    result.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) })
}

pub fn close(hwnd: HWND) {
    if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        unsafe {
            if let Err(e) = DestroyWindow(hwnd) {
                emit(UiEvent::Error(format!("关闭窗口失败：{e}")))
            }
        }
    }
}
pub fn close_all() {
    let windows: Vec<HWND> =
        WINDOWS.with(|w| w.borrow().keys().map(|&p| HWND(p as *mut _)).collect());
    for hwnd in windows {
        close(hwnd)
    }
}

pub struct RootHandler;
impl WindowHandler for RootHandler {
    fn created(&mut self, _hwnd: HWND) -> Result<(), String> {
        if TASKBAR_CREATED.with(Cell::get) == 0 {
            let name = common::wide("TaskbarCreated");
            let message = unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) };
            if message == 0 {
                return Err(format!(
                    "注册系统托盘恢复消息失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            TASKBAR_CREATED.with(|registered| registered.set(message));
        }
        Ok(())
    }
    fn message(
        &mut self,
        _hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> Result<Option<LRESULT>, String> {
        if TASKBAR_CREATED.with(|registered| registered.get() != 0 && registered.get() == msg) {
            emit(UiEvent::ShellRestart);
            return Ok(Some(LRESULT(0)));
        }
        if msg == WM_TIMER {
            emit(UiEvent::Tick);
            return Ok(Some(LRESULT(0)));
        }
        if msg == WM_HOTKEY {
            emit(UiEvent::Hotkey(w.0 as i32));
            return Ok(Some(LRESULT(0)));
        }
        if msg == crate::tray::MESSAGE {
            emit(UiEvent::Tray(l.0));
            return Ok(Some(LRESULT(0)));
        }
        if msg == WM_CLOSE {
            emit(UiEvent::Quit);
            return Ok(Some(LRESULT(0)));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posted_taskbar_recreated_message_reaches_root_event_queue() {
        while next_event().is_some() {}
        let hwnd = create(
            Box::new(RootHandler),
            WindowOptions {
                title: "Cap owned shell restart test".into(),
                rect: common::rect(0, 0, 1, 1),
                style: WS_POPUP,
                ex_style: WS_EX_TOOLWINDOW,
                owner: None,
                visible: false,
            },
        )
        .unwrap();
        let name = common::wide("TaskbarCreated");
        let message = unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) };
        assert_ne!(message, 0);
        unsafe {
            PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0)).unwrap();
            let mut posted = MSG::default();
            assert!(PeekMessageW(&mut posted, Some(hwnd), message, message, PM_REMOVE).as_bool());
            DispatchMessageW(&posted);
        }
        close(hwnd);
        assert!(matches!(next_event(), Some(UiEvent::ShellRestart)));
        assert!(next_event().is_none());
    }
}
