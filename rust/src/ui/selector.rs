use super::{
    UiEvent,
    common::{self, Canvas},
    ocr_card, options_bar, record_panel,
    runtime::{self, WindowHandler, WindowOptions},
    scroll_start,
    toolbar::{self, TOOL_COMMAND, Tool, ToolbarState},
};
use cap_rust::{
    annotation::{AnnotationDocument, Effect, PaintCommand, Point, Style},
    capture::{self, DesktopWindow},
    geometry::{self, Rect},
    pixels::Frame,
    recording::RecordingFormat,
    selection::{CaptureMode, SelectionAction, SelectionModel},
    settings::Settings,
};
use std::{cell::RefCell, rc::Rc};
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

#[path = "selector_buffer.rs"]
pub(super) mod buffer;

const TEXT_ID: u16 = 9400;
const COMMIT_TEXT: u32 = WM_APP + 82;
const CANCEL_TEXT: u32 = WM_APP + 83;
pub fn show(
    frame: Frame,
    desktop: Rect,
    mode: CaptureMode,
    format: RecordingFormat,
    settings: Rc<RefCell<Settings>>,
) -> Result<HWND, String> {
    frame.validate()?;
    desktop.validate()?;
    if frame.width != desktop.width || frame.height != desktop.height {
        return Err("冻结图像与桌面尺寸不一致。".into());
    }
    let mut dimmed = frame.copy()?;
    for pixel in dimmed.pixels.chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            *channel = (*channel as u16 * 48 / 100) as u8;
        }
    }
    let theme = settings.borrow().theme;
    let mut initial = ToolbarState {
        mode,
        ..ToolbarState::default()
    };
    initial.theme = theme;
    initial.color = toolbar::colors(theme)[0];
    let toolbar = Rc::new(RefCell::new(initial));
    let windows = capture::windows()?;
    let magnifier_state = Rc::new(RefCell::new(super::magnifier::MagnifierState::new(
        frame.copy()?,
        theme,
    )));
    runtime::create(
        Box::new(Selector {
            document: AnnotationDocument::new(frame.copy()?)?,
            rendered: frame,
            dimmed,
            back_buffer: None,
            model: SelectionModel::new(desktop, mode),
            format,
            settings,
            toolbar,
            mode_bar: HWND::default(),
            tools_bar: HWND::default(),
            options_bar: HWND::default(),
            magnifier: HWND::default(),
            magnifier_state,
            scroll_start: HWND::default(),
            record_panel: HWND::default(),
            ocr_card: HWND::default(),
            ocr_state: Rc::new(RefCell::new(ocr_card::OcrState::new(theme))),
            ocr_open: false,
            cropped: false,
            windows,
            hover: None,
            drag: None,
            paint_start: None,
            points: Vec::new(),
            edit: None,
        }),
        WindowOptions {
            title: "cap · 截图选区".into(),
            rect: common::rect(
                desktop.x,
                desktop.y,
                desktop.width as i32,
                desktop.height as i32,
            ),
            style: WS_POPUP,
            ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            owner: None,
            visible: true,
        },
    )
}

