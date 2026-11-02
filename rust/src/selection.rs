use crate::geometry::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    Screenshot,
    Scrolling,
    Recording,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionAction {
    None,
    Cancel,
    Copy,
    Save,
    Pin,
    Record(Rect),
    Scroll(Rect),
    Ocr(Rect),
}

pub struct SelectionModel {
    pub desktop: Rect,
    pub selected: Option<Rect>,
    pub mode: CaptureMode,
}

impl SelectionModel {
    pub fn new(desktop: Rect, mode: CaptureMode) -> Self {
        Self {
            desktop,
            selected: None,
            mode,
        }
    }
    pub fn choose(&mut self, local: Rect) -> Result<SelectionAction, String> {
        let checked = Rect::new(local.x, local.y, local.width, local.height)?;
        if checked.is_empty()
            || checked.x < 0
            || checked.y < 0
            || checked.right() as u32 > self.desktop.width
            || checked.bottom() as u32 > self.desktop.height
        {
            return Err("选区必须位于桌面图像内。".into());
        }
        self.selected = Some(checked);
        Ok(SelectionAction::None)
    }
    pub fn commit(&self) -> Result<SelectionAction, String> {
        let r = self.selected.ok_or("请先选择区域。")?;
        let physical = Rect::new(
            self.desktop.x.checked_add(r.x).ok_or("选区坐标溢出。")?,
            self.desktop.y.checked_add(r.y).ok_or("选区坐标溢出。")?,
            r.width,
            r.height,
        )?;
        Ok(match self.mode {
            CaptureMode::Screenshot => SelectionAction::Copy,
            CaptureMode::Scrolling => SelectionAction::Scroll(physical),
            CaptureMode::Recording => SelectionAction::Record(physical),
            CaptureMode::Text => SelectionAction::Ocr(physical),
        })
    }
    pub fn move_by(&mut self, dx: i32, dy: i32) -> Result<(), String> {
        let r = self.selected.ok_or("请先选择区域。")?;
        let x = (r.x as i64 + dx as i64).clamp(0, (self.desktop.width - r.width) as i64) as i32;
        let y = (r.y as i64 + dy as i64).clamp(0, (self.desktop.height - r.height) as i64) as i32;
        self.selected = Some(Rect::new(x, y, r.width, r.height)?);
        Ok(())
    }
    pub fn cycle_mode(&mut self, direction: i32) {
        let index = match self.mode {
            CaptureMode::Screenshot => 0,
            CaptureMode::Scrolling => 1,
            CaptureMode::Recording => 2,
            CaptureMode::Text => 3,
        };
        self.mode = match (index + direction).rem_euclid(4) {
            0 => CaptureMode::Screenshot,
            1 => CaptureMode::Scrolling,
            2 => CaptureMode::Recording,
            _ => CaptureMode::Text,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choosing_a_region_waits_for_action_and_preserves_negative_desktop_origin() {
        let mut model = SelectionModel::new(
            Rect {
                x: -1920,
                y: -200,
                width: 3840,
                height: 1280,
            },
            CaptureMode::Recording,
        );
        assert_eq!(
            model
                .choose(Rect {
                    x: 50,
                    y: 60,
                    width: 100,
                    height: 80
                })
                .unwrap(),
            SelectionAction::None
        );
        assert_eq!(
            model.commit().unwrap(),
            SelectionAction::Record(Rect {
                x: -1870,
                y: -140,
                width: 100,
                height: 80
            })
        );
        model.mode = CaptureMode::Screenshot;
        assert_eq!(model.commit().unwrap(), SelectionAction::Copy);
        model.mode = CaptureMode::Text;
        assert_eq!(
            model.commit().unwrap(),
            SelectionAction::Ocr(Rect {
                x: -1870,
                y: -140,
                width: 100,
                height: 80
            })
        );
    }
    #[test]
    fn keyboard_move_clamps_to_image_and_bad_regions_are_rejected() {
        let mut model = SelectionModel::new(
            Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 80,
            },
            CaptureMode::Screenshot,
        );
        assert!(
            model
                .choose(Rect {
                    x: 95,
                    y: 10,
                    width: 10,
                    height: 10
                })
                .is_err()
        );
        model
            .choose(Rect {
                x: 10,
                y: 10,
                width: 40,
                height: 30,
            })
            .unwrap();
        model.move_by(-100, 100).unwrap();
        assert_eq!(
            model.selected,
            Some(Rect {
                x: 0,
                y: 50,
                width: 40,
                height: 30
            })
        );
        model.cycle_mode(1);
        assert_eq!(model.mode, CaptureMode::Scrolling);
        model.cycle_mode(-1);
        assert_eq!(model.mode, CaptureMode::Screenshot);
    }
}
