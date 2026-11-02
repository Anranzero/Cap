use super::{PaintCommand, Point, Style};
use crate::pixels::Frame;

fn blend(pixel: &mut [u8], color: [u8; 4], coverage: u32) {
    let alpha = (color[3] as u32 * coverage + 2) / 4;
    if alpha == 0 {
        return;
    }
    let old = pixel[3] as u32;
    let weighted = alpha * 255 + old * (255 - alpha);
    for c in 0..3 {
        pixel[c] =
            ((color[c] as u32 * alpha * 255 + pixel[c] as u32 * old * (255 - alpha) + weighted / 2)
                / weighted) as u8;
    }
    pixel[3] = ((weighted + 127) / 255) as u8;
}
fn paint(
    frame: &mut Frame,
    bounds: (f64, f64, f64, f64),
    color: [u8; 4],
    inside: impl Fn(f64, f64) -> bool,
) {
    let left = bounds.0.floor().max(0.0).min(frame.width as f64) as usize;
    let top = bounds.1.floor().max(0.0).min(frame.height as f64) as usize;
    let right = bounds.2.ceil().max(0.0).min(frame.width as f64) as usize;
    let bottom = bounds.3.ceil().max(0.0).min(frame.height as f64) as usize;
    for y in top..bottom {
        for x in left..right {
            let mut coverage = 0;
            for dy in [0.25, 0.75] {
                for dx in [0.25, 0.75] {
                    coverage += u32::from(inside(x as f64 + dx, y as f64 + dy));
                }
            }
            let p = (y * frame.width as usize + x) * 4;
            blend(&mut frame.pixels[p..p + 4], color, coverage);
        }
    }
}
fn disk(frame: &mut Frame, center: Point, r: f64, color: [u8; 4]) {
    paint(
        frame,
        (center.x - r, center.y - r, center.x + r, center.y + r),
        color,
        |x, y| (x - center.x).hypot(y - center.y) <= r,
    );
}
fn line(frame: &mut Frame, a: Point, b: Point, style: Style) {
    let r = style.thickness / 2.0;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx * dx + dy * dy;
    paint(
        frame,
        (
            a.x.min(b.x) - r,
            a.y.min(b.y) - r,
            a.x.max(b.x) + r,
            a.y.max(b.y) + r,
        ),
        style.color,
        |x, y| {
            let t = if len == 0.0 {
                0.0
            } else {
                ((x - a.x) * dx + (y - a.y) * dy) / len
            }
            .clamp(0.0, 1.0);
            (x - a.x - t * dx).hypot(y - a.y - t * dy) <= r
        },
    );
}
fn triangle(frame: &mut Frame, a: Point, b: Point, c: Point, color: [u8; 4]) {
    let cross =
        |u: Point, v: Point, x: f64, y: f64| (v.x - u.x) * (y - u.y) - (v.y - u.y) * (x - u.x);
    paint(
        frame,
        (
            a.x.min(b.x).min(c.x),
            a.y.min(b.y).min(c.y),
            a.x.max(b.x).max(c.x),
            a.y.max(b.y).max(c.y),
        ),
        color,
        |x, y| {
            let d = [cross(a, b, x, y), cross(b, c, x, y), cross(c, a, x, y)];
            d.iter().all(|v| *v >= 0.0) || d.iter().all(|v| *v <= 0.0)
        },
    );
}
fn rectangle(frame: &mut Frame, a: Point, b: Point, style: Style) {
    let c = Point::new(b.x, a.y);
    let d = Point::new(a.x, b.y);
    for (s, e) in [(a, c), (c, b), (b, d), (d, a)] {
        line(frame, s, e, style);
    }
}
fn ellipse(frame: &mut Frame, a: Point, b: Point, style: Style) {
    let center = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let (rx, ry) = ((a.x - b.x).abs() / 2.0, (a.y - b.y).abs() / 2.0);
    let r = style.thickness / 2.0;
    if rx == 0.0 || ry == 0.0 {
        line(frame, a, b, style);
        return;
    }
    let (outer_x, outer_y) = (rx + r, ry + r);
    let (inner_x, inner_y) = ((rx - r).max(0.0), (ry - r).max(0.0));
    paint(
        frame,
        (
            center.x - outer_x,
            center.y - outer_y,
            center.x + outer_x,
            center.y + outer_y,
        ),
        style.color,
        |x, y| {
            let (dx, dy) = (x - center.x, y - center.y);
            (dx / outer_x).powi(2) + (dy / outer_y).powi(2) <= 1.0
                && (inner_x == 0.0
                    || inner_y == 0.0
                    || (dx / inner_x).powi(2) + (dy / inner_y).powi(2) >= 1.0)
        },
    );
}
fn number(frame: &mut Frame, center: Point, value: u32, style: Style) {
    let radius = 13.0f64.max(style.thickness * 4.0);
    disk(frame, center, radius, style.color);
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    let text = value.to_string();
    let columns = text.len() * 4 - 1;
    let scale = (radius * 1.25 / 5.0).min(radius * 1.65 / columns as f64);
    let left = center.x - columns as f64 * scale / 2.0;
    let top = center.y - 5.0 * scale / 2.0;
    for (n, digit) in text.bytes().enumerate() {
        for (row, bits) in DIGITS[(digit - b'0') as usize].iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) != 0 {
                    let x = left + (n * 4 + col) as f64 * scale;
                    let y = top + row as f64 * scale;
                    paint(frame, (x, y, x + scale, y + scale), [255; 4], |xx, yy| {
                        xx >= x && xx < x + scale && yy >= y && yy < y + scale
                    });
                }
            }
        }
    }
}
fn stamp(frame: &mut Frame, origin: Point, source: &Frame) {
    let (ox, oy) = (origin.x.round() as i64, origin.y.round() as i64);
    let left = ox.max(0).min(frame.width as i64);
    let top = oy.max(0).min(frame.height as i64);
    let right = (ox + source.width as i64).clamp(0, frame.width as i64);
    let bottom = (oy + source.height as i64).clamp(0, frame.height as i64);
    for y in top..bottom {
        for x in left..right {
            let s = (((y - oy) * source.width as i64 + x - ox) * 4) as usize;
            let d = ((y * frame.width as i64 + x) * 4) as usize;
            blend(
                &mut frame.pixels[d..d + 4],
                source.pixels[s..s + 4]
                    .try_into()
                    .expect("validated BGRA pixel"),
                4,
            );
        }
    }
}
pub(super) fn draw(frame: &mut Frame, command: &PaintCommand) {
    match command {
        PaintCommand::Rectangle { start, end, style } => rectangle(frame, *start, *end, *style),
        PaintCommand::Ellipse { start, end, style } => ellipse(frame, *start, *end, *style),
        PaintCommand::Line { start, end, style } => line(frame, *start, *end, *style),
        PaintCommand::Arrow { start, end, style } => {
            line(frame, *start, *end, *style);
            let len = (end.x - start.x).hypot(end.y - start.y);
            if len >= 1.0 {
                let (dx, dy) = ((end.x - start.x) / len, (end.y - start.y) / len);
                let size = 12.0f64.max(style.thickness * 4.0);
                let base = Point::new(end.x - dx * size, end.y - dy * size);
                triangle(
                    frame,
                    *end,
                    Point::new(base.x - dy * size * 0.45, base.y + dx * size * 0.45),
                    Point::new(base.x + dy * size * 0.45, base.y - dx * size * 0.45),
                    style.color,
                );
            }
        }
        PaintCommand::Pen { points, style } => {
            if points.len() == 1 {
                disk(frame, points[0], style.thickness / 2.0, style.color);
            }
            for pair in points.windows(2) {
                line(frame, pair[0], pair[1], *style);
            }
        }
        PaintCommand::Number {
            center,
            number: value,
            style,
        } => number(frame, *center, *value, *style),
        PaintCommand::Stamp {
            origin,
            frame: source,
        } => stamp(frame, *origin, source),
    }
}
