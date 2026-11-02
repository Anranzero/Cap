use super::*;
use std::time::Duration;

struct GifFixture(PathBuf);

impl GifFixture {
    fn new() -> Self {
        Self::with_delays([2, 2, 2])
    }

    fn with_delays(delays: [u16; 3]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cap-gif-timing-{}-{}.gif",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut bytes =
            b"GIF89a\x01\x00\x01\x00\x81\x00\x00\xff\x00\x00\x00\xff\x00\x00\x00\xff\xff\xff\xff"
                .to_vec();
        for (index, delay) in (0u8..3).zip(delays) {
            bytes.extend_from_slice(&[0x21, 0xf9, 4, 4]);
            bytes.extend_from_slice(&delay.to_le_bytes());
            bytes.extend_from_slice(&[0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
            let packed = 4u16 | ((index as u16) << 3) | (5 << 6);
            bytes.extend_from_slice(&[2, 2]);
            bytes.extend_from_slice(&packed.to_le_bytes());
            bytes.push(0);
        }
        bytes.push(0x3b);
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }

    fn player(&self, now: Instant) -> Player {
        let player = GifPlayer::new(&self.0).unwrap();
        let delay = Duration::from_millis(player.delay() as u64);
        Player::Gif {
            player,
            next: now + delay,
            paused_at: None,
        }
    }
}

impl Drop for GifFixture {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

#[test]
fn gif_timer_keeps_recorded_speed_across_late_callbacks() {
    let fixture = GifFixture::new();
    let now = Instant::now();
    let mut player = fixture.player(now);
    let red = [0, 0, 255, 255];
    let green = [0, 255, 0, 255];
    let blue = [255, 0, 0, 255];
    assert_eq!(player.frame().pixels, red);
    for (milliseconds, changed, pixels) in [
        (15, false, red),
        (30, true, green),
        (45, true, blue),
        (60, true, red),
        (75, false, red),
        (90, true, green),
        (105, true, blue),
        (120, true, red),
    ] {
        assert_eq!(
            player
                .tick(now + Duration::from_millis(milliseconds))
                .unwrap(),
            changed,
            "frame change at {milliseconds}ms"
        );
        assert_eq!(
            player.frame().pixels,
            pixels,
            "visible pixels at {milliseconds}ms"
        );
    }
}

#[test]
fn gif_pause_resume_preserves_the_remaining_frame_time() {
    let fixture = GifFixture::new();
    let now = Instant::now();
    let mut player = fixture.player(now);
    assert!(player.tick(now + Duration::from_millis(25)).unwrap());
    assert_eq!(player.frame().pixels, [0, 255, 0, 255]);
    player.toggle_at(now + Duration::from_millis(30));
    assert!(!player.playing());
    assert!(!player.tick(now + Duration::from_millis(900)).unwrap());
    assert_eq!(player.frame().pixels, [0, 255, 0, 255]);
    player.toggle_at(now + Duration::from_millis(1000));
    assert!(player.playing());
    assert!(!player.tick(now + Duration::from_millis(1009)).unwrap());
    assert!(player.tick(now + Duration::from_millis(1010)).unwrap());
    assert_eq!(player.frame().pixels, [255, 0, 0, 255]);
    assert!(player.tick(now + Duration::from_millis(1030)).unwrap());
    assert_eq!(player.frame().pixels, [0, 0, 255, 255]);
}

#[test]
fn gif_timer_preserves_variable_delays_and_loop_boundaries() {
    let fixture = GifFixture::with_delays([2, 3, 5]);
    let now = Instant::now();
    let mut player = fixture.player(now);
    let red = [0, 0, 255, 255];
    let green = [0, 255, 0, 255];
    let blue = [255, 0, 0, 255];
    for (milliseconds, changed, pixels) in [
        (19, false, red),
        (25, true, green),
        (50, true, blue),
        (99, false, blue),
        (100, true, red),
        (125, true, green),
        (155, true, blue),
    ] {
        assert_eq!(
            player
                .tick(now + Duration::from_millis(milliseconds))
                .unwrap(),
            changed,
            "frame change at {milliseconds}ms"
        );
        assert_eq!(player.frame().pixels, pixels);
    }
}

#[test]
fn gif_stall_catches_up_in_bounded_batches_without_resetting_timing() {
    let fixture = GifFixture::new();
    let now = Instant::now();
    let mut player = fixture.player(now);
    let stalled = now + Duration::from_millis(5025);
    let mut batches = 0;
    while player.tick(stalled).unwrap() {
        batches += 1;
        assert!(batches < 300, "GIF catch-up must make progress");
    }
    assert!(batches > 1, "long stalls must yield between decode batches");
    assert_eq!(player.frame().pixels, [255, 0, 0, 255]);
    assert!(!player.tick(now + Duration::from_millis(5039)).unwrap());
    assert!(player.tick(now + Duration::from_millis(5040)).unwrap());
    assert_eq!(player.frame().pixels, [0, 0, 255, 255]);
}
