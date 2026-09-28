//! Slint software scenes presented with per-pixel alpha through DirectComposition.
//! Like the terminal, HWNDs have no GDI redirection bitmap. Native input is
//! mirrored without its background; only scene backgrounds carry theme opacity.
#![allow(unsafe_op_in_unsafe_fn)]
use slint::platform::{
    Platform, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use std::{cell::RefCell, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            DirectComposition::*,
            Dxgi::{Common::*, *},
        },
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::*,
};
slint::include_modules!();

thread_local! {
    static ADAPTER: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
    static GRAPHICS: RefCell<Option<Rc<Graphics>>> = const { RefCell::new(None) };
    static PLATFORM_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// DirectComposition COM resources must be dropped before CoUninitialize.
pub fn release_graphics() {
    GRAPHICS.with(|slot| slot.borrow_mut().take());
}
struct Backend;
impl Platform for Backend {
    fn create_window_adapter(
        &self,
    ) -> std::result::Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        ADAPTER.with(|slot| *slot.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
}
struct Graphics {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    factory: IDXGIFactory2,
    composition: IDCompositionDevice,
}
impl Graphics {
    unsafe fn new() -> Result<Self> {
        let mut device = None;
        let mut context = None;
        let mut create = |driver| {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        create(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create(D3D_DRIVER_TYPE_WARP))?;
        let device = device.unwrap();
        let dxgi: IDXGIDevice = device.cast()?;
        let factory = dxgi.GetAdapter()?.GetParent::<IDXGIFactory2>()?;
        let composition = DCompositionCreateDevice(&dxgi)?;
        Ok(Self {
            device,
            context: context.unwrap(),
            factory,
            composition,
        })
    }
}
pub struct Surface {
    pub ui: BrowserSurface,
    window: Rc<MinimalSoftwareWindow>,
    graphics: Rc<Graphics>,
    swap: IDXGISwapChain1,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    hwnd: HWND,
    size: (u32, u32),
    pixels: Vec<PremultipliedRgbaColor>,
    bgra: Vec<u8>,
}
impl Surface {
    pub unsafe fn new(hwnd: HWND, kind: i32) -> Result<Self> {
        let graphics = GRAPHICS.with(|slot| -> Result<Rc<Graphics>> {
            if let Some(g) = slot.borrow().as_ref() {
                return Ok(g.clone());
            }
            if !PLATFORM_READY.get() {
                slint::platform::set_platform(Box::new(Backend))
                    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
                PLATFORM_READY.set(true);
            }
            let g = Rc::new(Graphics::new()?);
            *slot.borrow_mut() = Some(g.clone());
            Ok(g)
        })?;
        let ui = BrowserSurface::new().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let window = ADAPTER.with(|slot| slot.borrow_mut().take().unwrap());
        ui.set_kind(kind);
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: 1,
            Height: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        let swap = graphics
            .factory
            .CreateSwapChainForComposition(&graphics.device, &desc, None)?;
        let target = graphics.composition.CreateTargetForHwnd(hwnd, false)?;
        let visual = graphics.composition.CreateVisual()?;
        visual.SetContent(&swap)?;
        target.SetRoot(&visual)?;
        graphics.composition.Commit()?;
        Ok(Self {
            ui,
            window,
            graphics,
            swap,
            _target: target,
            _visual: visual,
            hwnd,
            size: (1, 1),
            pixels: vec![],
            bgra: vec![],
        })
    }
    pub fn theme(&self, theme: &illium_theme::Theme) {
        let c = |value: &str| {
            let (r, g, b) = illium_theme::rgb(value).unwrap();
            slint::Color::from_rgb_u8(r, g, b)
        };
        self.ui.set_bg(c(&theme.background));
        self.ui.set_surface(c(&theme.surface));
        self.ui.set_text_color(c(&theme.text));
        self.ui.set_subtext(c(&theme.subtext));
        self.ui.set_accent(c(&theme.accent));
        self.ui.set_border(c(&theme.overlay));
        self.ui.set_background_opacity(theme.background_opacity);
    }
    pub unsafe fn paint(&mut self) -> Result<()> {
        let mut rect = RECT::default();
        GetClientRect(self.hwnd, &mut rect)?;
        let size = (rect.right.max(1) as u32, rect.bottom.max(1) as u32);
        if size != self.size {
            self.swap.ResizeBuffers(
                2,
                size.0,
                size.1,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
            self.size = size;
        }
        self.window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: GetDpiForWindow(self.hwnd) as f32 / 96.,
        });
        self.window
            .set_size(slint::PhysicalSize::new(size.0, size.1));
        self.window.request_redraw();
        self.pixels.resize(
            (size.0 * size.1) as usize,
            PremultipliedRgbaColor::default(),
        );
        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut self.pixels, size.0 as usize);
        });
        self.bgra.clear();
        self.bgra.extend(
            self.pixels
                .iter()
                .flat_map(|p| [p.blue, p.green, p.red, p.alpha]),
        );
        let buffer: ID3D11Texture2D = self.swap.GetBuffer(0)?;
        self.graphics.context.UpdateSubresource(
            &buffer,
            0,
            None,
            self.bgra.as_ptr().cast(),
            size.0 * 4,
            0,
        );
        self.swap.Present(1, DXGI_PRESENT(0)).ok()?;
        Ok(())
    }
    #[cfg(test)]
    pub unsafe fn assert_alpha(&mut self, background: u8, glyphs: bool) {
        self.paint().unwrap();
        let sample = self.pixels[(self.size.0 * 4 + 4) as usize];
        assert!(
            sample.alpha.abs_diff(background) <= 1,
            "background alpha: {} != {background}",
            sample.alpha
        );
        if self.ui.get_kind() == 1 {
            // Field padding must carry exactly the same alpha as the palette,
            // not an opaque (or twice-composited) second background. This is
            // independent of RGB/luminance, so it catches light-theme regressions.
            let scale = GetDpiForWindow(self.hwnd) as f32 / 96.;
            for (x, y) in [(20., 30.), (55., 14.)] {
                let index = (y * scale) as usize * self.size.0 as usize + (x * scale) as usize;
                assert!(
                    self.pixels[index].alpha.abs_diff(background) <= 1,
                    "input background alpha: {} != {background}",
                    self.pixels[index].alpha
                );
            }
        }
        if glyphs {
            // Exclude border/separators and input. This region contains
            // only Slint text over the translucent palette background.
            let width = self.size.0 as usize;
            let opaque = self.pixels.chunks(width).enumerate().any(|(y, row)| {
                (100..self.size.1 as usize - 48).contains(&y)
                    && row[24..width - 24].iter().any(|p| p.alpha == 255)
            });
            assert!(opaque, "palette glyphs must remain fully opaque");
        }
        assert!(
            self.pixels
                .iter()
                .all(|p| p.red <= p.alpha && p.green <= p.alpha && p.blue <= p.alpha),
            "pixels must be premultiplied"
        );
    }
    pub unsafe fn pointer(&self, msg: u32, lp: LPARAM) {
        let scale = GetDpiForWindow(self.hwnd) as f32 / 96.;
        let position = slint::LogicalPosition::new(
            (lp.0 as i16) as f32 / scale,
            ((lp.0 >> 16) as i16) as f32 / scale,
        );
        let event = match msg {
            WM_LBUTTONDOWN => WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            },
            WM_LBUTTONUP => WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            },
            _ => WindowEvent::PointerMoved { position },
        };
        self.window.dispatch_event(event);
    }
}

/// Owned palettes are not independent managed applications. Screen coordinates
/// follow their owner's client area, including off-screen workspace parking.
pub unsafe fn place_popup(
    hwnd: HWND,
    parent: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    visible: bool,
) {
    let mut origin = POINT { x, y };
    let _ = windows::Win32::Graphics::Gdi::ClientToScreen(parent, &mut origin);
    let show = visible && IsWindowVisible(parent).as_bool() && !IsIconic(parent).as_bool();
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOP),
        origin.x,
        origin.y,
        width,
        height,
        SWP_NOACTIVATE | if show { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
    );
}
