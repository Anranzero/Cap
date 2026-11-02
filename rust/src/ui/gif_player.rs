//! Bounded GIF playback through the Windows GDI+ decoder.
use cap_rust::pixels::Frame;
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr};

#[repr(C)]
struct StartupInput {
    version: u32,
    debug: *mut c_void,
    suppress_thread: i32,
    suppress_codecs: i32,
}
#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const TIME: Guid = Guid {
    a: 0x6aedbd6d,
    b: 0x3fb5,
    c: 0x418a,
    d: [0x83, 0xa6, 0x7f, 0x45, 0x22, 0x9d, 0xc8, 0x72],
};
#[repr(C)]
struct PropertyItem {
    id: u32,
    length: u32,
    kind: u16,
    value: *const u8,
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const StartupInput, output: *mut c_void) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipLoadImageFromFile(path: *const u16, image: *mut *mut c_void) -> i32;
    fn GdipDisposeImage(image: *mut c_void) -> i32;
    fn GdipGetImageWidth(image: *mut c_void, width: *mut u32) -> i32;
    fn GdipGetImageHeight(image: *mut c_void, height: *mut u32) -> i32;
    fn GdipImageGetFrameCount(image: *mut c_void, dimension: *const Guid, count: *mut u32) -> i32;
    fn GdipImageSelectActiveFrame(image: *mut c_void, dimension: *const Guid, index: u32) -> i32;
    fn GdipGetPropertyItemSize(image: *mut c_void, id: u32, size: *mut u32) -> i32;
    fn GdipGetPropertyItem(image: *mut c_void, id: u32, size: u32, value: *mut c_void) -> i32;
    fn GdipCreateBitmapFromScan0(
        width: i32,
        height: i32,
        stride: i32,
        format: i32,
        pixels: *mut u8,
        bitmap: *mut *mut c_void,
    ) -> i32;
    fn GdipGetImageGraphicsContext(image: *mut c_void, graphics: *mut *mut c_void) -> i32;
    fn GdipGraphicsClear(graphics: *mut c_void, color: u32) -> i32;
    fn GdipDrawImageRectI(
        graphics: *mut c_void,
        image: *mut c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
}

fn check(status: i32, operation: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation}失败（GDI+ {status}）。"))
    }
}

/// Keeps one composed canvas in memory; the system decoder owns frame disposal.
pub struct GifPlayer {
    token: usize,
    image: *mut c_void,
    delays: Vec<u32>,
    index: usize,
    frame: Frame,
}

impl GifPlayer {
    pub fn new(path: &Path) -> Result<Self, String> {
        let input = StartupInput {
            version: 1,
            debug: ptr::null_mut(),
            suppress_thread: 0,
            suppress_codecs: 0,
        };
        let mut token = 0;
        unsafe {
            check(
                GdiplusStartup(&mut token, &input, ptr::null_mut()),
                "初始化 GIF 播放器",
            )?;
        }
        let mut player = Self {
            token,
            image: ptr::null_mut(),
            delays: vec![],
            index: 0,
            frame: Frame::blank(1, 1, [0; 4])?,
        };
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            check(
                GdipLoadImageFromFile(path.as_ptr(), &mut player.image),
                "读取 GIF",
            )?;
            let (mut width, mut height, mut count) = (0, 0, 0);
            check(GdipGetImageWidth(player.image, &mut width), "读取 GIF 宽度")?;
            check(
                GdipGetImageHeight(player.image, &mut height),
                "读取 GIF 高度",
            )?;
            check(
                GdipImageGetFrameCount(player.image, &TIME, &mut count),
                "读取 GIF 帧数",
            )?;
            if width == 0
                || height == 0
                || width as u64 * height as u64 > 134_217_728
                || count == 0
                || count > 1_000_000
            {
                return Err("GIF 尺寸或帧数超过支持范围。".into());
            }
            player.frame = Frame::blank(width, height, [0; 4])?;
            player.delays = vec![100; count as usize];
            let mut size = 0;
            // A missing delay property is legal for a one-frame GIF.
            let status = GdipGetPropertyItemSize(player.image, 0x5100, &mut size);
            if status == 0 {
                if (size as usize) < size_of::<PropertyItem>() || size > 4_000_064 {
                    return Err("GIF 帧延迟属性大小无效。".into());
                }
                let mut storage = vec![0u64; (size as usize).div_ceil(8)];
                check(
                    GdipGetPropertyItem(player.image, 0x5100, size, storage.as_mut_ptr().cast()),
                    "读取 GIF 帧延迟",
                )?;
                let property = &*storage.as_ptr().cast::<PropertyItem>();
                let begin = storage.as_ptr() as usize;
                let end = begin + size as usize;
                let value = property.value as usize;
                if property.kind != 4
                    || value < begin
                    || value
                        .checked_add(property.length as usize)
                        .is_none_or(|n| n > end)
                    || property.length < count.saturating_mul(4)
                {
                    return Err("GIF 帧延迟属性损坏。".into());
                }
                for (i, delay) in player.delays.iter_mut().enumerate() {
                    *delay = delay_ms(ptr::read_unaligned(property.value.add(i * 4).cast::<u32>()));
                }
            } else if status != 19 {
                return Err(format!("读取 GIF 帧延迟失败（GDI+ {status}）。"));
            }
            check(
                GdipImageSelectActiveFrame(player.image, &TIME, 0),
                "选择 GIF 首帧",
            )?;
        }
        player.render()?;
        Ok(player)
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }
    pub fn delay(&self) -> u32 {
        self.delays[self.index]
    }
    pub fn duration_ms(&self) -> u64 {
        self.delays.iter().map(|d| *d as u64).sum()
    }
    pub fn advance(&mut self) -> Result<(), String> {
        let next = (self.index + 1) % self.delays.len();
        unsafe {
            check(
                GdipImageSelectActiveFrame(self.image, &TIME, next as u32),
                "切换 GIF 帧",
            )?;
        }
        self.index = next;
        self.render()
    }

    fn render(&mut self) -> Result<(), String> {
        let mut bitmap = ptr::null_mut();
        let mut graphics = ptr::null_mut();
        unsafe {
            // PixelFormat32bppARGB stores straight-alpha BGRA on Windows.
            check(
                GdipCreateBitmapFromScan0(
                    self.frame.width as i32,
                    self.frame.height as i32,
                    self.frame.stride() as i32,
                    0x0026200a,
                    self.frame.pixels.as_mut_ptr(),
                    &mut bitmap,
                ),
                "创建 GIF 显示画布",
            )?;
            let result = (|| {
                check(
                    GdipGetImageGraphicsContext(bitmap, &mut graphics),
                    "创建 GIF 绘图上下文",
                )?;
                check(GdipGraphicsClear(graphics, 0xff131315), "清空 GIF 画布")?;
                check(
                    GdipDrawImageRectI(
                        graphics,
                        self.image,
                        0,
                        0,
                        self.frame.width as i32,
                        self.frame.height as i32,
                    ),
                    "合成 GIF 图像",
                )
            })();
            if !graphics.is_null() {
                GdipDeleteGraphics(graphics);
            }
            GdipDisposeImage(bitmap);
            result
        }
    }
}