struct Drag {
    start: (f64, f64),
    original: Option<Rect>,
    handle: i32,
}
struct TextEdit {
    window: HWND,
    point: Point,
}
struct Selector {
    document: AnnotationDocument,
    rendered: Frame,
    dimmed: Frame,
    back_buffer: Option<buffer::PaintBuffer>,
    model: SelectionModel,
    format: RecordingFormat,
    settings: Rc<RefCell<Settings>>,
    toolbar: Rc<RefCell<ToolbarState>>,
    mode_bar: HWND,
    tools_bar: HWND,
    options_bar: HWND,
    magnifier: HWND,
    magnifier_state: Rc<RefCell<super::magnifier::MagnifierState>>,
    scroll_start: HWND,
    record_panel: HWND,
    ocr_card: HWND,
    ocr_state: Rc<RefCell<ocr_card::OcrState>>,
    ocr_open: bool,
    cropped: bool,
    windows: Vec<DesktopWindow>,
    hover: Option<Rect>,
    drag: Option<Drag>,
    paint_start: Option<Point>,
    points: Vec<Point>,
    edit: Option<TextEdit>,
}
impl Drop for Selector {
    fn drop(&mut self) {
        runtime::close(self.mode_bar);
        runtime::close(self.tools_bar);
        runtime::close(self.options_bar);
        runtime::close(self.magnifier);
        runtime::close(self.scroll_start);
        runtime::close(self.record_panel);
        runtime::close(self.ocr_card);
        runtime::emit(UiEvent::SelectionClosed);
    }
}
impl Selector {
    fn style(&self) -> Style {
        let style = self.toolbar.borrow();
        Style {
            color: style.color,
            thickness: style.width as f64,
        }
    }
    fn refresh(&mut self, hwnd: HWND) -> Result<(), String> {
        self.rendered = self.document.render()?;
        {
            let mut toolbar = self.toolbar.borrow_mut();
            toolbar.can_undo = self.document.can_undo();
            toolbar.can_redo = self.document.can_redo();
        }
        common::invalidate(hwnd);
        common::invalidate(self.tools_bar);
        common::invalidate(self.options_bar);
        Ok(())
    }
    fn close(&self, hwnd: HWND) {
        runtime::close(hwnd)
    }
    fn bars(&self) -> Result<(), String> {
        let desktop = self.model.desktop;
        let center = POINT {
            x: desktop.x + desktop.width as i32 / 2,
            y: desktop.y + desktop.height as i32 / 2,
        };
        let monitor = unsafe { MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err("无法读取工具栏所在显示器。".into());
        }
        let work = info.rcWork;
        let (mode_width, mode_height) = toolbar::bar_size(true);
        let (tools_width, tools_height) = toolbar::bar_size(false);
        // Feishu parks the function bar at the top centre of the monitor, with or without a selection.
        let modes = common::rect(
            (work.left + (work.right - work.left - mode_width) / 2)
                .clamp(work.left, (work.right - mode_width).max(work.left)),
            work.top + 6,
            (mode_width).min(work.right - work.left),
            mode_height,
        );
        let tools = match self.model.selected {
            Some(region) => {
                // Right edge aligns with the selection, 15px below it (Feishu layout).
                let x = (desktop.x + region.right() - tools_width)
                    .clamp(work.left, (work.right - tools_width).max(work.left));
                let below = desktop.y + region.bottom() + 15;
                let y = if below + tools_height <= work.bottom {
                    below
                } else {
                    (desktop.y + region.y - tools_height - 15).max(work.top + 6)
                };
                common::rect(
                    x,
                    y.clamp(work.top, (work.bottom - tools_height).max(work.top)),
                    tools_width,
                    tools_height,
                )
            }
            None => common::rect(0, 0, tools_width, tools_height),
        };
        // Feishu floats the contextual row 15px below the annotation bar, content-width.
        let options_width = options_bar::width(self.toolbar.borrow().tool);
        let options_height = options_bar::height();
        let options_top = (tools.bottom + 15).min((work.bottom - options_height).max(work.top));
        let caret_x = toolbar::tool_center_x(self.toolbar.borrow().tool);
        let options_left = (tools.left + caret_x - 40)
            .clamp(tools.left, (tools.right - options_width).max(tools.left))
            .clamp(work.left, (work.right - options_width).max(work.left));
        let options = common::rect(options_left, options_top, options_width, options_height);
        let tools_visible =
            self.model.selected.is_some() && self.model.mode == CaptureMode::Screenshot;
        // Feishu's scrolling pill and recording card appear inside the overlay, never auto-starting.
        let selection = self.model.selected;
        let scroll_card = selection
            .map(|region| {
                let x = (desktop.x + region.x + 13)
                    .clamp(work.left, (work.right - scroll_start::WIDTH).max(work.left));
                let y = (desktop.y + region.bottom() + 15)
                    .clamp(work.top, (work.bottom - scroll_start::HEIGHT).max(work.top));
                common::rect(x, y, scroll_start::WIDTH, scroll_start::HEIGHT)
            })
            .unwrap_or_else(|| common::rect(0, 0, scroll_start::WIDTH, scroll_start::HEIGHT));
        let record_card = selection
            .map(|region| {
                let x = (desktop.x + region.x + (region.width as i32 - record_panel::WIDTH) / 2)
                    .clamp(work.left, (work.right - record_panel::WIDTH).max(work.left));
                let y = (desktop.y + region.bottom() + 15)
                    .clamp(work.top, (work.bottom - record_panel::HEIGHT).max(work.top));
                common::rect(x, y, record_panel::WIDTH, record_panel::HEIGHT)
            })
            .unwrap_or_else(|| common::rect(0, 0, record_panel::WIDTH, record_panel::HEIGHT));
        for (window, rect, visible) in [
            (self.mode_bar, modes, !self.cropped),
            (self.tools_bar, tools, tools_visible),
            (
                self.options_bar,
                options,
                tools_visible && options_bar::visible_for(self.toolbar.borrow().tool),
            ),
            (
                self.scroll_start,
                scroll_card,
                selection.is_some() && self.model.mode == CaptureMode::Scrolling,
            ),
            (
                self.record_panel,
                record_card,
                selection.is_some() && self.model.mode == CaptureMode::Recording,
            ),
        ] {
            unsafe {
                SetWindowPos(
                    window,
                    Some(HWND_TOPMOST),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE
                        | if visible {
                            SWP_SHOWWINDOW
                        } else {
                            SWP_HIDEWINDOW
                        },
                )
            }
            .map_err(|e| e.to_string())?;
            common::invalidate(window);
        }
        Ok(())
    }
    /// Feishu crop: bake the selection, shrink the overlay to it, and keep editing in place.
    fn crop(&mut self, hwnd: HWND) -> Result<(), String> {
        let Some(region) = self.model.selected else {
            return Ok(());
        };
        if region.is_empty() {
            return Ok(());
        }
        let desktop = self.model.desktop;
        let absolute = Rect::new(
            desktop.x + region.x,
            desktop.y + region.y,
            region.width,
            region.height,
        )?;
        let frame = self.document.export(region)?;
        self.document = AnnotationDocument::new(frame.clone())?;
        self.rendered = frame.clone();
        self.dimmed = frame.clone();
        {
            let mut state = self.magnifier_state.borrow_mut();
            state.frame = Some(frame);
            state.cursor = (0, 0);
        }
        self.model = SelectionModel::new(absolute, self.model.mode);
        self.model
            .choose(Rect::new(0, 0, region.width, region.height)?)?;
        self.cropped = true;
        self.back_buffer = None;
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                absolute.x,
                absolute.y,
                absolute.width as i32,
                absolute.height as i32,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
        .map_err(|e| e.to_string())?;
        self.bars()?;
        common::invalidate(hwnd);
        Ok(())
    }

