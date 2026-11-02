use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
pub(crate) struct NativeTarget {
    pub handle: isize,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl NativeTarget {
    pub fn new() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "capture::native_tests::target_child",
                "--ignored",
                "--nocapture",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut target = Self {
            handle: 0,
            child,
            input,
            output,
        };
        target.handle = target.line("CAP_WINDOW=").parse().unwrap();
        target
    }
    fn line(&mut self, prefix: &str) -> String {
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "Native target ended before {prefix}"
            );
            if let Some(text) = line.trim().strip_prefix(prefix) {
                return text.into();
            }
        }
    }
    pub fn move_resize(&mut self) {
        writeln!(self.input, "MOVE").unwrap();
        self.input.flush().unwrap();
        self.line("CAP_MOVED");
    }
    pub fn shrink(&mut self) {
        writeln!(self.input, "SHRINK").unwrap();
        self.input.flush().unwrap();
        self.line("CAP_SHRUNK");
    }
    pub fn close(&mut self) {
        writeln!(self.input, "CLOSE").unwrap();
        self.input.flush().unwrap();
        self.line("CAP_CLOSED");
        let start = Instant::now();
        while self.child.try_wait().unwrap().is_none() {
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "Native target did not exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for NativeTarget {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
#[test]
#[ignore = "Native target helper; invoked only by child-process acceptance harness"]
fn target_child() {
    // Keep the child in per-monitor DPI coordinates so MOVE targets physical pixels.
    let _dpi = DpiScope::new();
    let mut window = super::test_window::TestWindow::new(140, 140, 160, 96).unwrap();
    println!("CAP_WINDOW={}", window.handle);
    std::io::stdout().flush().unwrap();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        match line.unwrap().as_str() {
            "MOVE" => {
                window.move_resize(180, 180, 240, 144).unwrap();
                println!("CAP_MOVED");
                std::io::stdout().flush().unwrap()
            }
            "SHRINK" => {
                window.move_resize(180, 180, 80, 48).unwrap();
                println!("CAP_SHRUNK");
                std::io::stdout().flush().unwrap()
            }
            "CLOSE" => {
                window.close();
                println!("CAP_CLOSED");
                std::io::stdout().flush().unwrap();
                break;
            }
            _ => break,
        }
    }
}
#[test]
#[ignore = "WGC child scenario; the parent applies an 8-second watchdog"]
fn capture_child_scenario() {
    assert!(
        windows::Graphics::Capture::GraphicsCaptureSession::IsSupported().unwrap(),
        "Strict WGC acceptance requires Windows Graphics Capture support"
    );
    let _com = crate::media::ComGuard::new().unwrap();
    let _dpi = DpiScope::new();
    let mut target = NativeTarget::new();
    let bounds = window_bounds(target.handle).unwrap();
    let gdi = screenshot(bounds).unwrap();
    let i = (gdi.height as usize / 2 * gdi.width as usize + gdi.width as usize / 2) * 4;
    assert_eq!(&gdi.pixels[i..i + 4], &[0x20, 0x40, 0xe0, 255]);
    let mut source = CaptureSession::new(CaptureTarget {
        bounds,
        window_handle: target.handle,
        prefer_graphics_capture: true,
    })
    .unwrap();
    assert_eq!(
        source.backend_name(),
        "Windows Graphics Capture",
        "Strict WGC acceptance used a fallback: {:?}",
        source.fallback_reason
    );
    let first = wait_for_wgc_pixels(
        &mut source,
        &[
            (8, 8, [255, 255, 255, 255]),
            (40, 24, [0x20, 0x40, 0xe0, 255]),
            (150, 24, [200, 40, 30, 255]),
            (40, 86, [20, 180, 40, 255]),
        ],
    );
    assert_eq!((first.width, first.height), (bounds.width, bounds.height));
    eprintln!(
        "WGC scenario backend: {} {:?}",
        source.backend_name(),
        source.fallback_reason
    );
    let case = std::env::var("CAP_RUST_WGC_CASE").unwrap();
    if case == "resized" {
        target.move_resize();
        let frame = wait_for_wgc_pixels(
            &mut source,
            &[
                (8, 8, [255, 255, 255, 255]),
                (150, 24, [0x20, 0x40, 0xe0, 255]),
                (40, 86, [0x20, 0x40, 0xe0, 255]),
                (159, 95, [0x20, 0x40, 0xe0, 255]),
            ],
        );
        assert_eq!((frame.width, frame.height), (bounds.width, bounds.height));
        let moved = source.bounds().unwrap();
        assert_eq!((moved.x, moved.y), (180, 180));
        target.shrink();
        let shrunk = wait_for_wgc_pixels(
            &mut source,
            &[
                (8, 8, [255, 255, 255, 255]),
                (24, 8, [0x20, 0x40, 0xe0, 255]),
                (70, 8, [200, 40, 30, 255]),
                (24, 30, [20, 180, 40, 255]),
                (100, 20, [0, 0, 0, 255]),
                (20, 60, [0, 0, 0, 255]),
                (159, 95, [0, 0, 0, 255]),
            ],
        );
        assert_eq!((shrunk.width, shrunk.height), (bounds.width, bounds.height));
    }
    if case != "normal" {
        target.close();
        assert_eq!(source.capture().unwrap_err(), TARGET_CLOSED);
    }
    eprintln!("WGC scenario {case}: releasing source");
    drop(source);
    eprintln!("WGC scenario {case}: source released");
    if case == "normal" {
        target.close();
    }
}
fn wait_for_wgc_pixels(
    source: &mut CaptureSession,
    expected: &[(u32, u32, [u8; 4])],
) -> crate::pixels::Frame {
    let start = Instant::now();
    loop {
        let frame = source.capture().unwrap();
        assert_eq!(
            source.backend_name(),
            "Windows Graphics Capture",
            "Capture fell back during strict WGC acceptance: {:?}",
            source.fallback_reason
        );
        let actual = expected
            .iter()
            .map(|&(x, y, _)| {
                let index = (y as usize * frame.width as usize + x as usize) * 4;
                <[u8; 4]>::try_from(&frame.pixels[index..index + 4]).unwrap()
            })
            .collect::<Vec<_>>();
        if expected
            .iter()
            .zip(&actual)
            .all(|((_, _, expected), actual)| expected == actual)
        {
            return frame;
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "WGC pixel probes did not match: expected={expected:?}, actual={actual:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub(crate) fn run_cases() {
    for case in ["normal", "closed", "resized"] {
        run_child_case("capture::native_tests::capture_child_scenario", Some(case));
    }
}
pub(crate) fn run_child_case(test: &str, case: Option<&str>) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(case) = case {
        command.env("CAP_RUST_WGC_CASE", case);
    }
    let mut child = command.spawn().unwrap();
    let start = Instant::now();
    let mut timed_out = false;
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(8) {
            timed_out = true;
            child.kill().unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    eprintln!(
        "Native child {case:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !timed_out && output.status.success(),
        "Native child {test} {case:?} timed_out={timed_out}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
