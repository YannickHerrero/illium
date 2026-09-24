//! Live DWM thumbnails of client windows, composed by the Desktop Window
//! Manager into rectangles of a shell surface. Parked windows keep their
//! surface alive, so their thumbnails animate like the visible ones.
#[cfg(test)]
#[path = "thumbnails_tests.rs"]
mod tests;
use super::native;
use crate::layout::Rect;
use std::collections::HashMap;
use windows::Win32::{Foundation::RECT, Graphics::Dwm::*};

pub struct Thumbnails {
    destination: isize,
    handles: HashMap<(isize, usize), isize>,
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
        self.place_part(source, 0, dest, region, opacity);
    }
    /// Multiple cropped pieces let a miniature desktop reproduce occlusion
    /// without relying on DWM thumbnail registration order or Slint's Z order.
    pub fn place_part(
        &mut self,
        source: isize,
        part: usize,
        dest: Rect,
        region: Option<Rect>,
        opacity: u8,
    ) -> bool {
        let key = (source, part);
        let handle = match self.handles.get(&key) {
            Some(handle) => *handle,
            None => {
                let Ok(handle) = (unsafe {
                    DwmRegisterThumbnail(native::hwnd(self.destination), native::hwnd(source))
                }) else {
                    return false;
                };
                self.handles.insert(key, handle);
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
        unsafe { DwmUpdateThumbnailProperties(handle, &properties) }.is_ok()
    }
    /// Keeps the registration but draws nothing, for a card filtered out.
    pub fn hide(&mut self, source: isize) {
        for ((id, _), handle) in &self.handles {
            if *id != source {
                continue;
            }
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_VISIBLE,
                fVisible: false.into(),
                ..Default::default()
            };
            let _ = unsafe { DwmUpdateThumbnailProperties(*handle, &properties) };
        }
    }
    pub fn remove(&mut self, source: isize) {
        self.handles.retain(|(id, _), handle| {
            if *id != source {
                return true;
            }
            let _ = unsafe { DwmUnregisterThumbnail(*handle) };
            false
        });
    }
    /// Release obsolete fragments so repeated occlusion changes do not grow
    /// the registration cache indefinitely. Exposé still uses hide_others.
    pub fn retain_parts(&mut self, shown: &[(isize, usize)]) {
        self.handles.retain(|key, handle| {
            if shown.contains(key) {
                return true;
            }
            let _ = unsafe { DwmUnregisterThumbnail(*handle) };
            false
        });
    }
    /// Windows without a card any more: hidden, so a later card reuses them.
    pub fn hide_others(&mut self, shown: &[isize]) {
        let hidden: Vec<isize> = self
            .handles
            .keys()
            .filter(|(id, _)| !shown.contains(id))
            .map(|(id, _)| *id)
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
