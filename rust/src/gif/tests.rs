use super::*;
#[derive(Debug)]
struct Image {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    delay: u16,
    pixels: Vec<u8>,
}
fn word(data: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([data[p], data[p + 1]])
}
// Independent decoder: dictionary entries are byte strings, unlike encoder prefix keys.
fn lzw(data: &[u8], minimum: u8) -> Vec<u8> {
    let clear = 1usize << minimum;
    let end = clear + 1;
    let mut table: Vec<Vec<u8>> = Vec::new();
    let mut bits = minimum as usize + 1;
    let mut bit = 0;
    let mut prev: Option<Vec<u8>> = None;
    let mut out = Vec::new();
    loop {
        assert!(bit + bits <= data.len() * 8, "truncated LZW stream");
        let mut code = 0usize;
        for b in 0..bits {
            code |= (((data[(bit + b) / 8] >> ((bit + b) % 8)) & 1) as usize) << b;
        }
        bit += bits;
        if code == clear {
            table = (0..clear).map(|i| vec![i as u8]).collect();
            table.extend([vec![], vec![]]);
            bits = minimum as usize + 1;
            prev = None;
            continue;
        }
        if code == end {
            break;
        }
        let entry = if code < table.len() {
            table[code].clone()
        } else {
            assert_eq!(code, table.len());
            let mut v = prev.clone().expect("missing LZW prefix");
            v.push(v[0]);
            v
        };
        assert!(!entry.is_empty());
        out.extend_from_slice(&entry);
        match prev {
            Some(mut p) if table.len() < 4096 => {
                p.push(entry[0]);
                table.push(p);
                if table.len() == 1 << bits && bits < 12 {
                    bits += 1;
                }
            }
            _ => {}
        }
        prev = Some(entry);
    }
    out
}
fn decode(data: &[u8]) -> Vec<Image> {
    assert_eq!(&data[..6], b"GIF89a");
    let mut p = 13 + 3 * (1usize << ((data[10] & 7) + 1));
    let mut delay = 0;
    let mut images = Vec::new();
    loop {
        match data[p] {
            0x21 => {
                let tag = data[p + 1];
                p += 2;
                if tag == 0xf9 {
                    assert_eq!(data[p], 4);
                    delay = word(data, p + 2);
                }
                loop {
                    let n = data[p] as usize;
                    p += 1;
                    if n == 0 {
                        break;
                    }
                    p += n;
                }
            }
            0x2c => {
                let left = word(data, p + 1);
                let top = word(data, p + 3);
                let width = word(data, p + 5);
                let height = word(data, p + 7);
                let packed = data[p + 9];
                assert_eq!(packed & 0x40, 0);
                p += 10;
                if packed & 0x80 != 0 {
                    p += 3 * (1usize << ((packed & 7) + 1));
                }
                let minimum = data[p];
                p += 1;
                let mut compressed = Vec::new();
                loop {
                    let n = data[p] as usize;
                    p += 1;
                    if n == 0 {
                        break;
                    }
                    compressed.extend_from_slice(&data[p..p + n]);
                    p += n;
                }
                let pixels = lzw(&compressed, minimum);
                assert_eq!(pixels.len(), width as usize * height as usize);
                images.push(Image {
                    left,
                    top,
                    width,
                    height,
                    delay,
                    pixels,
                });
            }
            0x3b => break,
            other => panic!("bad GIF block {other:x}"),
        }
    }
    images
}
fn noise() -> Frame {
    let mut f = Frame::blank(160, 100, [0, 0, 0, 255]).unwrap();
    let mut h = 713u32;
    for p in f.pixels.chunks_exact_mut(4) {
        h = h.wrapping_mul(1664525).wrapping_add(1013904223);
        p[0] = ((h & 3) * 85) as u8;
        p[1] = (((h >> 8) & 7) * 255 / 7) as u8;
        p[2] = (((h >> 16) & 7) * 255 / 7) as u8;
    }
    f
}
#[test]
fn round_trip_noise_lzw_delta_and_repeat_delays() {
    let f = noise();
    let mut bytes = Vec::new();
    let mut e = GifEncoder::new(&mut bytes, 160, 100, 30).unwrap();
    e.write(&f, 0, 333333).unwrap();
    e.write(&f, 333333, 333333).unwrap();
    let mut changed = f.clone();
    let p = (50 * 160 + 70) * 4;
    changed.pixels[p..p + 4].copy_from_slice(&[0, 0, 255, 255]);
    e.write(&changed, 666666, 333334).unwrap();
    e.write(&changed, 1000000, 1000000).unwrap();
    e.finish().unwrap();
    drop(e);
    let frames = decode(&bytes);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].delay, 7);
    assert_eq!(
        (
            frames[1].left,
            frames[1].top,
            frames[1].width,
            frames[1].height,
            frames[1].delay
        ),
        (70, 50, 1, 1, 13)
    );
    if let Some(directory) = std::env::var_os("CAP_GIF_FIXTURES") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("algorithm-roundtrip.gif"), &bytes).unwrap();
        std::fs::write(directory.join("algorithm-roundtrip.bgra"), &f.pixels).unwrap();
    }
    for (i, &idx) in frames[0].pixels.iter().enumerate() {
        assert_eq!(((idx >> 5) as u16 * 255 / 7) as u8, f.pixels[i * 4 + 2]);
        assert_eq!(
            (((idx >> 2) & 7) as u16 * 255 / 7) as u8,
            f.pixels[i * 4 + 1]
        );
        assert_eq!((idx & 3) * 85, f.pixels[i * 4]);
    }
}
#[test]
fn sixty_fps_cumulative_rounding_is_exact() {
    let mut bytes = Vec::new();
    let mut e = GifEncoder::new(&mut bytes, 2, 2, 60).unwrap();
    for i in 0..60 {
        let a = i * 10000000 / 60;
        let b = (i + 1) * 10000000 / 60;
        e.write(
            &Frame::blank(2, 2, [0, 0, (i % 2 * 255) as u8, 255]).unwrap(),
            a,
            b - a,
        )
        .unwrap();
    }
    e.finish().unwrap();
    drop(e);
    assert_eq!(
        decode(&bytes).iter().map(|f| f.delay as u32).sum::<u32>(),
        100
    );
}
#[test]
fn invalid_and_non_monotonic_times_fail() {
    let f = Frame::blank(2, 2, [0; 4]).unwrap();
    assert!(GifEncoder::new(Vec::new(), 0, 2, 30).is_err());
    let mut e = GifEncoder::new(Vec::new(), 2, 2, 30).unwrap();
    assert!(e.write(&f, -1, 1).is_err());
    assert!(e.write(&f, 0, 0).is_err());
    e.write(&f, 10, 10).unwrap();
    assert!(e.write(&f, 0, 10).is_err());
    e.finish().unwrap();
    assert!(e.write(&f, 20, 10).is_err());
}
#[test]
fn long_hold_splits_delay_without_overflow() {
    let mut e = GifEncoder::new(Vec::new(), 1, 1, 30).unwrap();
    e.write(
        &Frame::blank(1, 1, [0, 0, 255, 255]).unwrap(),
        0,
        7000000000,
    )
    .unwrap();
    let bytes = e.into_inner().unwrap();
    let frames = decode(&bytes);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames.iter().map(|i| i.delay as u32).sum::<u32>(), 70000);
}
#[test]
fn dither_gradient_average_and_temporal_stability() {
    let mut f = Frame::blank(256, 64, [0, 0, 0, 255]).unwrap();
    for y in 0..64 {
        for x in 0..256 {
            f.pixels[(y * 256 + x) * 4] = x as u8;
        }
    }
    let mut e = GifEncoder::new(Vec::new(), 256, 64, 30).unwrap();
    e.write(&f, 0, 1000000).unwrap();
    e.write(&f, 1000000, 1000000).unwrap();
    let frames = decode(&e.into_inner().unwrap());
    assert_eq!(frames.len(), 1);
    for x in (4..252).step_by(8) {
        let mut sum = 0u32;
        for y in 0..64 {
            for dx in 0..8 {
                sum += (frames[0].pixels[y * 256 + x + dx - 4] & 3) as u32 * 85;
            }
        }
        assert!((sum as f64 / 512.0 - (x as f64 - 0.5)).abs() < 4.0);
    }
}
struct FailAfter {
    remaining: usize,
}
impl Write for FailAfter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::Error::other("simulated disk failure"));
        }
        let n = self.remaining.min(data.len());
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn output_errors_and_empty_movie_are_reported() {
    assert!(
        GifEncoder::new(Vec::new(), 2, 2, 30)
            .unwrap()
            .finish()
            .is_err()
    );
    let mut e = GifEncoder::new(FailAfter { remaining: 820 }, 2, 2, 30).unwrap();
    e.write(&Frame::blank(2, 2, [0, 0, 0, 255]).unwrap(), 0, 1000000)
        .unwrap();
    assert!(e.finish().is_err());
    assert!(e.finish().is_err());
}
