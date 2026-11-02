use super::*;

const BODY_TOP: i32 = 96;
const FOOTER_HEIGHT: i32 = 64;

fn monitor_work(monitor: HMONITOR) -> Result<RECT, String> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(format!(
            "读取设置窗口工作区失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(info.rcWork)
}

pub(super) fn work_area(hwnd: HWND) -> Result<RECT, String> {
    monitor_work(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) })
}

pub(super) fn placement(work: RECT) -> RECT {
    let width = 900.min(work.right - work.left).max(1);
    let height = 680.min(work.bottom - work.top).max(1);
    common::rect(
        work.left + (work.right - work.left - width) / 2,
        work.top + (work.bottom - work.top - height) / 2,
        width,
        height,
    )
}

pub(super) fn initial_rect() -> Result<RECT, String> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.map_err(|e| format!("读取设置窗口位置失败：{e}"))?;
    Ok(placement(monitor_work(unsafe {
        MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
    })?))
}

impl SettingsView {
    pub(super) fn body_label(
        &mut self,
        hwnd: HWND,
        id: u16,
        text: &str,
        rect: RECT,
    ) -> Result<(), String> {
        let child = common::child(hwnd, "STATIC", text, id, rect, WINDOW_STYLE(0))?;
        self.controls.insert(id, child);
        self.positions.insert(id, rect);
        Ok(())
    }

    pub(super) fn layout(&mut self, hwnd: HWND) -> Result<(), String> {
        let client = common::client(hwnd);
        let status_height = if self.tab == 1 { 40 } else { 0 };
        let body_bottom = (client.bottom - FOOTER_HEIGHT - 12 - status_height).max(BODY_TOP + 1);
        let visible_height = body_bottom - BODY_TOP;
        let content_bottom = self
            .positions
            .iter()
            .filter(|(id, _)| !matches!(**id, SAVE | EXIT | KEY_ERROR | 300..=304))
            .map(|(_, rect)| rect.bottom + 16)
            .max()
            .unwrap_or(BODY_TOP);
        let content_height = (content_bottom - BODY_TOP).max(1);
        self.scroll = self
            .scroll
            .clamp(0, (content_height - visible_height).max(0));
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: content_height - 1,
            nPage: visible_height as u32,
            nPos: self.scroll,
            ..Default::default()
        };
        unsafe {
            SetScrollInfo(hwnd, SB_VERT, &info, true);
        }
        // SetScrollInfo can add/remove the non-client scrollbar and change width.
        let client = common::client(hwnd);
        let footer_top = client.bottom - FOOTER_HEIGHT;
        let body_width = (client.right - 248).max(1);
        for (&id, &child) in &self.controls {
            let original = self.positions[&id];
            let (rect, visible) = match id {
                SAVE => (
                    common::rect(client.right - 170, footer_top + 10, 140, 40),
                    true,
                ),
                EXIT => (common::rect(218, footer_top + 10, 125, 40), true),
                KEY_ERROR => (common::rect(218, footer_top - 46, body_width, 36), true),
                300..=304 => {
                    let step = ((footer_top - 107) / 5).clamp(26, 51);
                    (
                        common::rect(
                            20,
                            95 + (id - 300) as i32 * step,
                            145,
                            (step - 6).clamp(20, 40),
                        ),
                        true,
                    )
                }
                _ => {
                    let left = 218 + (original.left - 218) * body_width / 597;
                    let right = 218 + (original.right - 218) * body_width / 597;
                    let top = original.top - self.scroll;
                    let height = original.bottom - original.top;
                    (
                        common::rect(left, top, (right - left).max(1), height),
                        top >= BODY_TOP && top + height <= body_bottom,
                    )
                }
            };
            unsafe {
                SetWindowPos(
                    child,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .map_err(|e| format!("调整设置控件位置失败：{e}"))?;
                let _ = ShowWindow(child, if visible { SW_SHOW } else { SW_HIDE });
            }
        }
        common::invalidate(hwnd);
        Ok(())
    }

    pub(super) fn scroll_message(&mut self, hwnd: HWND, w: WPARAM) -> Result<(), String> {
        let page = (common::client(hwnd).bottom - FOOTER_HEIGHT - BODY_TOP - 12).max(38);
        match w.0 as u16 {
            code if code == SB_LINEUP.0 as u16 => self.scroll -= 38,
            code if code == SB_LINEDOWN.0 as u16 => self.scroll += 38,
            code if code == SB_PAGEUP.0 as u16 => self.scroll -= page,
            code if code == SB_PAGEDOWN.0 as u16 => self.scroll += page,
            code if code == SB_TOP.0 as u16 => self.scroll = 0,
            code if code == SB_BOTTOM.0 as u16 => self.scroll = i32::MAX,
            code if code == SB_THUMBPOSITION.0 as u16 || code == SB_THUMBTRACK.0 as u16 => {
                let mut info = SCROLLINFO {
                    cbSize: size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_TRACKPOS,
                    ..Default::default()
                };
                unsafe { GetScrollInfo(hwnd, SB_VERT, &mut info) }
                    .map_err(|e| format!("读取设置滚动位置失败：{e}"))?;
                self.scroll = info.nTrackPos;
            }
            _ => {}
        }
        self.layout(hwnd)
    }
}
