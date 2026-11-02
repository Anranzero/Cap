//! Physical desktop rectangles. Right and bottom edges are exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, String> {
        let rect = Self {
            x,
            y,
            width,
            height,
        };
        rect.validate()?;
        Ok(rect)
    }
    pub fn validate(self) -> Result<(), String> {
        if self.x as i64 + self.width as i64 > i32::MAX as i64
            || self.y as i64 + self.height as i64 > i32::MAX as i64
        {
            return Err("Rectangle edge exceeds physical coordinate range.".into());
        }
        Ok(())
    }
    pub fn right(self) -> i32 {
        (self.x as i64 + self.width as i64).min(i32::MAX as i64) as i32
    }
    pub fn bottom(self) -> i32 {
        (self.y as i64 + self.height as i64).min(i32::MAX as i64) as i32
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.validate().is_ok()
            && x >= self.x
            && y >= self.y
            && x < self.right()
            && y < self.bottom()
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: (self.right().min(other.right()) as i64 - x as i64).max(0) as u32,
            height: (self.bottom().min(other.bottom()) as i64 - y as i64).max(0) as u32,
        }
    }
    pub fn from_points(a: (i32, i32), b: (i32, i32)) -> Result<Self, String> {
        Self::new(
            a.0.min(b.0),
            a.1.min(b.1),
            (a.0 as i64 - b.0 as i64).unsigned_abs() as u32,
            (a.1 as i64 - b.1 as i64).unsigned_abs() as u32,
        )
    }
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}
fn finite(point: (f64, f64)) -> bool {
    point.0.is_finite() && point.1.is_finite()
}
pub fn drag(from: (f64, f64), to: (f64, f64), width: u32, height: u32) -> Result<Rect, String> {
    if !finite(from) || !finite(to) || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("Invalid selection coordinates.".into());
    }
    Rect::from_points(
        (
            from.0.clamp(0.0, width as f64) as i32,
            from.1.clamp(0.0, height as f64) as i32,
        ),
        (
            to.0.clamp(0.0, width as f64) as i32,
            to.1.clamp(0.0, height as f64) as i32,
        ),
    )
}
pub fn handles(r: Rect) -> [(f64, f64); 8] {
    let (x, y, w, h) = (r.x as f64, r.y as f64, r.width as f64, r.height as f64);
    [
        (x, y),
        (x + w / 2.0, y),
        (x + w, y),
        (x + w, y + h / 2.0),
        (x + w, y + h),
        (x + w / 2.0, y + h),
        (x, y + h),
        (x, y + h / 2.0),
    ]
}
pub fn hit(rect: Rect, point: (f64, f64), tolerance: f64) -> i32 {
    if rect.is_empty()
        || rect.validate().is_err()
        || !finite(point)
        || !tolerance.is_finite()
        || tolerance < 0.0
    {
        return -2;
    }
    for (i, h) in handles(rect).iter().enumerate() {
        if (h.0 - point.0).abs() <= tolerance && (h.1 - point.1).abs() <= tolerance {
            return i as i32;
        }
    }
    if rect.contains(point.0.floor() as i32, point.1.floor() as i32) {
        -1
    } else {
        -2
    }
}
pub fn adjust(
    original: Rect,
    start: (f64, f64),
    end: (f64, f64),
    handle: i32,
    width: u32,
    height: u32,
) -> Result<Rect, String> {
    original.validate()?;
    if original.is_empty()
        || original.x < 0
        || original.y < 0
        || original.right() as u32 > width
        || original.bottom() as u32 > height
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || !finite(start)
        || !finite(end)
        || !(-1..=7).contains(&handle)
    {
        return Err("Invalid selection adjustment.".into());
    }
    let dx = (end.0 - start.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    let dy = (end.1 - start.1)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i64;
    if handle == -1 {
        return Rect::new(
            (original.x as i64 + dx).clamp(0, (width - original.width) as i64) as i32,
            (original.y as i64 + dy).clamp(0, (height - original.height) as i64) as i32,
            original.width,
            original.height,
        );
    }
    let (mut l, mut t, mut r, mut b) = (
        original.x as i64,
        original.y as i64,
        original.right() as i64,
        original.bottom() as i64,
    );
    if matches!(handle, 0 | 6 | 7) {
        l = (l + dx).clamp(0, r - 1);
    }
    if matches!(handle, 0..=2) {
        t = (t + dy).clamp(0, b - 1);
    }
    if matches!(handle, 2..=4) {
        r = (r + dx).clamp(l + 1, width as i64);
    }
    if matches!(handle, 4..=6) {
        b = (b + dy).clamp(t + 1, height as i64);
    }
    Rect::new(l as i32, t as i32, (r - l) as u32, (b - t) as u32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_origin_and_exclusive_edges() {
        let r = Rect::new(-1920, -200, 1920, 1080).unwrap();
        assert!(r.contains(-100, 0));
        assert!(!r.contains(0, 0));
        assert_eq!(
            r.intersect(Rect::new(-50, 0, 100, 100).unwrap()),
            Rect::new(-50, 0, 50, 100).unwrap()
        );
        assert_eq!(
            Rect::from_points((80, 70), (-20, -10)).unwrap(),
            Rect::new(-20, -10, 100, 80).unwrap()
        );
    }
    #[test]
    fn overflowing_edges_are_rejected() {
        assert!(Rect::new(i32::MAX, 0, 1, 1).is_err());
    }
    #[test]
    fn selection_drag_handles_move_resize_and_invalid_input() {
        assert_eq!(
            drag((80.0, 70.0), (-20.0, -10.0), 100, 100).unwrap(),
            Rect::new(0, 0, 80, 70).unwrap()
        );
        let original = Rect::new(20, 30, 40, 50).unwrap();
        assert_eq!(
            adjust(original, (0.0, 0.0), (200.0, 200.0), -1, 100, 100).unwrap(),
            Rect::new(60, 50, 40, 50).unwrap()
        );
        for (i, p) in handles(original).into_iter().enumerate() {
            assert_eq!(hit(original, p, 2.0), i as i32);
            let r = adjust(original, p, (200.0, -200.0), i as i32, 100, 100).unwrap();
            assert!(!r.is_empty() && r.x >= 0 && r.y >= 0 && r.right() <= 100 && r.bottom() <= 100);
        }
        assert!(drag((f64::NAN, 0.0), (1.0, 1.0), 100, 100).is_err());
    }
}
