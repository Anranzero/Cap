//! Process-private HarmonyOS fonts shared by the UI thread's windows.
use std::cell::RefCell;
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

const FAMILY: &str = "HarmonyOS Sans SC";
const FONT_BYTES: &[u8] = include_bytes!("../../resources/fonts/HarmonyOS_Sans_SC_Regular.ttf");
pub(super) const LICENSE: &str = include_str!("../../resources/fonts/LICENSE.txt");

thread_local! {
    static FONTS: RefCell<Option<UiFonts>> = const { RefCell::new(None) };
}

struct UiFonts {
    normal: HFONT,
    compact: HFONT,
    title: HFONT,
    resource: HANDLE,
}
impl UiFonts {
    fn new() -> Result<Self, String> {
        let mut count = 0;
        let resource = unsafe {
            AddFontMemResourceEx(
                FONT_BYTES.as_ptr().cast(),
                FONT_BYTES.len() as u32,
                None,
                &raw mut count,
            )
        };
        if resource.is_invalid() {
            return Err(format!(
                "加载鸿蒙字体失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let mut fonts = Self {
            normal: HFONT::default(),
            compact: HFONT::default(),
            title: HFONT::default(),
            resource,
        };
        if count == 0 {
            return Err("鸿蒙字体文件未包含可用字体。".into());
        }
        fonts.normal = create(16, 400)?;
        fonts.compact = create(14, 500)?;
        fonts.title = create(22, 500)?;
        Ok(fonts)
    }
}
impl Drop for UiFonts {
    fn drop(&mut self) {
        unsafe {
            for font in [self.normal, self.compact, self.title] {
                if !font.is_invalid() && !DeleteObject(font.into()).as_bool() {
                    crate::platform::log_error("释放鸿蒙界面字体失败。");
                }
            }
            if !RemoveFontMemResourceEx(self.resource).as_bool() {
                crate::platform::log_error("释放鸿蒙字体资源失败。");
            }
        }
    }
}

fn create(height: i32, weight: i32) -> Result<HFONT, String> {
    let mut info = LOGFONTW {
        lfHeight: -height,
        lfWeight: weight,
        lfCharSet: DEFAULT_CHARSET,
        lfQuality: CLEARTYPE_QUALITY,
        ..Default::default()
    };
    for (dst, value) in info.lfFaceName.iter_mut().zip(FAMILY.encode_utf16()) {
        *dst = value;
    }
    let font = unsafe { CreateFontIndirectW(&info) };
    if font.is_invalid() {
        return Err(format!(
            "创建鸿蒙界面字体失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(font)
}

pub(super) fn initialize() -> Result<(), String> {
    FONTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(UiFonts::new()?);
        }
        Ok(())
    })
}
pub(super) fn normal() -> HFONT {
    FONTS.with(|slot| slot.borrow().as_ref().expect("UI fonts initialized").normal)
}
pub(super) fn title() -> HFONT {
    FONTS.with(|slot| slot.borrow().as_ref().expect("UI fonts initialized").title)
}
pub(super) fn compact() -> HFONT {
    FONTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("UI fonts initialized")
            .compact
    })
}
