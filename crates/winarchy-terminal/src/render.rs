//! Native GPU composition with opaque glyphs over an alpha background.
//! Devices/fonts live in the resident process. Each window owns only a swap
//! chain/context; DirectWrite layouts are cached and bounded, never recreated
//! for every glyph on every frame. No continuous render loop.
use crate::{
    config::Config,
    model::{Model, Size},
    palette::Palette,
};
use alacritty_terminal::{
    term::cell::Flags,
    vte::ansi::{Color, CursorShape, NamedColor, Rgb},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::*,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dxgi::{Common::*, *},
        },
    },
    core::{Interface, PCWSTR, w},
};
use windows_numerics::Vector2;
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
type Result<T> = windows::core::Result<T>;
fn color(c: Rgb, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r as f32 / 255.,
        g: c.g as f32 / 255.,
        b: c.b as f32 / 255.,
        a,
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
pub struct Graphics {
    d3d: ID3D11Device,
    dxgi: IDXGIDevice,
    factory: IDXGIFactory2,
    d2d: ID2D1Device,
    composition: IDCompositionDevice,
    pub fonts: Rc<Fonts>,
    write: IDWriteFactory,
}
impl Graphics {
    pub fn new(config: &Config) -> Result<Rc<Self>> {
        unsafe {
            let mut device = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
            .or_else(|_| {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    None,
                )
            })?;
            let d3d = device.unwrap();
            let dxgi: IDXGIDevice = d3d.cast()?;
            let dxgi1: IDXGIDevice1 = dxgi.cast()?;
            dxgi1.SetMaximumFrameLatency(1)?;
            let factory = dxgi.GetAdapter()?.GetParent::<IDXGIFactory2>()?;
            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d = d2d_factory.CreateDevice(&dxgi)?;
            let composition = DCompositionCreateDevice(&dxgi)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = Rc::new(Fonts::new(&write, config)?);
            // Warm the actual face/fallback machinery, not the entire font database.
            for c in ' '..='~' {
                fonts.layout(&c.to_string(), 0)?;
            }
            Ok(Rc::new(Self {
                d3d,
                dxgi,
                factory,
                d2d,
                composition,
                fonts,
                write,
            }))
        }
    }
    pub fn fonts(&self, config: &Config) -> Result<Rc<Fonts>> {
        Ok(Rc::new(Fonts::new(&self.write, config)?))
    }
}
pub struct Fonts {
    write: IDWriteFactory,
    format: IDWriteTextFormat,
    // Separate style buckets allow borrowed &str lookup without allocating a
    // (String, style) key for every visible cell on every frame.
    cache: RefCell<[HashMap<String, IDWriteTextLayout>; 4]>,
    pub width: f32,
    pub height: f32,
}
impl Fonts {
    fn new(write: &IDWriteFactory, c: &Config) -> Result<Self> {
        unsafe {
            let name: Vec<u16> = c.font_family.encode_utf16().chain([0]).collect();
            let format = write.CreateTextFormat(
                PCWSTR(name.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                c.font_size * 96. / 72.,
                w!("en-us"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let layout = write.CreateTextLayout(&[b'M' as u16], &format, 4096., 4096.)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(Self {
                write: write.clone(),
                format,
                cache: RefCell::new(std::array::from_fn(|_| HashMap::new())),
                width: metrics.widthIncludingTrailingWhitespace.max(1.),
                height: metrics.height.ceil().max(1.),
            })
        }
    }
    fn layout(&self, text: &str, style: u8) -> Result<IDWriteTextLayout> {
        unsafe {
            let bucket = usize::from(style & 3);
            let mut cache = self.cache.borrow_mut();
            if let Some(layout) = cache[bucket].get(text) {
                return Ok(layout.clone());
            }
            let utf16: Vec<_> = text.encode_utf16().collect();
            let layout = self
                .write
                .CreateTextLayout(&utf16, &self.format, 4096., self.height)?;
            let range = DWRITE_TEXT_RANGE {
                startPosition: 0,
                length: utf16.len() as u32,
            };
            if style & 1 != 0 {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
            }
            if style & 2 != 0 {
                layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)?;
            }
            if cache.iter().map(HashMap::len).sum::<usize>() >= 4096 {
                for bucket in cache.iter_mut() { bucket.clear(); }
            }
            cache[bucket].insert(text.to_owned(), layout.clone());
            Ok(layout)
        }
    }
}
/// Most terminal cells contain one Unicode scalar. Only combining sequences
/// need owned text; ordinary cells no longer allocate while building a frame.
enum Glyph {
    Empty,
    Scalar(char),
    Combined(String),
}
impl Glyph {
    fn with_str<T>(&self, use_text: impl FnOnce(&str) -> T) -> T {
        let mut utf8 = [0; 4];
        use_text(match self {
            Self::Empty => "",
            Self::Scalar(ch) => ch.encode_utf8(&mut utf8),
            Self::Combined(text) => text,
        })
    }
}
struct Cell {
    col: usize,
    row: usize,
    text: Glyph,
    fg: Rgb,
    bg: Rgb,
    background: bool,
    selected: bool,
    flags: Flags,
}
pub struct Frame {
    cells: Vec<Cell>,
    cursor: Option<(usize, usize, CursorShape)>,
    background: Rgb,
    cursor_color: Rgb,
    selection: Rgb,
    opacity: f32,
}
impl Frame {
    pub fn new(model: Option<&Model>, p: &Palette) -> Self {
        let mut frame = Self {
            cells: Vec::new(),
            cursor: None,
            background: p.colors[NamedColor::Background as usize],
            cursor_color: p.colors[NamedColor::Cursor as usize],
            selection: p.selection,
            opacity: p.opacity,
        };
        let Some(model) = model else {
            return frame;
        };
        let content = model.term.renderable_content();
        frame.background = p.resolve(Color::Named(NamedColor::Background), content.colors);
        frame.cursor_color = p.resolve(Color::Named(NamedColor::Cursor), content.colors);
        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + content.display_offset as i32;
        if cursor_row >= 0 && cursor.shape != CursorShape::Hidden {
            frame.cursor = Some((cursor.point.column.0, cursor_row as usize, cursor.shape));
        }
        for indexed in content.display_iter {
            let c = indexed.cell;
            let row = indexed.point.line.0 + content.display_offset as i32;
            if row < 0 {
                continue;
            }
            let mut fg = p.resolve(c.fg, content.colors);
            let mut bg = p.resolve(c.bg, content.colors);
            let selected = content.selection.is_some_and(|s| s.contains(indexed.point));
            let inverse = c.flags.contains(Flags::INVERSE);
            if inverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            if c.flags.contains(Flags::DIM) {
                fg = Rgb {
                    r: fg.r / 2,
                    g: fg.g / 2,
                    b: fg.b / 2,
                };
            }
            let text = if c.flags.intersects(
                Flags::HIDDEN | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            ) {
                Glyph::Empty
            } else if let Some(extra) = c.zerowidth().filter(|extra| !extra.is_empty()) {
                let mut text = c.c.to_string();
                text.extend(extra);
                Glyph::Combined(text)
            } else {
                Glyph::Scalar(c.c)
            };
            frame.cells.push(Cell {
                col: indexed.point.column.0,
                row: row as usize,
                text,
                fg,
                bg,
                background: inverse || c.bg != Color::Named(NamedColor::Background),
                selected,
                flags: c.flags,
            });
        }
        if model.error.is_some() || model.exited {
            use alacritty_terminal::grid::Dimensions;
            let message = model
                .error
                .as_deref()
                .unwrap_or("WSL exited — close this window");
            let row = model.term.screen_lines() - 1;
            frame.cells.retain(|c| c.row != row);
            for (col, c) in message
                .chars()
                .chain(std::iter::repeat(' '))
                .take(model.term.columns())
                .enumerate()
            {
                frame.cells.push(Cell {
                    col,
                    row,
                    text: Glyph::Scalar(c),
                    fg: p.colors[NamedColor::Foreground as usize],
                    bg: p.selection,
                    background: true,
                    selected: false,
                    flags: Flags::empty(),
                });
            }
            frame.cursor = None;
        }
        frame
    }
}
pub struct Surface {
    graphics: Rc<Graphics>,
    swap: IDXGISwapChain1,
    context: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    pub fonts: Rc<Fonts>,
    pub width: u32,
    pub height: u32,
    pub dpi: f32,
    pub padding: f32,
}
impl Surface {
    pub fn new(
        g: Rc<Graphics>,
        hwnd: HWND,
        width: u32,
        height: u32,
        dpi: u32,
        padding: u16,
    ) -> Result<Self> {
        unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width.max(1),
                Height: height.max(1),
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
            let swap = g
                .factory
                .CreateSwapChainForComposition(&g.d3d, &desc, None)?;
            let context = g
                .d2d
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = context.CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)?;
            let target = g.composition.CreateTargetForHwnd(hwnd, true)?;
            let visual = g.composition.CreateVisual()?;
            visual.SetContent(&swap)?;
            target.SetRoot(&visual)?;
            g.composition.Commit()?;
            let mut s = Self {
                fonts: g.fonts.clone(),
                graphics: g,
                swap,
                context,
                brush,
                _target: target,
                _visual: visual,
                width,
                height,
                dpi: dpi as f32,
                padding: padding as f32,
            };
            s.bind()?;
            Ok(s)
        }
    }
    fn bind(&mut self) -> Result<()> {
        unsafe {
            let buffer: IDXGISurface = self.swap.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: self.dpi,
                dpiY: self.dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bitmap = self
                .context
                .CreateBitmapFromDxgiSurface(&buffer, Some(&props))?;
            self.context.SetTarget(&bitmap);
            self.context.SetDpi(self.dpi, self.dpi);
            Ok(())
        }
    }
    pub fn resize(&mut self, width: u32, height: u32, dpi: u32) -> Result<()> {
        unsafe {
            if width == 0 || height == 0 {
                return Ok(());
            }
            if self.width == width && self.height == height && self.dpi == dpi as f32 {
                return Ok(());
            }
            // EndDraw already flushed the previous frame. Flush outside a
            // BeginDraw/EndDraw pair poisons this context with WRONG_STATE.
            self.context.SetTarget(None);
            self.swap.ResizeBuffers(
                2,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
            self.width = width;
            self.height = height;
            self.dpi = dpi as f32;
            self.bind()
        }
    }
    pub fn size(&self) -> Size {
        Size::new(
            ((self.width as f32 * 96. / self.dpi - 2. * self.padding) / self.fonts.width).max(2.)
                as usize,
            ((self.height as f32 * 96. / self.dpi - 2. * self.padding) / self.fonts.height).max(1.)
                as usize,
        )
    }
    pub fn cell_at(&self, x: i32, y: i32) -> (usize, usize) {
        let size = self.size();
        let col = ((x as f32 * 96. / self.dpi - self.padding) / self.fonts.width).max(0.) as usize;
        let row = ((y as f32 * 96. / self.dpi - self.padding) / self.fonts.height).max(0.) as usize;
        (col.min(size.cols - 1), row.min(size.rows - 1))
    }
    fn fill(&self, r: &D2D_RECT_F, c: Rgb, a: f32) {
        unsafe {
            self.brush.SetColor(&color(c, a));
            self.context.FillRectangle(r, &self.brush);
        }
    }
    pub fn draw(&mut self, frame: &Frame) -> Result<()> {
        self.draw_frame(frame, true)
    }
    fn draw_frame(&mut self, frame: &Frame, present: bool) -> Result<()> {
        unsafe {
            // Resolve layouts before BeginDraw: an allocation/font failure cannot
            // leave the render target in the drawing state.
            let layouts: Vec<_> = frame
                .cells
                .iter()
                .map(|c| c.text.with_str(|text| {
                    if text.trim().is_empty() {
                        Ok(None)
                    } else {
                        self.fonts
                            .layout(
                                text,
                                u8::from(c.flags.contains(Flags::BOLD))
                                    | (2 * u8::from(c.flags.contains(Flags::ITALIC))),
                            )
                            .map(Some)
                    }
                }))
                .collect::<Result<_>>()?;
            self.context.BeginDraw();
            self.context
                .Clear(Some(&color(frame.background, frame.opacity)));
            let fw = self.fonts.width;
            let fh = self.fonts.height;
            for c in &frame.cells {
                let r = rect(
                    self.padding + c.col as f32 * fw,
                    self.padding + c.row as f32 * fh,
                    fw,
                    fh,
                );
                if c.selected {
                    self.fill(&r, frame.selection, 1.);
                } else if c.background {
                    self.fill(&r, c.bg, 1.);
                }
            }
            if let Some((col, row, shape)) = frame.cursor {
                let x = self.padding + col as f32 * fw;
                let y = self.padding + row as f32 * fh;
                let r = match shape {
                    CursorShape::Beam => rect(x, y, 1.5, fh),
                    CursorShape::Underline => rect(x, y + fh - 2., fw, 2.),
                    _ => rect(x, y, fw, fh),
                };
                if shape == CursorShape::HollowBlock {
                    self.brush.SetColor(&color(frame.cursor_color, 1.));
                    self.context.DrawRectangle(&r, &self.brush, 1., None);
                } else {
                    self.fill(&r, frame.cursor_color, 1.);
                }
            }
            for (c, layout) in frame.cells.iter().zip(layouts) {
                let x = self.padding + c.col as f32 * fw;
                let y = self.padding + c.row as f32 * fh;
                let cursor = frame.cursor.is_some_and(|(col, row, s)| {
                    col == c.col && row == c.row && s == CursorShape::Block
                });
                let fg = if cursor { frame.background } else { c.fg };
                if let Some(layout) = layout {
                    self.brush.SetColor(&color(fg, 1.));
                    let clip = rect(
                        x,
                        y,
                        fw * if c.flags.contains(Flags::WIDE_CHAR) {
                            2.
                        } else {
                            1.
                        },
                        fh,
                    );
                    self.context
                        .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
                    self.context.DrawTextLayout(
                        Vector2 { X: x, Y: y },
                        &layout,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );
                    self.context.PopAxisAlignedClip();
                }
                if c.flags.intersects(Flags::ALL_UNDERLINES) {
                    self.fill(&rect(x, y + fh - 2., fw, 1.), fg, 1.);
                }
                if c.flags.contains(Flags::STRIKEOUT) {
                    self.fill(&rect(x, y + fh * 0.55, fw, 1.), fg, 1.);
                }
            }
            self.context.EndDraw(None, None)?;
            // No extra vsync wait on the UI thread; DWM composes the latest frame.
            if present {
                self.swap.Present(0, DXGI_PRESENT(0)).ok()?;
                self.graphics.composition.Commit()?;
            }
            Ok(())
        }
    }
    pub fn trim(&self) {
        unsafe {
            if let Ok(d) = self.graphics.dxgi.cast::<IDXGIDevice3>() {
                d.Trim();
            }
        }
    }
}
