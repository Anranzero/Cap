//! Original Cap icon shapes on a 24-unit grid, rendered without a symbol font.
use std::{ffi::c_void, ptr};
use windows::Win32::{Foundation::RECT, Graphics::Gdi::HDC};

#[repr(C)]
struct StartupInput {
    version: u32,
    callback: *mut c_void,
    suppress_thread: i32,
    suppress_codecs: i32,
}
#[repr(C)]
struct Point {
    x: f32,
    y: f32,
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const StartupInput, output: *mut c_void) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipCreateFromHDC(dc: HDC, graphics: *mut *mut c_void) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipSetSmoothingMode(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipSetPixelOffsetMode(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipFlush(graphics: *mut c_void, intention: i32) -> i32;
    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut c_void) -> i32;
    fn GdipSetPenLineJoin(pen: *mut c_void, join: i32) -> i32;
    fn GdipSetPenWidth(pen: *mut c_void, width: f32) -> i32;
    fn GdipDeletePen(pen: *mut c_void) -> i32;
    fn GdipDrawLines(
        graphics: *mut c_void,
        pen: *mut c_void,
        points: *const Point,
        count: i32,
    ) -> i32;
    fn GdipDrawRectangle(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> i32;
    fn GdipDrawEllipse(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> i32;
    fn GdipDrawArc(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        start: f32,
        sweep: f32,
    ) -> i32;
    fn GdipCreateSolidFill(color: u32, brush: *mut *mut c_void) -> i32;
    fn GdipFillEllipse(
        graphics: *mut c_void,
        brush: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> i32;
    fn GdipDeleteBrush(brush: *mut c_void) -> i32;
}
fn checked(status: i32, action: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("工具图标{action}失败：GDI+ {status}"))
    }
}
fn argb(color: u32) -> u32 {
    0xff00_0000 | ((color & 255) << 16) | (color & 0xff00) | ((color >> 16) & 255)
}

pub(super) struct Renderer(usize);
impl Renderer {
    pub(super) fn new() -> Result<Self, String> {
        let mut token = 0;
        checked(
            unsafe {
                GdiplusStartup(
                    &mut token,
                    &StartupInput {
                        version: 1,
                        callback: ptr::null_mut(),
                        suppress_thread: 0,
                        suppress_codecs: 0,
                    },
                    ptr::null_mut(),
                )
            },
            "初始化",
        )?;
        Ok(Self(token))
    }
    pub(super) fn frame(&self, dc: HDC) -> Result<Graphics<'_>, String> {
        let mut raw = ptr::null_mut();
        checked(unsafe { GdipCreateFromHDC(dc, &mut raw) }, "创建画布")?;
        let frame = Graphics {
            raw,
            _renderer: self,
        };
        checked(unsafe { GdipSetSmoothingMode(frame.raw, 4) }, "设置抗锯齿")?;
        checked(
            unsafe { GdipSetPixelOffsetMode(frame.raw, 4) },
            "设置像素对齐",
        )?;
        Ok(frame)
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.0);
        }
    }
}
pub(super) struct Graphics<'a> {
    raw: *mut c_void,
    _renderer: &'a Renderer,
}
impl Drop for Graphics<'_> {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteGraphics(self.raw);
        }
    }
}
struct Pen(*mut c_void);
impl Drop for Pen {
    fn drop(&mut self) {
        unsafe {
            GdipDeletePen(self.0);
        }
    }
}
struct Brush(*mut c_void);
impl Drop for Brush {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteBrush(self.0);
        }
    }
}
struct Stroke<'a> {
    graphics: &'a Graphics<'a>,
    pen: Pen,
    x: f32,
    y: f32,
    scale: f32,
}
impl Stroke<'_> {
    fn poly(&self, points: &[(f32, f32)]) -> Result<(), String> {
        let points: Vec<Point> = points
            .iter()
            .map(|&(x, y)| Point {
                x: self.x + x * self.scale,
                y: self.y + y * self.scale,
            })
            .collect();
        checked(
            unsafe {
                GdipDrawLines(
                    self.graphics.raw,
                    self.pen.0,
                    points.as_ptr(),
                    points.len() as i32,
                )
            },
            "绘制线条",
        )
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
        checked(
            unsafe {
                GdipDrawRectangle(
                    self.graphics.raw,
                    self.pen.0,
                    self.x + x * self.scale,
                    self.y + y * self.scale,
                    w * self.scale,
                    h * self.scale,
                )
            },
            "绘制矩形",
        )
    }
    fn ellipse(&self, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
        checked(
            unsafe {
                GdipDrawEllipse(
                    self.graphics.raw,
                    self.pen.0,
                    self.x + x * self.scale,
                    self.y + y * self.scale,
                    w * self.scale,
                    h * self.scale,
                )
            },
            "绘制圆形",
        )
    }
    fn arc(&self, x: f32, y: f32, w: f32, h: f32, start: f32, sweep: f32) -> Result<(), String> {
        checked(
            unsafe {
                GdipDrawArc(
                    self.graphics.raw,
                    self.pen.0,
                    self.x + x * self.scale,
                    self.y + y * self.scale,
                    w * self.scale,
                    h * self.scale,
                    start,
                    sweep,
                )
            },
            "绘制弧线",
        )
    }
    fn width(&self, width: f32) -> Result<(), String> {
        checked(
            unsafe { GdipSetPenWidth(self.pen.0, width * self.scale) },
            "设置线宽",
        )
    }
    fn fill_ellipse(&self, x: f32, y: f32, w: f32, h: f32, color: u32) -> Result<(), String> {
        let mut raw = ptr::null_mut();
        checked(
            unsafe { GdipCreateSolidFill(argb(color), &mut raw) },
            "创建色块",
        )?;
        let brush = Brush(raw);
        checked(
            unsafe {
                GdipFillEllipse(
                    self.graphics.raw,
                    brush.0,
                    self.x + x * self.scale,
                    self.y + y * self.scale,
                    w * self.scale,
                    h * self.scale,
                )
            },
            "绘制色块",
        )
    }
}
impl Graphics<'_> {
    pub(super) fn finish(&self) -> Result<(), String> {
        checked(unsafe { GdipFlush(self.raw, 1) }, "提交绘制")
    }
    pub(super) fn icon(
        &self,
        index: usize,
        rect: RECT,
        color: u32,
        ink: u32,
        width: u32,
    ) -> Result<(), String> {
        // Feishu draws 24px icons on the same 24-unit grid.
        let scale = 1.0;
        let mut raw = ptr::null_mut();
        checked(
            unsafe { GdipCreatePen1(argb(color), 1.9 * scale, 2, &mut raw) },
            "创建画笔",
        )?;
        let pen = Pen(raw);
        checked(unsafe { GdipSetPenLineJoin(pen.0, 2) }, "设置连接")?;
        let s = Stroke {
            graphics: self,
            pen,
            x: rect.left as f32 + ((rect.right - rect.left) as f32 - 24.0) / 2.0,
            y: rect.top as f32 + ((rect.bottom - rect.top) as f32 - 24.0) / 2.0,
            scale,
        };
        match index {
            0 => {
                s.poly(&[(5., 3.), (18., 13.), (12., 14.), (9., 20.), (5., 3.)])?;
                s.poly(&[(12., 14.), (16., 21.)])?;
            }
            1 => s.rect(4., 5., 16., 14.)?,
            2 => s.ellipse(3., 5., 18., 14.)?,
            3 => {
                s.poly(&[(5., 19.), (19., 5.)])?;
                s.poly(&[(8., 5.), (19., 5.), (19., 16.)])?;
            }
            4 => s.poly(&[(5., 19.), (19., 5.)])?,
            5 => {
                s.poly(&[
                    (4., 17.),
                    (16., 5.),
                    (19., 8.),
                    (7., 20.),
                    (3., 21.),
                    (4., 17.),
                ])?;
                s.poly(&[(14., 7.), (17., 10.)])?;
            }
            6 => {
                s.poly(&[(4., 7.), (4., 4.), (20., 4.), (20., 7.)])?;
                s.poly(&[(12., 4.), (12., 20.)])?;
                s.poly(&[(8., 20.), (16., 20.)])?;
            }
            7 => {
                s.ellipse(3., 3., 18., 18.)?;
                s.poly(&[(10., 9.), (12., 7.), (12., 17.)])?;
                s.poly(&[(10., 17.), (14., 17.)])?;
            }
            8 => {
                for (x, y) in [(4., 4.), (15., 4.), (9.5, 9.5), (4., 15.), (15., 15.)] {
                    s.rect(x, y, 5., 5.)?;
                }
            }
            9 => {
                for (y, xs) in [
                    (6., &[4., 9., 14., 19.][..]),
                    (12., &[6., 11., 16.][..]),
                    (18., &[4., 9., 14., 19.][..]),
                ] {
                    for x in xs {
                        s.poly(&[(*x, y), (*x + 1., y)])?;
                    }
                }
            }
            10 => {
                s.poly(&[(9., 5.), (4., 10.), (9., 15.)])?;
                s.poly(&[(4., 10.), (14., 10.)])?;
                s.arc(8., 10., 12., 12., 270., 180.)?;
            }
            11 => {
                s.poly(&[(15., 5.), (20., 10.), (15., 15.)])?;
                s.poly(&[(20., 10.), (10., 10.)])?;
                s.arc(4., 10., 12., 12., 270., -180.)?;
            }
            12 => {
                s.poly(&[(8., 3.), (16., 3.)])?;
                s.poly(&[
                    (9., 3.),
                    (9., 8.),
                    (6., 13.),
                    (6., 15.),
                    (18., 15.),
                    (18., 13.),
                    (15., 8.),
                    (15., 3.),
                ])?;
                s.poly(&[(12., 15.), (12., 22.)])?;
            }
            13 => {
                s.poly(&[
                    (4., 3.),
                    (17., 3.),
                    (21., 7.),
                    (21., 21.),
                    (3., 21.),
                    (3., 3.),
                    (4., 3.),
                ])?;
                s.poly(&[(7., 3.), (7., 9.), (16., 9.), (16., 3.)])?;
                s.poly(&[(7., 21.), (7., 14.), (17., 14.), (17., 21.)])?;
            }
            14 => s.poly(&[(4., 12.), (9., 17.), (20., 6.)])?,
            15 => {
                s.poly(&[(6., 6.), (18., 18.)])?;
                s.poly(&[(6., 18.), (18., 6.)])?;
            }
            16 => {
                s.rect(2., 6., 14., 12.)?;
                s.poly(&[(16., 10.), (22., 6.), (22., 18.), (16., 14.)])?;
            }
            17 => {
                s.fill_ellipse(3., 5., 14., 14., ink)?;
                s.ellipse(3., 5., 14., 14.)?;
                s.poly(&[(19., 10.), (22., 13.), (25., 10.)])?;
            }
            18 => {
                s.width(width.min(8) as f32)?;
                s.poly(&[(3., 10.), (17., 10.)])?;
                s.width(1.9)?;
                s.poly(&[(18., 16.), (21., 19.), (24., 16.)])?;
            }
            // 19: 标注 / 高亮笔 (three slanted strokes)
            19 => {
                s.poly(&[(4., 20.), (9., 15.)])?;
                s.poly(&[(7., 22.), (12., 17.)])?;
                s.poly(&[(13., 10.), (20., 3.)])?;
                s.poly(&[(10., 7.), (17., 0.)])?;
            }
            // 20: 提取文字（工具栏 OCR）：角括号 + 文本行
            20 => {
                s.poly(&[(4., 8.), (4., 4.), (8., 4.)])?;
                s.poly(&[(16., 4.), (20., 4.), (20., 8.)])?;
                s.poly(&[(20., 16.), (20., 20.), (16., 20.)])?;
                s.poly(&[(8., 20.), (4., 20.), (4., 16.)])?;
                s.poly(&[(8., 9.), (16., 9.)])?;
                s.poly(&[(8., 12.), (16., 12.)])?;
                s.poly(&[(8., 15.), (13., 15.)])?;
            }
            // 24: 裁剪（裁切标记）
            24 => {
                s.poly(&[(7., 3.), (7., 17.), (3., 17.)])?;
                s.poly(&[(7., 3.), (7., 17.), (21., 17.)])?;
                s.poly(&[(3., 7.), (17., 7.), (17., 21.)])?;
                s.poly(&[(17., 7.), (21., 7.), (21., 21.), (17., 21.)])?;
            }
            // 23: 复制（虚线框 + 剪刀，飞书样式）
            23 => {
                s.poly(&[(4., 4.), (8., 4.)])?;
                s.poly(&[(12., 4.), (16., 4.)])?;
                s.poly(&[(20., 4.), (20., 8.)])?;
                s.poly(&[(20., 12.), (20., 16.)])?;
                s.poly(&[(20., 20.), (16., 20.)])?;
                s.poly(&[(12., 20.), (8., 20.)])?;
                s.poly(&[(4., 20.), (4., 16.)])?;
                s.poly(&[(4., 12.), (4., 8.)])?;
                s.poly(&[(8., 15.), (12., 9.)])?;
                s.poly(&[(12., 15.), (8., 9.)])?;
                s.ellipse(5.6, 15.2, 3.4, 3.4)?;
                s.ellipse(15., 15.2, 3.4, 3.4)?;
            }
            // 21: 自动滚动（2×2 点阵）
            21 => {
                s.rect(4., 4., 6., 6.)?;
                s.rect(14., 4., 6., 6.)?;
                s.rect(4., 14., 6., 6.)?;
                s.rect(14., 14., 6., 6.)?;
                s.poly(&[(11., 5.), (13., 7.)])?;
                s.poly(&[(11., 15.), (13., 17.)])?;
            }
            _ => return Err(format!("未知工具图标：{index}")),
        }
        Ok(())
    }
}