impl Drop for GifPlayer {
    fn drop(&mut self) {
        unsafe {
            if !self.image.is_null() {
                GdipDisposeImage(self.image);
            }
            GdiplusShutdown(self.token);
        }
    }
}

fn delay_ms(centiseconds: u32) -> u32 {
    if centiseconds == 0 {
        100
    } else {
        centiseconds.saturating_mul(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cap-preview-{}-{}.gif",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut bytes =
            b"GIF89a\x02\x00\x01\x00\x81\x00\x00\x00\x00\x00\xff\x00\x00\x00\x00\xff\xff\xff\xff"
                .to_vec();
        for (left, width, delay, indices) in [(0u16, 2u16, 7u16, vec![1u8, 1]), (1, 1, 13, vec![2])]
        {
            bytes.extend_from_slice(&[0x21, 0xf9, 4, 4]);
            bytes.extend_from_slice(&delay.to_le_bytes());
            bytes.extend_from_slice(&[0, 0, 0x2c]);
            bytes.extend_from_slice(&left.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.push(0);
            let mut packed = 0u32;
            let mut bits = 0;
            for value in indices.into_iter().flat_map(|i| [4u8, i]).chain([5]) {
                packed |= (value as u32) << bits;
                bits += 3;
            }
            let count = (bits as usize).div_ceil(8);
            bytes.extend_from_slice(&[2, count as u8]);
            bytes.extend_from_slice(&packed.to_le_bytes()[..count]);
            bytes.push(0);
        }
        bytes.push(0x3b);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn delays_preserve_recorded_timing_and_zero_cannot_spin() {
        assert_eq!(delay_ms(7), 70);
        assert_eq!(delay_ms(0), 100);
        assert_eq!(delay_ms(u32::MAX), u32::MAX);
    }

    #[test]
    fn native_gif_composes_delta_frames_preserves_delays_and_loops() {
        let path = fixture();
        let mut player = GifPlayer::new(&path).unwrap();
        assert_eq!(player.frame().pixels, [0, 0, 255, 255].repeat(2));
        assert_eq!(player.delay(), 70);
        assert_eq!(player.duration_ms(), 200);
        player.advance().unwrap();
        assert_eq!(player.frame().pixels, [0, 0, 255, 255, 255, 0, 0, 255]);
        assert_eq!(player.delay(), 130);
        player.advance().unwrap();
        assert_eq!(player.frame().pixels, [0, 0, 255, 255].repeat(2));
        drop(player);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_gif_is_reported_without_leaving_file_locked() {
        let path = fixture();
        std::fs::write(&path, b"bad gif").unwrap();
        assert!(GifPlayer::new(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
