use super::{CaptureTarget, monitors};
use crate::pixels::Frame;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
    core::{Interface, factory},
};
pub struct GraphicsSource {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    rt: IDirect3DDevice,
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: Option<GraphicsCaptureSession>,
    pool_size: SizeInt32,
    offset: (u32, u32),
    width: u32,
    height: u32,
    previous: Option<Frame>,
    staging: Option<ID3D11Texture2D>,
    staging_size: (u32, u32),
    closed: Arc<AtomicBool>,
    closed_token: i64,
    arrived_token: i64,
    arrived: Arc<(Mutex<bool>, Condvar)>,
    window_handle: isize,
}
struct FrameLease {
    frame: Direct3D11CaptureFrame,
    closed: bool,
}
impl FrameLease {
    fn new(frame: Direct3D11CaptureFrame) -> Self {
        Self {
            frame,
            closed: false,
        }
    }
    fn close(&mut self) -> windows::core::Result<()> {
        if !self.closed {
            self.frame.Close()?;
            self.closed = true;
        }
        Ok(())
    }
}
impl std::ops::Deref for FrameLease {
    type Target = Direct3D11CaptureFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}
impl Drop for FrameLease {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Returning Windows capture frame: {e}")
        }
    }
}
impl GraphicsSource {
    pub fn new(target: CaptureTarget) -> Result<Option<Self>, String> {
        Self::create(target).map_err(|e| format!("Windows Graphics Capture initialization: {e}"))
    }
    fn create(target: CaptureTarget) -> windows::core::Result<Option<Self>> {
        if !GraphicsCaptureSession::IsSupported()? {
            return Ok(None);
        }
        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let (item, offset): (GraphicsCaptureItem, (u32, u32)) = if target.window_handle != 0 {
            (
                unsafe { interop.CreateForWindow(HWND(target.window_handle as *mut _))? },
                (0, 0),
            )
        } else {
            let Some(m) = monitors()
                .map_err(|e| {
                    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e)
                })?
                .into_iter()
                .find(|m| target.bounds.intersect(m.bounds) == target.bounds)
            else {
                return Ok(None);
            };
            (
                unsafe {
                    interop.CreateForMonitor(windows::Win32::Graphics::Gdi::HMONITOR(
                        m.handle as *mut _,
                    ))?
                },
                (
                    (target.bounds.x - m.bounds.x) as u32,
                    (target.bounds.y - m.bounds.y) as u32,
                ),
            )
        };
        let mut device = None;
        let mut context = None;
        let hardware = unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        if hardware.is_err() {
            unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )?
            }
        }
        let device = device.unwrap();
        let context = context.unwrap();
        let dxgi: IDXGIDevice = device.cast()?;
        let rt: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        let pool_size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &rt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            pool_size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(false)?;
        let arrived = Arc::new((Mutex::new(false), Condvar::new()));
        let frame_signal = arrived.clone();
        let arrived_token =
            pool.FrameArrived(&windows::Foundation::TypedEventHandler::new(move |_, _| {
                if let Ok(mut ready) = frame_signal.0.lock() {
                    *ready = true;
                    frame_signal.1.notify_one();
                }
                Ok(())
            }))?;
        let closed = Arc::new(AtomicBool::new(false));
        let signal = closed.clone();
        let closed_token =
            item.Closed(&windows::Foundation::TypedEventHandler::new(move |_, _| {
                signal.store(true, Ordering::Release);
                Ok(())
            }))?;
        session.StartCapture()?;
        Ok(Some(Self {
            device,
            context,
            rt,
            item,
            pool,
            session: Some(session),
            pool_size,
            offset,
            width: target.bounds.width,
            height: target.bounds.height,
            previous: None,
            staging: None,
            staging_size: (0, 0),
            closed,
            closed_token,
            arrived_token,
            arrived,
            window_handle: target.window_handle,
        }))
    }
    pub fn capture(&mut self) -> Result<Frame, String> {
        self.read()
            .map_err(|e| format!("Windows Graphics Capture readback: {e}"))
    }
    pub fn target_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    fn read(&mut self) -> windows::core::Result<Frame> {
        pump_messages();
        if self.target_closed() {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80000013u32 as i32),
                super::TARGET_CLOSED,
            ));
        }
        let start = Instant::now();
        let mut frame = loop {
            match self.pool.TryGetNextFrame() {
                Ok(f) => break FrameLease::new(f),
                Err(e) => {
                    if self.target_closed() {
                        return Err(e);
                    }
                    if let Some(p) = &self.previous {
                        return Ok(p.clone());
                    }
                    if start.elapsed() > Duration::from_millis(1500) {
                        return Err(e);
                    }
                    if let Ok(ready) = self.arrived.0.lock() {
                        let _ = self
                            .arrived
                            .1
                            .wait_timeout(ready, Duration::from_millis(10));
                    }
                }
            }
        };
        while let Ok(newer) = self.pool.TryGetNextFrame() {
            frame.close()?;
            frame = FrameLease::new(newer);
        }
        let size = frame.ContentSize()?;
        let surface = frame.Surface()?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
        let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        if self.staging_size != (desc.Width, desc.Height) {
            let d = D3D11_TEXTURE2D_DESC {
                Width: desc.Width,
                Height: desc.Height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            unsafe {
                self.device
                    .CreateTexture2D(&d, None, Some(&mut self.staging))?
            }
            self.staging_size = (desc.Width, desc.Height);
        }
        let staging = self.staging.as_ref().unwrap();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context.CopyResource(staging, &texture);
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        }
        let mut pixels = vec![0u8; self.width as usize * self.height as usize * 4];
        let source_w = desc.Width.min(size.Width.max(0) as u32);
        let source_h = desc.Height.min(size.Height.max(0) as u32);
        let copy_w = self.width.min(source_w.saturating_sub(self.offset.0));
        let copy_h = self.height.min(source_h.saturating_sub(self.offset.1));
        unsafe {
            for row in 0..copy_h as usize {
                let src = (mapped.pData as *const u8).add(
                    (row + self.offset.1 as usize) * mapped.RowPitch as usize
                        + self.offset.0 as usize * 4,
                );
                std::ptr::copy_nonoverlapping(
                    src,
                    pixels.as_mut_ptr().add(row * self.width as usize * 4),
                    copy_w as usize * 4,
                );
            }
            self.context.Unmap(staging, 0);
        }
        drop(texture);
        drop(access);
        drop(surface);
        frame.close()?;
        drop(frame);
        if size.Width > 0 && size.Height > 0 && size != self.pool_size {
            self.pool.Recreate(
                &self.rt,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )?;
            self.pool_size = size;
        }
        for a in pixels.iter_mut().skip(3).step_by(4) {
            *a = 255
        }
        let output = Frame {
            width: self.width,
            height: self.height,
            pixels,
        };
        self.previous = Some(output.clone());
        Ok(output)
    }
}
impl Drop for GraphicsSource {
    fn drop(&mut self) {
        // Item.Closed is delivered through this creator thread's native queue,
        // including on an MTA. Dispatch it before the synchronous stop RPC.
        pump_messages();
        if self.window_handle != 0 && !super::is_window(self.window_handle) {
            let start = Instant::now();
            while !self.target_closed() && start.elapsed() < Duration::from_millis(1500) {
                pump_messages();
                std::thread::sleep(Duration::from_millis(2));
            }
            if !self.target_closed() {
                eprintln!("WGC target closure notification timed out before shutdown");
            }
        }
        if let Err(e) = self.item.RemoveClosed(self.closed_token) {
            eprintln!("Removing capture target closure handler: {e}")
        }
        if let Err(e) = self.pool.RemoveFrameArrived(self.arrived_token) {
            eprintln!("Removing capture frame handler: {e}")
        }
        self.staging = None;
        unsafe {
            self.context.ClearState();
            self.context.Flush();
        }
        if let Some(session) = self.session.take() {
            if let Err(e) = session.Close() {
                eprintln!("Closing capture session: {e}")
            }
            drop(session);
        }
        if let Err(e) = self.pool.Close() {
            eprintln!("Closing capture frame pool: {e}")
        }
    }
}
fn pump_messages() {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let mut message = MSG::default();
        let mut quit = None;
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            if message.message == WM_QUIT {
                quit = Some(message.wParam.0 as i32);
                continue;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if let Some(code) = quit {
            PostQuitMessage(code);
        }
    }
}
