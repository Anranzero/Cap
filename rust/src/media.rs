//! Windows codecs and offline OCR. Media Foundation samples use 100 ns units.
use crate::pixels::Frame;
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Graphics::Imaging::*,
        System::Com::*,
    },
    core::PCWSTR,
};
mod mp4;
pub use mp4::{DecodedVideo, Mp4Writer, VideoDecoder, decode_video};
pub(crate) fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
pub(crate) struct ComGuard {
    initialized: bool,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl ComGuard {
    pub fn new() -> Result<Self, String> {
        let hr = windows::core::HRESULT(unsafe { RoInitialize(1) });
        if hr.0 == 0x80010106u32 as i32 {
            Ok(Self {
                initialized: false,
                _thread: std::marker::PhantomData,
            })
        } else {
            hr.ok().map_err(|e| format!("Initializing COM: {e}"))?;
            Ok(Self {
                initialized: true,
                _thread: std::marker::PhantomData,
            })
        }
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { RoUninitialize() }
        }
    }
}
#[link(name = "runtimeobject")]
unsafe extern "system" {
    fn RoInitialize(kind: u32) -> i32;
    fn RoUninitialize();
}
fn factory() -> Result<IWICImagingFactory, String> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| format!("Creating Windows image codec: {e}"))
}
pub fn save_image(path: &Path, frame: &Frame) -> Result<(), String> {
    frame.validate()?;
    let _com = ComGuard::new()?;
    if path.exists() {
        return Err("Image output already exists".into());
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let jpeg = matches!(ext.as_str(), "jpg" | "jpeg");
    if !jpeg && ext != "png" {
        return Err("Image extension must be PNG or JPG".into());
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.{}.partial",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("Reserving image output: {e}"))?;
    drop(reservation);
    let p = wide(&temporary);
    let f = factory()?;
    let result = (|| -> windows::core::Result<()> {
        unsafe {
            let stream = f.CreateStream()?;
            stream.InitializeFromFilename(PCWSTR(p.as_ptr()), GENERIC_WRITE.0)?;
            let encoder = f.CreateEncoder(
                if jpeg {
                    &GUID_ContainerFormatJpeg
                } else {
                    &GUID_ContainerFormatPng
                },
                std::ptr::null(),
            )?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut output = None;
            let mut options = None;
            encoder.CreateNewFrame(&mut output, &mut options)?;
            let output = output.unwrap();
            output.Initialize(options.as_ref())?;
            output.SetSize(frame.width, frame.height)?;
            let mut format = if jpeg {
                GUID_WICPixelFormat24bppBGR
            } else {
                GUID_WICPixelFormat32bppBGRA
            };
            output.SetPixelFormat(&mut format)?;
            let bitmap = f.CreateBitmapFromMemory(
                frame.width,
                frame.height,
                &GUID_WICPixelFormat32bppBGRA,
                frame.width * 4,
                &frame.pixels,
            )?;
            let converter = f.CreateFormatConverter()?;
            converter.Initialize(
                &bitmap,
                &format,
                WICBitmapDitherTypeNone,
                None,
                0.,
                WICBitmapPaletteTypeCustom,
            )?;
            output.WriteSource(&converter, std::ptr::null())?;
            output.Commit()?;
            encoder.Commit()?;
            Ok(())
        }
    })();
    let result = result
        .map_err(|e| format!("Saving {}: {e}", path.display()))
        .and_then(|_| crate::recovery::publish(&temporary, path));
    if result.is_err() {
        if let Err(e) = std::fs::remove_file(&temporary) {
            eprintln!("Removing failed image output {}: {e}", temporary.display())
        }
    }
    result
}
pub fn load_image(path: &Path) -> Result<Frame, String> {
    let _com = ComGuard::new()?;
    let p = wide(path);
    let f = factory()?;
    (|| -> windows::core::Result<Frame> {
        unsafe {
            let decoder = f.CreateDecoderFromFilename(
                PCWSTR(p.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )?;
            let frame = decoder.GetFrame(0)?;
            let converter = f.CreateFormatConverter()?;
            converter.Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.,
                WICBitmapPaletteTypeCustom,
            )?;
            let (mut width, mut height) = (0, 0);
            converter.GetSize(&mut width, &mut height)?;
            if width == 0 || height == 0 || width as u64 * height as u64 > 134_217_728 {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80070057u32 as i32),
                    "Image dimensions exceed supported limit",
                ));
            }
            let mut pixels = vec![0; width as usize * height as usize * 4];
            converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
            Ok(Frame {
                width,
                height,
                pixels,
            })
        }
    })()
    .map_err(|e| format!("Decoding {}: {e}", path.display()))
}
pub fn ocr(frame: &Frame) -> Result<String, String> {
    use windows::{
        Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
    };
    frame.validate()?;
    let _com = ComGuard::new()?;
    (|| -> windows::core::Result<String> {
        let limit = OcrEngine::MaxImageDimension()?;
        let scale = 1f64.min(limit as f64 / frame.width.max(frame.height) as f64);
        let mut frame = if scale < 1.0 {
            frame
                .resize_bilinear(
                    (frame.width as f64 * scale).round().max(1.) as u32,
                    (frame.height as f64 * scale).round().max(1.) as u32,
                )
                .map_err(|e| {
                    windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), e)
                })?
        } else {
            frame.clone()
        };
        for pixel in frame.pixels.chunks_exact_mut(4) {
            let alpha = pixel[3] as u32;
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&frame.pixels)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            frame.width as i32,
            frame.height as i32,
            BitmapAlphaMode::Ignore,
        )?;
        let engine = match OcrEngine::TryCreateFromUserProfileLanguages() {
            Ok(e) => e,
            Err(profile_error) => {
                let available = OcrEngine::AvailableRecognizerLanguages()?;
                let mut languages = Vec::new();
                for i in 0..available.Size()? {
                    let language = available.GetAt(i)?;
                    let tag = language.LanguageTag()?.to_string().to_ascii_lowercase();
                    languages.push((
                        if tag.starts_with("zh") {
                            0
                        } else if tag.starts_with("en") {
                            1
                        } else {
                            2
                        },
                        language,
                    ));
                }
                languages.sort_by_key(|(priority, _)| *priority);
                let mut found = None;
                for (_, language) in languages {
                    if let Ok(e) = OcrEngine::TryCreateFromLanguage(&language) {
                        found = Some(e);
                        break;
                    }
                }
                found.ok_or(profile_error)?
            }
        };
        let result = engine.RecognizeAsync(&bitmap)?.get()?;
        let lines = result.Lines()?;
        let mut output = Vec::new();
        for i in 0..lines.Size()? {
            output.push(lines.GetAt(i)?.Text()?.to_string());
        }
        Ok(output.join("\r\n"))
    })()
    .map_err(|e| {
        format!("Windows offline OCR failed (install a Windows OCR language if missing): {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires an installed Windows OCR language; run explicitly for native acceptance"]
    fn native_ocr_recognizes_owned_text() {
        use windows::{
            Win32::{Foundation::COLORREF, Graphics::Gdi::*},
            core::w,
        };
        let dib = crate::capture::Dib::new(640, 120).unwrap();
        let white = Frame::blank(640, 120, [255, 255, 255, 255]).unwrap();
        dib.copy_from(&white).unwrap();
        unsafe {
            let dc = HDC(dib.dc as *mut _);
            let font = CreateFontW(
                -52,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe UI"),
            );
            assert!(!font.is_invalid());
            let old = SelectObject(dc, font.into());
            SetTextColor(dc, COLORREF(0));
            SetBkColor(dc, COLORREF(0x00ffffff));
            SetBkMode(dc, OPAQUE);
            let text = "CAPTURE 123".encode_utf16().collect::<Vec<_>>();
            assert!(TextOutW(dc, 20, 20, &text).as_bool());
            SelectObject(dc, old);
            let _ = DeleteObject(font.into());
        }
        let text = ocr(&dib.frame().unwrap()).unwrap();
        assert!(text.contains("123"), "OCR result: {text}");
    }
    #[test]
    fn wic_png_roundtrip() {
        let p = std::env::temp_dir().join(format!("cap-rust-codec-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let pixels = vec![
            10, 20, 200, 255, 180, 40, 60, 255, 80, 100, 30, 255, 9, 8, 7, 255,
        ];
        let frame = Frame::new(2, 2, pixels).unwrap();
        save_image(&p, &frame).unwrap();
        assert_eq!(load_image(&p).unwrap().pixels, frame.pixels);
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn mp4_decodes_top_down_and_duration() {
        let p = std::env::temp_dir().join(format!("cap-rust-video-{}.mp4", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let mut frame = Frame::new(160, 96, vec![0; 160 * 96 * 4]).unwrap();
        for y in 0..96 {
            for x in 0..160 {
                let i = (y * 160 + x) * 4;
                frame.pixels[i + if y < 48 { 2 } else { 0 }] = 255;
                frame.pixels[i + 3] = 255;
            }
        }
        {
            let mut w = Mp4Writer::new(&p, 160, 96, 10).unwrap();
            for n in 0..10 {
                w.write(&frame, n * 1_000_000, 1_000_000).unwrap();
            }
            w.finish().unwrap();
            w.finish().unwrap();
        }
        let d = decode_video(&p).unwrap();
        assert_eq!(d.frame_count, 10);
        assert!((d.duration_ticks - 10_000_000).abs() < 100_000);
        assert!(d.first.pixels[(12 * 160 + 80) * 4 + 2] > 200);
        assert!(d.first.pixels[(80 * 160 + 80) * 4] > 200);
        let mut preview = VideoDecoder::new(&p).unwrap();
        assert!((preview.duration_ticks - 10_000_000).abs() < 100_000);
        assert!(preview.seek(-1).is_err());
        preview.seek(5_000_000).unwrap();
        assert!(preview.next_frame().unwrap().is_some());
        assert!(preview.last_time + preview.last_duration > 5_000_000);
        preview.seek(0).unwrap();
        assert!(preview.next_frame().unwrap().is_some());
        assert_eq!(preview.last_time, 0);
        drop(preview);
        std::fs::remove_file(p).unwrap();
    }
}
