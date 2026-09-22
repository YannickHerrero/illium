//! Live DWM thumbnails of client windows, composed by the Desktop Window
//! Manager into rectangles of the exposé surface. Parked windows keep their
//! surface alive, so their thumbnails animate like the visible ones.
use super::native;
use crate::layout::Rect;
use std::collections::HashMap;
use windows::Win32::{Foundation::RECT, Graphics::Dwm::*};

pub struct Thumbnails {
    destination: isize,
    handles: HashMap<isize, isize>,
}
fn rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}
impl Thumbnails {
    /// `destination` is a top-level window of this process.
    pub fn new(destination: isize) -> Self {
        Self {
            destination,
            handles: HashMap::new(),
        }
    }
    /// Shows `source` inside `dest` (physical client coordinates of the
    /// destination). `region` selects a part of the source window, relative
    /// to its own top-left corner; None shows the whole window.
    pub fn place(&mut self, source: isize, dest: Rect, region: Option<Rect>, opacity: u8) {
        let handle = match self.handles.get(&source) {
            Some(handle) => *handle,
            None => {
                let Ok(handle) = (unsafe {
                    DwmRegisterThumbnail(native::hwnd(self.destination), native::hwnd(source))
                }) else {
                    return;
                };
                self.handles.insert(source, handle);
                handle
            }
        };
        let mut properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
            rcDestination: rect(dest),
            opacity,
            fVisible: true.into(),
            ..Default::default()
        };
        if let Some(region) = region {
            properties.dwFlags |= DWM_TNP_RECTSOURCE;
            properties.rcSource = rect(region);
        }
        let _ = unsafe { DwmUpdateThumbnailProperties(handle, &properties) };
    }
    /// Keeps the registration but draws nothing, for a card filtered out.
    pub fn hide(&mut self, source: isize) {
        if let Some(handle) = self.handles.get(&source) {
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_VISIBLE,
                fVisible: false.into(),
                ..Default::default()
            };
            let _ = unsafe { DwmUpdateThumbnailProperties(*handle, &properties) };
        }
    }
    pub fn remove(&mut self, source: isize) {
        if let Some(handle) = self.handles.remove(&source) {
            let _ = unsafe { DwmUnregisterThumbnail(handle) };
        }
    }
    /// Windows without a card any more: hidden, so a later card reuses them.
    pub fn hide_others(&mut self, shown: &[isize]) {
        let hidden: Vec<isize> = self
            .handles
            .keys()
            .filter(|id| !shown.contains(id))
            .copied()
            .collect();
        for id in hidden {
            self.hide(id);
        }
    }
}
impl Drop for Thumbnails {
    fn drop(&mut self) {
        for handle in self.handles.values() {
            let _ = unsafe { DwmUnregisterThumbnail(*handle) };
        }
    }
}