    /// Feishu''s colour picker follows the cursor over the frozen desktop.
    fn update_picker(&mut self, x: i32, y: i32) -> Result<(), String> {
        if self.magnifier.is_invalid() {
            return Ok(());
        }
        let desktop = self.model.desktop;
        let screen = (desktop.x + x, desktop.y + y);
        super::magnifier::refresh(
            self.magnifier,
            &self.magnifier_state,
            &self.rendered,
            screen,
        );
        let monitor = unsafe {
            MonitorFromPoint(
                POINT {
                    x: screen.0,
                    y: screen.1,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            super::magnifier::place(self.magnifier, screen, info.rcWork);
        }
        Ok(())
    }

    fn export(&mut self) -> Result<Frame, String> {
        self.finish_text(true)?;
        self.document
            .export(self.model.selected.ok_or("请先框选截图区域。")?)
    }
    fn finish(&mut self, hwnd: HWND) -> Result<(), String> {
        self.finish_text(true)?;
        match self.model.commit()? {
            SelectionAction::Copy => {
                crate::platform::copy_image(hwnd, &self.export()?)?;
                self.close(hwnd);
            }
            SelectionAction::Record(bounds) => {
                runtime::emit(UiEvent::RecordRegion(bounds, self.format));
                self.close(hwnd);
            }
            SelectionAction::Scroll(bounds) => {
                runtime::emit(UiEvent::BeginScroll(bounds));
                self.close(hwnd);
            }
            SelectionAction::Ocr(_) => {
                runtime::emit(UiEvent::Recognize(self.export()?));
                self.close(hwnd);
            }
            _ => {}
        }
        Ok(())
    }
    fn finish_text(&mut self, commit: bool) -> Result<(), String> {
        if let Some(edit) = self.edit.take() {
            let value = common::get_text(edit.window);
            unsafe {
                let _ = DestroyWindow(edit.window);
            }
            if commit && !value.trim().is_empty() {
                let style = self.style();
                let stamp = cap_rust::text_raster::rasterize(&value, style.color, style.thickness)?;
                self.document.add(PaintCommand::Stamp {
                    origin: edit.point,
                    frame: stamp,
                })?;
                self.rendered = self.document.render()?;
            }
        }
        Ok(())
    }
    fn point_in_selection(&self, x: i32, y: i32) -> Point {
        if let Some(r) = self.model.selected {
            Point::new(
                x.clamp(r.x, r.right() - 1) as f64,
                y.clamp(r.y, r.bottom() - 1) as f64,
            )
        } else {
            Point::new(x as f64, y as f64)
        }
    }
    fn tool_command(&mut self, hwnd: HWND, id: usize) -> Result<(), String> {
        self.finish_text(true)?;
        if (1000..1004).contains(&id) {
            self.model.mode = match id - 1000 {
                0 => CaptureMode::Screenshot,
                1 => CaptureMode::Scrolling,
                2 => CaptureMode::Recording,
                _ => CaptureMode::Text,
            };
            self.toolbar.borrow_mut().mode = self.model.mode;
            self.bars()?;
            common::invalidate(hwnd);
            return Ok(());
        }
        if (toolbar::EFFECT_BASE..toolbar::EFFECT_BASE + 3).contains(&id) {
            self.toolbar.borrow_mut().effect = match id - toolbar::EFFECT_BASE {
                0 => Effect::Pixelate,
                1 => Effect::Blur,
                _ => Effect::Block,
            };
            common::invalidate(self.tools_bar);
            common::invalidate(self.options_bar);
            return Ok(());
        }
        if id == toolbar::CMD_SIZE_MENU {
            // Interim dropdown behaviour: one button cycles the five Feishu size steps.
            let width = self.toolbar.borrow().width;
            let next = options_bar::SIZE_PRESETS
                .iter()
                .map(|(thickness, _)| *thickness)
                .find(|thickness| *thickness > width)
                .unwrap_or(options_bar::SIZE_PRESETS[0].0);
            self.toolbar.borrow_mut().width = next;
            common::invalidate(self.tools_bar);
            common::invalidate(self.options_bar);
            return Ok(());
        }
        if (toolbar::COLOR_BASE..toolbar::COLOR_BASE + toolbar::COLORS.len()).contains(&id) {
            let theme = self.toolbar.borrow().theme;
            let index = id - toolbar::COLOR_BASE;
            self.toolbar.borrow_mut().color = toolbar::colors(theme)[index];
            common::invalidate(self.tools_bar);
            common::invalidate(self.options_bar);
            return Ok(());
        }
        if id > toolbar::WIDTH_BASE && id <= toolbar::WIDTH_BASE + 12 {
            self.toolbar.borrow_mut().width = (id - toolbar::WIDTH_BASE) as u32;
            common::invalidate(self.tools_bar);
            common::invalidate(self.options_bar);
            return Ok(());
        }
        match id {
            0..=9 => {
                self.toolbar.borrow_mut().tool = match id {
                    0 => Tool::Select,
                    1 => Tool::Rectangle,
                    2 => Tool::Ellipse,
                    3 => Tool::Arrow,
                    4 => Tool::Line,
                    5 => Tool::Pen,
                    6 => Tool::Text,
                    7 => Tool::Number,
                    8 => Tool::Mosaic,
                    _ => Tool::Blur,
                };
                common::invalidate(self.tools_bar);
                // Feishu only shows the contextual row for drawing tools.
                self.bars()?;
            }
            toolbar::CMD_MARKER => {
                self.toolbar.borrow_mut().tool = Tool::Marker;
                common::invalidate(self.tools_bar);
                self.bars()?;
            }
            10 => {
                self.document.undo();
                self.refresh(hwnd)?;
            }
            11 => {
                self.document.redo();
                self.refresh(hwnd)?;
            }
            12 => {
                runtime::emit(UiEvent::Pin(self.export()?));
                self.close(hwnd);
            }
            13 => {
                let image = self.export()?;
                let directory = self.settings.borrow().save_directory.clone();
                if crate::platform::save_image(hwnd, &directory, &image)? {
                    self.close(hwnd);
                }
            }
            14 => self.finish(hwnd)?,
            15 => self.close(hwnd),
            toolbar::CMD_CROP => self.crop(hwnd)?,
            toolbar::CMD_OCR => self.run_ocr(hwnd)?,
            ocr_card::CMD_CLOSE => self.hide_ocr_card(),
            ocr_card::CMD_COPY => self.copy_ocr(hwnd)?,
            ocr_card::CMD_EDIT => {
                // Editing the recognition in place is not supported yet; copy instead.
                self.copy_ocr(hwnd)?;
            }
            toolbar::CMD_AUTOSCROLL => {
                self.model.mode = CaptureMode::Scrolling;
                self.toolbar.borrow_mut().mode = self.model.mode;
                self.finish(hwnd)?;
            }
            toolbar::CMD_CONFIRM => self.finish(hwnd)?,
            scroll_start::CMD_START => {
                if let SelectionAction::Scroll(bounds) = self.model.commit()? {
                    runtime::emit(UiEvent::BeginScroll(bounds));
                    self.close(hwnd);
                }
            }
            scroll_start::CMD_HELP => crate::ui::common::error(
                hwnd,
                "滚动截图：点击「开始滚动截图」后滚动鼠标滚轮或拖动页面，cap 会自动拼接长图；Esc 可随时退出。",
            ),
            record_panel::CMD_RECORD_MP4 => {
                self.start_recording(hwnd, record_panel::Format::Mp4)?
            }
            record_panel::CMD_RECORD_GIF => {
                self.start_recording(hwnd, record_panel::Format::Gif)?
            }
            16 => {
                self.model.mode = CaptureMode::Recording;
                self.finish(hwnd)?;
            }
            _ => {}
        }
        Ok(())
    }
    /// Runs Windows OCR on the selection and shows Feishu's inline card.
    fn run_ocr(&mut self, hwnd: HWND) -> Result<(), String> {
        let Some(region) = self.model.selected else {
            return Ok(());
        };
        let frame = self.document.export(region)?;
        {
            let mut state = self.ocr_state.borrow_mut();
            state.busy = true;
            state.text.clear();
        }
        self.show_ocr_card();
        common::invalidate(self.ocr_card);
        unsafe {
            let _ = UpdateWindow(self.ocr_card);
        }
        let text = match cap_rust::media::ocr(&frame) {
            Ok(text) => text,
            Err(error) => format!("识别失败：{error}"),
        };
        {
            let mut state = self.ocr_state.borrow_mut();
            state.busy = false;
            state.text = text;
        }
        common::invalidate(self.ocr_card);
        let _ = hwnd;
        Ok(())
    }
    fn show_ocr_card(&mut self) {
        let Some(region) = self.model.selected else {
            return;
        };
        let x = region.x + (region.width as i32 - ocr_card::WIDTH) / 2;
        let y = region.y + (region.height as i32 - ocr_card::HEIGHT) / 2;
        let x = x.clamp(0, (self.dimmed.width as i32 - ocr_card::WIDTH).max(0));
        let y = y.clamp(0, (self.dimmed.height as i32 - ocr_card::HEIGHT).max(0));
        unsafe {
            let _ = SetWindowPos(
                self.ocr_card,
                Some(HWND_TOPMOST),
                x,
                y,
                ocr_card::WIDTH,
                ocr_card::HEIGHT,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        self.ocr_open = true;
    }
    fn hide_ocr_card(&mut self) {
        self.ocr_open = false;
        unsafe {
            let _ = ShowWindow(self.ocr_card, SW_HIDE);
        }
    }
    fn copy_ocr(&mut self, hwnd: HWND) -> Result<(), String> {
        let text = { self.ocr_state.borrow().text.clone() };
        if !text.trim().is_empty() {
            crate::platform::copy_text(hwnd, &text)?;
        }
        Ok(())
    }

    /// Starts region recording with the format chosen on the overlay card.
    fn start_recording(&mut self, hwnd: HWND, format: record_panel::Format) -> Result<(), String> {
        if let SelectionAction::Record(bounds) = self.model.commit()? {
            runtime::emit(UiEvent::RecordRegion(bounds, format.recording()));
            self.close(hwnd);
        }
        Ok(())
    }
    fn mouse_down(&mut self, hwnd: HWND, x: i32, y: i32) -> Result<(), String> {
        self.finish_text(true)?;
        let tool = self.toolbar.borrow().tool;
        if self.model.selected.is_none()
            || tool == Tool::Select
            || self.model.mode != CaptureMode::Screenshot
            || self
                .model
                .selected
                .is_some_and(|r| geometry::hit(r, (x as f64, y as f64), 6.) >= 0)
        {
            let original = self.model.selected;
            let handle = original
                .map(|r| geometry::hit(r, (x as f64, y as f64), 6.))
                .unwrap_or(-2);
            self.drag = Some(Drag {
                start: (x as f64, y as f64),
                original: if handle >= -1 { original } else { None },
                handle,
            });
            unsafe {
                SetCapture(hwnd);
            }
            return Ok(());
        }
        let selected = self.model.selected.ok_or("请先选择区域。")?;
        if !selected.contains(x, y) {
            self.drag = Some(Drag {
                start: (x as f64, y as f64),
                original: None,
                handle: -2,
            });
            unsafe {
                SetCapture(hwnd);
            }
            return Ok(());
        }
        let point = self.point_in_selection(x, y);
        if tool == Tool::Text {
            let edit = common::child(
                hwnd,
                "EDIT",
                "",
                TEXT_ID,
                common::rect(
                    x,
                    y,
                    (selected.right() - x).clamp(70, 420),
                    (selected.bottom() - y).clamp(32, 100),
                ),
                WS_BORDER
                    | WS_TABSTOP
                    | WINDOW_STYLE(
                        ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
                    ),
            )?;
            self.edit = Some(TextEdit {
                window: edit,
                point,
            });
            unsafe {
                let _ = SetFocus(Some(edit));
            }
            return Ok(());
        }
        if tool == Tool::Number {
            self.document.add(PaintCommand::Number {
                center: point,
                number: self.document.next_number(),
                style: self.style(),
            })?;
            return self.refresh(hwnd);
        }
        self.paint_start = Some(point);
        self.points = vec![point];
        unsafe {
            SetCapture(hwnd);
        }
        Ok(())
    }
    fn mouse_move(&mut self, hwnd: HWND, x: i32, y: i32) -> Result<(), String> {
        self.update_picker(x, y)?;
        if let Some(drag) = &self.drag {
            let r = if let Some(original) = drag.original {
                geometry::adjust(
                    original,
                    drag.start,
                    (x as f64, y as f64),
                    drag.handle,
                    self.model.desktop.width,
                    self.model.desktop.height,
                )?
            } else {
                geometry::drag(
                    drag.start,
                    (x as f64, y as f64),
                    self.model.desktop.width,
                    self.model.desktop.height,
                )?
            };
            if !r.is_empty() && self.model.selected != Some(r) {
                self.model.choose(r)?;
                common::invalidate(hwnd);
            }
            return Ok(());
        }
        if self.paint_start.is_some() {
            self.points.push(self.point_in_selection(x, y));
            common::invalidate(hwnd);
            return Ok(());
        }
        if self.model.selected.is_none() {
            let p = (x + self.model.desktop.x, y + self.model.desktop.y);
            let hover = self
                .windows
                .iter()
                .find(|w| w.bounds.contains(p.0, p.1))
                .and_then(|w| {
                    let r = w.bounds.intersect(self.model.desktop);
                    Rect::new(
                        r.x - self.model.desktop.x,
                        r.y - self.model.desktop.y,
                        r.width,
                        r.height,
                    )
                    .ok()
                });
            if self.hover != hover {
                self.hover = hover;
                common::invalidate(hwnd);
            }
        }
        Ok(())
    }
    fn mouse_up(&mut self, hwnd: HWND, x: i32, y: i32) -> Result<(), String> {
        // Finish the gesture before releasing capture. ReleaseCapture sends a
        // synchronous WM_CAPTURECHANGED even for our own normal mouse-up.
        let result = self.complete_gesture(hwnd, x, y);
        if unsafe { GetCapture() } == hwnd {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        result
    }
    fn complete_gesture(&mut self, hwnd: HWND, x: i32, y: i32) -> Result<(), String> {
        if let Some(drag) = self.drag.take() {
            if self.model.selected.is_none()
                || ((x as f64 - drag.start.0).abs() < 3.
                    && (y as f64 - drag.start.1).abs() < 3.
                    && drag.original.is_none())
            {
                if let Some(r) = self.hover {
                    self.model.choose(r)?;
                }
            }
            self.bars()?;
            common::invalidate(hwnd);
            // Feishu runs OCR as soon as a region is selected in text mode.
            if self.model.mode == CaptureMode::Text && self.model.selected.is_some() {
                self.run_ocr(hwnd)?;
            }
            return Ok(());
        }
        if let Some(start) = self.paint_start.take() {
            let end = self.point_in_selection(x, y);
            let style = self.style();
            let command = match self.toolbar.borrow().tool {
                Tool::Rectangle => Some(PaintCommand::Rectangle { start, end, style }),
                Tool::Ellipse => Some(PaintCommand::Ellipse { start, end, style }),
                Tool::Arrow => Some(PaintCommand::Arrow { start, end, style }),
                Tool::Line => Some(PaintCommand::Line { start, end, style }),
                Tool::Pen => Some(PaintCommand::Pen {
                    points: std::mem::take(&mut self.points),
                    style,
                }),
                Tool::Marker => {
                    // Feishu 标注: translucent highlighter (30%) with a broad nib.
                    let mut style = style;
                    // Feishu's 标注 opacity slider drives the highlight alpha (default ~30%).
                    style.color[3] = (self.toolbar.borrow().width * 8).clamp(8, 96) as u8;
                    style.thickness *= 2.6;
                    Some(PaintCommand::Pen {
                        points: std::mem::take(&mut self.points),
                        style,
                    })
                }
                Tool::Mosaic | Tool::Blur => {
                    let rect = Rect::from_points(
                        (start.x as i32, start.y as i32),
                        (end.x as i32, end.y as i32),
                    )?;
                    if rect.is_empty() {
                        None
                    } else {
                        Some(self.document.effect(
                            if self.toolbar.borrow().tool == Tool::Mosaic {
                                Effect::Pixelate
                            } else {
                                Effect::Blur
                            },
                            rect,
                            style.thickness as u32,
                        )?)
                    }
                }
                _ => None,
            };
            if let Some(command) = command {
                self.document.add(command)?;
            }
            self.refresh(hwnd)?;
        }
        Ok(())
    }
    fn paint(&mut self, hwnd: HWND) -> Result<(), String> {
        let canvas = Canvas::begin(hwnd);
        if self
            .back_buffer
            .as_ref()
            .is_none_or(|buffer| !buffer.matches(self.dimmed.width, self.dimmed.height))
        {
            self.back_buffer = Some(buffer::PaintBuffer::new(
                canvas.dc,
                self.dimmed.width,
                self.dimmed.height,
            )?);
        }
        let c = self.back_buffer.as_ref().ok_or("选区绘图缓存未创建。")?;
        let full = common::rect(0, 0, self.dimmed.width as i32, self.dimmed.height as i32);
        common::draw_frame(c.dc, &self.dimmed, full)?;
        if let Some(r) = self.model.selected.or(self.hover) {
            // Feishu paints recording regions pink; screenshots use #2596FF.
            let frame_colour = if self.model.mode == CaptureMode::Recording {
                common::rgb(0xDB, 0x5E, 0xA4)
            } else if self.toolbar.borrow().theme == cap_rust::settings::Theme::Light {
                common::rgb(0x33, 0x6D, 0xF4)
            } else {
                common::rgb(0x25, 0x96, 0xFF)
            };
            unsafe {
                let saved = SaveDC(c.dc);
                IntersectClipRect(c.dc, r.x, r.y, r.right(), r.bottom());
                let result = common::draw_frame(c.dc, &self.rendered, full);
                let _ = RestoreDC(c.dc, saved);
                result?;
                // Round handles with a white ring, in the frame's own colour.
                let pen = CreatePen(PS_SOLID, 2, COLORREF(frame_colour));
                let old = SelectObject(c.dc, pen.into());
                let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
                let _ = Rectangle(c.dc, r.x, r.y, r.right(), r.bottom());
                SelectObject(c.dc, brush);
                SelectObject(c.dc, old);
                let _ = DeleteObject(pen.into());
            }
            unsafe {
                let null_pen = SelectObject(c.dc, GetStockObject(NULL_PEN));
                let white = CreateSolidBrush(COLORREF(common::rgb(255, 255, 255)));
                let blue = CreateSolidBrush(COLORREF(frame_colour));
                for (x, y) in geometry::handles(r) {
                    let (x, y) = (x as i32, y as i32);
                    let previous = SelectObject(c.dc, white.into());
                    let _ = Ellipse(c.dc, x - 7, y - 7, x + 7, y + 7);
                    SelectObject(c.dc, blue.into());
                    let _ = Ellipse(c.dc, x - 5, y - 5, x + 5, y + 5);
                    SelectObject(c.dc, previous);
                }
                SelectObject(c.dc, null_pen);
                let _ = DeleteObject(white.into());
                let _ = DeleteObject(blue.into());
            }
            let label = format!("{} × {}", r.width, r.height);
            let mut size = SIZE::default();
            let text: Vec<u16> = label.encode_utf16().collect();
            unsafe {
                let previous = SelectObject(c.dc, super::fonts::normal().into());
                let _ = GetTextExtentPoint32W(c.dc, &text, &mut size);
                SelectObject(c.dc, previous);
            }
            let chip_width = (size.cx + 18).max(56);
            let chip_x =
                r.x.min((self.model.desktop.width as i32 - chip_width).max(0));
            let y = (r.y - 27).max(0);
            common::rounded(
                c.dc,
                common::rect(chip_x, y, chip_width, 24),
                common::rgb(0x1C, 0x1D, 0x21),
                4,
            );
            common::label(
                c.dc,
                &label,
                common::rect(chip_x + 9, y, chip_width - 12, 24),
                0xffffff,
            );
        } else {
            common::fill(c.dc, common::rect(20, 20, 600, 42), common::rgb(28, 29, 33));
            common::label(
                c.dc,
                "拖动框选区域，或单击窗口 · 松开后选择功能 · Esc 取消",
                common::rect(32, 20, 575, 42),
                0xffffff,
            );
        }
        if let Some(start) = self.paint_start {
            if let Some(end) = self.points.last() {
                let s = self.style();
                let color = common::rgb(s.color[2], s.color[1], s.color[0]);
                unsafe {
                    let pen = CreatePen(PS_SOLID, s.thickness as i32, COLORREF(color));
                    let old = SelectObject(c.dc, pen.into());
                    let brush = SelectObject(c.dc, GetStockObject(NULL_BRUSH));
                    match self.toolbar.borrow().tool {
                        Tool::Rectangle | Tool::Mosaic | Tool::Blur => {
                            let _ = Rectangle(
                                c.dc,
                                start.x as i32,
                                start.y as i32,
                                end.x as i32,
                                end.y as i32,
                            );
                        }
                        Tool::Ellipse => {
                            let _ = Ellipse(
                                c.dc,
                                start.x as i32,
                                start.y as i32,
                                end.x as i32,
                                end.y as i32,
                            );
                        }
                        Tool::Pen | Tool::Marker => {
                            let points: Vec<POINT> = self
                                .points
                                .iter()
                                .map(|p| POINT {
                                    x: p.x as i32,
                                    y: p.y as i32,
                                })
                                .collect();
                            let _ = Polyline(c.dc, &points);
                        }
                        _ => {
                            let _ = MoveToEx(c.dc, start.x as i32, start.y as i32, None);
                            let _ = LineTo(c.dc, end.x as i32, end.y as i32);
                        }
                    }
                    SelectObject(c.dc, brush);
                    SelectObject(c.dc, old);
                    let _ = DeleteObject(pen.into());
                }
            }
        }
        c.present(canvas.dc)
    }
}
impl WindowHandler for Selector {
    fn created(&mut self, hwnd: HWND) -> Result<(), String> {
        // The desktop frame is already frozen. Keep this UI capturable so
        // remote desktops can display the selection mask and accept input.
        self.mode_bar = toolbar::show(hwnd, self.toolbar.clone(), true)?;
        self.tools_bar = toolbar::show(hwnd, self.toolbar.clone(), false)?;
        self.options_bar = options_bar::show(hwnd, self.toolbar.clone())?;
        self.magnifier = super::magnifier::show(hwnd, self.magnifier_state.clone())?;
        self.scroll_start = scroll_start::show(hwnd)?;
        self.record_panel = record_panel::show(hwnd, self.toolbar.borrow().theme)?;
        self.ocr_card = ocr_card::show(hwnd, self.ocr_state.clone())?;
        // Feishu shows the function bar immediately, parked at the monitor's top centre.
        self.bars()?;
        unsafe {
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
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
            WM_CLOSE => {
                self.close(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            WM_PAINT => {
                self.paint(hwnd)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_LBUTTONDOWN => {
                let (x, y) = common::point(l);
                self.mouse_down(hwnd, x, y)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_MOUSEMOVE => {
                let (x, y) = common::point(l);
                self.mouse_move(hwnd, x, y)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_LBUTTONUP => {
                let (x, y) = common::point(l);
                self.mouse_up(hwnd, x, y)?;
                return Ok(Some(LRESULT(0)));
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                self.drag = None;
                self.paint_start = None;
                self.points.clear();
                if msg == WM_CANCELMODE && unsafe { GetCapture() } == hwnd {
                    unsafe { ReleaseCapture() }.map_err(|e| e.to_string())?;
                }
                if !self.mode_bar.is_invalid() {
                    self.bars()?;
                }
                common::invalidate(hwnd);
                return Ok(Some(LRESULT(0)));
            }
            TOOL_COMMAND => {
                self.tool_command(hwnd, w.0)?;
                return Ok(Some(LRESULT(0)));
            }
            COMMIT_TEXT | CANCEL_TEXT => {
                self.finish_text(msg == COMMIT_TEXT)?;
                self.refresh(hwnd)?;
                unsafe {
                    let _ = SetFocus(Some(hwnd));
                }
                return Ok(Some(LRESULT(0)));
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let key = w.0 as u32;
                let modifiers = crate::platform::held_modifiers();
                if key == 0x1b {
                    if self.ocr_open {
                        self.hide_ocr_card();
                        return Ok(Some(LRESULT(0)));
                    }
                    self.close(hwnd)
                } else if key == 0x0d {
                    self.finish(hwnd)?;
                } else if key == 0x10 {
                    // Shift toggles RGB/HEX exactly like Feishu's picker.
                    {
                        let mut state = self.magnifier_state.borrow_mut();
                        state.hex = !state.hex;
                    }
                    common::invalidate(self.magnifier);
                } else if modifiers & 2 != 0 && key == 0x43 {
                    if self.ocr_open {
                        self.copy_ocr(hwnd)?;
                    } else if self.model.selected.is_none() {
                        let value = self.magnifier_state.borrow().value_text();
                        if !value.is_empty() {
                            crate::platform::copy_text(hwnd, &value)?;
                        }
                    } else {
                        crate::platform::copy_image(hwnd, &self.export()?)?;
                        self.close(hwnd)
                    }
                } else if modifiers & 2 != 0 && key == 0x53 {
                    self.tool_command(hwnd, 13)?;
                } else if modifiers & 2 != 0 && (key == 0x5a || key == 0x59) {
                    if key == 0x59 || modifiers & 4 != 0 {
                        self.document.redo();
                    } else {
                        self.document.undo();
                    }
                    self.refresh(hwnd)?;
                } else if modifiers & 1 != 0 && matches!(key, 0x25 | 0x27) {
                    self.model.cycle_mode(if key == 0x27 { 1 } else { -1 });
                    self.toolbar.borrow_mut().mode = self.model.mode;
                    self.bars()?;
                } else if matches!(key, 0x25..=0x28) && self.model.selected.is_some() {
                    let step = if modifiers & 4 != 0 { 10 } else { 1 };
                    let (dx, dy) = match key {
                        0x25 => (-step, 0),
                        0x27 => (step, 0),
                        0x26 => (0, -step),
                        _ => (0, step),
                    };
                    self.model.move_by(dx, dy)?;
                    self.bars()?;
                    common::invalidate(hwnd);
                }
                return Ok(Some(LRESULT(0)));
            }
            _ => {}
        }
        Ok(None)
    }
}

pub fn capture_text_key(message: &MSG) -> bool {
    if message.message != WM_KEYDOWN || !matches!(message.wParam.0, 0x0d | 0x1b) {
        return false;
    }
    let focus = unsafe { GetFocus() };
    if unsafe { GetDlgCtrlID(focus) } != TEXT_ID as i32 {
        return false;
    }
    if message.wParam.0 == 0x0d && crate::platform::held_modifiers() & 2 == 0 {
        return false;
    }
    if let Ok(parent) = unsafe { GetParent(focus) } {
        if let Err(error) = unsafe {
            PostMessageW(
                Some(parent),
                if message.wParam.0 == 0x0d {
                    COMMIT_TEXT
                } else {
                    CANCEL_TEXT
                },
                WPARAM(0),
                LPARAM(0),
            )
        } {
            runtime::emit(UiEvent::Error(format!("发送文字编辑指令失败：{error}")));
        }
        return true;
    }
    false
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
