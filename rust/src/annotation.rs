//! Raster annotations in physical image pixels; text is supplied by the native UI as a stamp.
use crate::{geometry::Rect, pixels::Frame};
use std::collections::VecDeque;
#[path = "annotation/effects.rs"]
mod effects;
#[path = "annotation/raster.rs"]
mod raster;
pub use effects::{blur, pixelate};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: [u8; 4],
    pub thickness: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Pixelate,
    Blur,
    /// Coarse square blocks (Feishu 块状).
    Block,
}
#[derive(Clone, Debug)]
pub enum PaintCommand {
    Rectangle {
        start: Point,
        end: Point,
        style: Style,
    },
    Ellipse {
        start: Point,
        end: Point,
        style: Style,
    },
    Line {
        start: Point,
        end: Point,
        style: Style,
    },
    Arrow {
        start: Point,
        end: Point,
        style: Style,
    },
    Pen {
        points: Vec<Point>,
        style: Style,
    },
    Number {
        center: Point,
        number: u32,
        style: Style,
    },
    Stamp {
        origin: Point,
        frame: Frame,
    },
}
impl PaintCommand {
    fn storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Pen { points, .. } => points.capacity() * std::mem::size_of::<Point>(),
                Self::Stamp { frame, .. } => frame.pixels.capacity(),
                _ => 0,
            }
    }
    fn is_number(&self) -> bool {
        matches!(self, Self::Number { .. })
    }
    fn validate(&self) -> Result<(), String> {
        let point = |p: Point| {
            p.x.is_finite()
                && p.y.is_finite()
                && p.x.abs() <= i32::MAX as f64
                && p.y.abs() <= i32::MAX as f64
        };
        let style = |s: Style| {
            s.thickness.is_finite() && s.thickness > 0.0 && s.thickness <= i32::MAX as f64
        };
        let valid = match self {
            Self::Rectangle {
                start,
                end,
                style: s,
            }
            | Self::Ellipse {
                start,
                end,
                style: s,
            }
            | Self::Line {
                start,
                end,
                style: s,
            }
            | Self::Arrow {
                start,
                end,
                style: s,
            } => point(*start) && point(*end) && style(*s),
            Self::Pen { points, style: s } => {
                !points.is_empty() && points.iter().all(|p| point(*p)) && style(*s)
            }
            Self::Number {
                center,
                number,
                style: s,
            } => point(*center) && *number > 0 && style(*s),
            Self::Stamp { origin, frame } => {
                frame.validate()?;
                point(*origin)
            }
        };
        if valid {
            Ok(())
        } else {
            Err("Invalid annotation coordinates, number or stroke width.".into())
        }
    }
}
pub struct AnnotationDocument {
    base: Frame,
    commands: VecDeque<PaintCommand>,
    redo: Vec<PaintCommand>,
    history_bytes: usize,
    max_operations: usize,
    max_bytes: usize,
    baked_numbers: u32,
}
impl AnnotationDocument {
    pub fn new(frame: Frame) -> Result<Self, String> {
        Self::with_history_limits(frame, 80, 128 * 1024 * 1024)
    }
    pub fn with_history_limits(
        frame: Frame,
        operations: usize,
        bytes: usize,
    ) -> Result<Self, String> {
        frame.validate()?;
        if operations == 0 || bytes == 0 {
            return Err("Annotation history limits must be nonzero.".into());
        }
        Ok(Self {
            base: frame,
            commands: VecDeque::new(),
            redo: Vec::new(),
            history_bytes: 0,
            max_operations: operations,
            max_bytes: bytes,
            baked_numbers: 0,
        })
    }
    pub fn add(&mut self, command: PaintCommand) -> Result<(), String> {
        command.validate()?;
        for old in self.redo.drain(..) {
            self.history_bytes -= old.storage_bytes();
        }
        self.history_bytes += command.storage_bytes();
        self.commands.push_back(command);
        while self.commands.len() > self.max_operations || self.history_bytes > self.max_bytes {
            if let Some(first) = self.commands.pop_front() {
                raster::draw(&mut self.base, &first);
                self.history_bytes -= first.storage_bytes();
                if first.is_number() {
                    self.baked_numbers = self.baked_numbers.saturating_add(1);
                }
            }
        }
        Ok(())
    }
    pub fn render(&self) -> Result<Frame, String> {
        let mut frame = self.base.copy()?;
        for command in &self.commands {
            raster::draw(&mut frame, command);
        }
        Ok(frame)
    }
    pub fn export(&self, rect: Rect) -> Result<Frame, String> {
        self.render()?.crop(rect)
    }
    pub fn effect(&self, effect: Effect, rect: Rect, amount: u32) -> Result<PaintCommand, String> {
        let mut frame = self.export(rect)?;
        match effect {
            Effect::Pixelate => pixelate(&mut frame, amount.saturating_mul(4).max(8))?,
            Effect::Blur => blur(&mut frame, amount.saturating_mul(2).max(3))?,
            Effect::Block => pixelate(&mut frame, amount.saturating_mul(12).max(16))?,
        }
        Ok(PaintCommand::Stamp {
            origin: Point::new(rect.x as f64, rect.y as f64),
            frame,
        })
    }
    pub fn undo(&mut self) -> bool {
        if let Some(command) = self.commands.pop_back() {
            self.redo.push(command);
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(command) = self.redo.pop() {
            self.commands.push_back(command);
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.commands.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn next_number(&self) -> u32 {
        self.baked_numbers
            .saturating_add(self.commands.iter().filter(|c| c.is_number()).count() as u32)
            .saturating_add(1)
    }
    pub fn history_bytes(&self) -> usize {
        self.history_bytes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn style() -> Style {
        Style {
            color: [0, 0, 255, 255],
            thickness: 3.0,
        }
    }
    fn line() -> PaintCommand {
        PaintCommand::Line {
            start: Point::new(3.0, 4.0),
            end: Point::new(20.0, 4.0),
            style: style(),
        }
    }
    #[test]
    fn undo_redo_branch_and_bake_are_lossless() {
        let base = Frame::blank(32, 24, [255, 0, 0, 255]).unwrap();
        let mut doc = AnnotationDocument::new(base.clone()).unwrap();
        doc.add(line()).unwrap();
        let drawn = doc.render().unwrap();
        assert_ne!(drawn, base);
        assert!(doc.undo());
        assert_eq!(doc.render().unwrap(), base);
        assert!(doc.can_redo());
        assert!(doc.redo());
        assert_eq!(doc.render().unwrap(), drawn);
        assert!(doc.undo());
        doc.add(line()).unwrap();
        assert!(!doc.can_redo());
        for _ in 0..85 {
            doc.add(line()).unwrap();
        }
        let baked = doc.render().unwrap();
        let mut count = 0;
        while doc.undo() {
            count += 1;
        }
        assert_eq!(count, 80);
        assert_ne!(doc.render().unwrap(), base);
        while doc.redo() {}
        assert_eq!(doc.render().unwrap(), baked);
    }
    #[test]
    fn shape_tools_and_stamp_change_pixels() {
        let a = Point::new(20.0, 20.0);
        let b = Point::new(90.0, 60.0);
        let s = style();
        for command in [
            PaintCommand::Rectangle {
                start: a,
                end: b,
                style: s,
            },
            PaintCommand::Ellipse {
                start: a,
                end: b,
                style: s,
            },
            PaintCommand::Arrow {
                start: a,
                end: b,
                style: s,
            },
            line(),
            PaintCommand::Pen {
                points: vec![a, b],
                style: s,
            },
            PaintCommand::Number {
                center: a,
                number: 12,
                style: s,
            },
            PaintCommand::Stamp {
                origin: a,
                frame: Frame::blank(3, 3, [0, 255, 0, 128]).unwrap(),
            },
        ] {
            let base = Frame::blank(120, 80, [255, 0, 0, 255]).unwrap();
            let mut d = AnnotationDocument::new(base.clone()).unwrap();
            d.add(command).unwrap();
            assert_ne!(d.render().unwrap(), base);
            assert!(d.undo());
            assert_eq!(d.render().unwrap(), base);
        }
    }
    #[test]
    fn effect_edges_and_alpha_match_reference() {
        let mut f = Frame::new(
            3,
            1,
            vec![0, 20, 40, 255, 100, 120, 140, 255, 200, 220, 240, 255],
        )
        .unwrap();
        let mut p = f.clone();
        pixelate(&mut p, 2).unwrap();
        assert_eq!(p.pixels[0], 50);
        assert_eq!(p.pixels[4], 50);
        assert_eq!(p.pixels[8], 200);
        blur(&mut f, 1).unwrap();
        assert_eq!([f.pixels[0], f.pixels[4], f.pixels[8]], [50, 100, 150]);
        assert_eq!(f.pixels[3], 255);
        let mut single = Frame::blank(1, 1, [10, 20, 30, 255]).unwrap();
        blur(&mut single, 30).unwrap();
        assert_eq!(single.pixels, [10, 20, 30, 255]);
    }
    #[test]
    fn effects_include_annotations_and_invalid_input_is_atomic() {
        let mut d =
            AnnotationDocument::new(Frame::blank(32, 24, [255, 0, 0, 255]).unwrap()).unwrap();
        d.add(line()).unwrap();
        let r = Rect::new(0, 0, 32, 24).unwrap();
        let before = d.render().unwrap();
        for effect in [Effect::Pixelate, Effect::Blur] {
            d.add(d.effect(effect, r, 4).unwrap()).unwrap();
            assert_ne!(d.render().unwrap(), before);
            d.undo();
            assert_eq!(d.render().unwrap(), before);
        }
        assert!(
            d.add(PaintCommand::Line {
                start: Point::new(f64::NAN, 0.0),
                end: Point::new(1.0, 1.0),
                style: style()
            })
            .is_err()
        );
        assert_eq!(d.render().unwrap(), before);
    }
    #[test]
    fn history_budget_and_number_sequence_remain_bounded() {
        let mut d = AnnotationDocument::with_history_limits(
            Frame::blank(32, 24, [0, 0, 0, 255]).unwrap(),
            80,
            128,
        )
        .unwrap();
        for n in 1..5 {
            d.add(PaintCommand::Number {
                center: Point::new(10.0, 10.0),
                number: n,
                style: style(),
            })
            .unwrap();
        }
        assert_eq!(d.next_number(), 5);
        assert!(d.history_bytes() <= 128);
        d.add(PaintCommand::Stamp {
            origin: Point::new(0.0, 0.0),
            frame: Frame::blank(32, 24, [255, 0, 0, 255]).unwrap(),
        })
        .unwrap();
        assert!(d.history_bytes() <= 128);
        assert!(!d.can_undo());
        assert_eq!(d.render().unwrap().pixels[0], 255);
    }
    #[test]
    fn straight_alpha_stamp_composites_without_losing_background() {
        let mut d = AnnotationDocument::new(Frame::blank(1, 1, [0, 0, 0, 255]).unwrap()).unwrap();
        d.add(PaintCommand::Stamp {
            origin: Point::new(0.0, 0.0),
            frame: Frame::blank(1, 1, [255, 255, 255, 128]).unwrap(),
        })
        .unwrap();
        assert_eq!(d.render().unwrap().pixels, [128, 128, 128, 255]);
    }
    #[test]
    fn history_accounts_for_retained_allocation_capacity() {
        let mut pixels = Vec::with_capacity(1024);
        pixels.extend_from_slice(&[255, 0, 0, 255]);
        let mut d = AnnotationDocument::with_history_limits(
            Frame::blank(1, 1, [0, 0, 0, 255]).unwrap(),
            80,
            128,
        )
        .unwrap();
        d.add(PaintCommand::Stamp {
            origin: Point::new(0.0, 0.0),
            frame: Frame::new(1, 1, pixels).unwrap(),
        })
        .unwrap();
        assert!(
            !d.can_undo(),
            "Retained stamp capacity must count toward the memory budget"
        );
        assert_eq!(d.render().unwrap().pixels, [255, 0, 0, 255]);
    }
}
