//! Full-screen capture through Windows Graphics Capture.
//!
//! The primary monitor is captured whole. There is no window or region picker:
//! the caller wants whatever the current screen share is showing.

#![cfg(windows)]

use std::sync::mpsc::channel;
use std::time::Duration;

use talkcer_core::{CoreError, ScreenCapturer, ScreenFrame};
use windows::core::{IInspectable, Interface};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;

/// Captures the primary monitor.
#[derive(Debug, Default)]
pub struct WindowsScreen;

impl WindowsScreen {
    /// Build a capturer.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ScreenCapturer for WindowsScreen {
    async fn capture(&self) -> Result<ScreenFrame, CoreError> {
        // The capture callbacks arrive on a Windows thread, so the grab itself
        // runs on a blocking thread and reports back one frame.
        tokio::task::spawn_blocking(grab_primary)
            .await
            .map_err(|error| CoreError::Screen(format!("capture task failed: {error}")))?
    }
}

fn grab_primary() -> Result<ScreenFrame, CoreError> {
    let (device, inspectable) = direct3d_device()?;
    let item = primary_monitor()?;
    let size = item.Size().map_err(screen_error)?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &inspectable,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(screen_error)?;
    let (sender, receiver) = channel();
    let pool_for_frame = pool.clone();
    pool.FrameArrived(&windows::Foundation::TypedEventHandler::<
        Direct3D11CaptureFramePool,
        IInspectable,
    >::new(move |pool, _| {
        if let Some(pool) = pool.as_ref() {
            if let Ok(frame) = pool.TryGetNextFrame() {
                let _ = sender.send(frame);
            }
        }
        Ok(())
    }))
    .map_err(screen_error)?;
    let session = pool_for_frame.CreateCaptureSession(&item).map_err(screen_error)?;
    session.StartCapture().map_err(screen_error)?;
    let frame = receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| CoreError::Screen("no frame arrived".to_owned()))?;
    let captured = copy_frame(&frame, &device)?;
    session.Close().ok();
    pool.Close().ok();
    let _ = device;
    Ok(captured)
}

/// The primary monitor as a capture item.
fn primary_monitor() -> Result<GraphicsCaptureItem, CoreError> {
    let interop = windows::core::factory::<
        windows::Graphics::Capture::GraphicsCaptureItem,
        windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop,
    >()
    .map_err(screen_error)?;
    // Enum the first output the primary adapter exposes, which is the current
    // screen. A window or region is never selected.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(screen_error)?;
    let adapter = unsafe { factory.EnumAdapters1(0) }.map_err(screen_error)?;
    let output = unsafe { adapter.EnumOutputs(0) }.map_err(screen_error)?;
    let monitor = unsafe { output.GetDesc() }.map_err(screen_error)?.Monitor;
    let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor) }.map_err(screen_error)?;
    Ok(item)
}

/// A Direct3D device plus the WinRT wrapper Graphics Capture needs.
fn direct3d_device() -> Result<(ID3D11Device, IDirect3DDevice), CoreError> {
    let mut device = None;
    unsafe {
        D3D11CreateDevice(
            None,
            windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }
    .map_err(screen_error)?;
    let device = device.ok_or_else(|| CoreError::Screen("no Direct3D device".to_owned()))?;
    let dxgi: windows::Win32::Graphics::Dxgi::IDXGIDevice = device.cast().map_err(screen_error)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(screen_error)?;
    let wrapped: IDirect3DDevice = inspectable.cast().map_err(screen_error)?;
    Ok((device, wrapped))
}

/// Copy one captured frame into owned bytes.
fn copy_frame(
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    device: &ID3D11Device,
) -> Result<ScreenFrame, CoreError> {
    let size = frame.ContentSize().map_err(screen_error)?;
    let width = size.Width as u32;
    let height = size.Height as u32;
    let surface = frame.Surface().map_err(screen_error)?;
    let access: windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess =
        surface.cast().map_err(screen_error)?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(screen_error)?;
    let bytes = read_texture(device, &texture, width, height)?;
    Ok(ScreenFrame { png: bytes, width, height })
}

/// Copy a GPU texture into a tightly packed BGRA buffer.
fn read_texture(
    device: &ID3D11Device,
    texture: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CoreError> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        BindFlags: 0,
        MiscFlags: 0,
    };
    let mut staging = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging)) }.map_err(screen_error)?;
    let staging = staging.ok_or_else(|| CoreError::Screen("no staging texture".to_owned()))?;
    let context: ID3D11DeviceContext = unsafe { device.GetImmediateContext() }.map_err(screen_error)?;
    let region = D3D11_BOX { left: 0, top: 0, front: 0, right: width, bottom: height, back: 1 };
    unsafe { context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, Some(&region)) };
    let mut mapped = windows::Win32::Graphics::Direct3D11::D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.map_err(screen_error)?;
    let row_bytes = width as usize * 4;
    let mut bytes = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = mapped.pData.cast::<u8>().wrapping_add(row * mapped.RowPitch as usize);
        bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(start, row_bytes) });
    }
    unsafe { context.Unmap(&staging, 0) };
    Ok(bytes)
}

fn screen_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Screen(error.to_string())
}
