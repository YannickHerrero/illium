//! UI-side wallpaper selection. Decode/resize and preloading belong to Loader.
use super::Shell;
use crate::wallpaper::{
    Selections,
    loader::{Key, Prepared},
};
use std::{collections::VecDeque, sync::Arc, time::Instant};

pub(super) struct Pending {
    candidates: VecDeque<(String, Key)>,
    persist: bool,
    fallback: bool,
    started: Instant,
    selections: Result<Vec<u8>, String>,
}
impl Shell {
    fn wallpaper_keys(&self) -> Result<Vec<(String, Key)>, String> {
        let dir = winarchy_theme::pack::wallpaper_dir(&self.home, &self.theme)?;
        Ok(winarchy_theme::pack::fingerprint(&self.home, &self.theme)?
            .into_iter()
            .map(|(name, size, modified)| {
                let key = Key {
                    path: dir.join(&name),
                    size,
                    modified,
                    screens: self.wallpaper_sizes.clone(),
                    dynamic_home: winarchy_theme::dynamic::is_dynamic(&self.theme)
                        .then(|| self.home.clone()),
                };
                (name, key)
            })
            .collect())
    }
    pub fn pending_wallpaper(&self) -> Option<&str> {
        self.wallpaper_pending
            .as_ref()?
            .candidates
            .front()
            .map(|(name, _)| name.as_str())
    }
    fn show_wallpaper(
        &mut self,
        name: Option<String>,
        images: Vec<slint::Image>,
        blur: Vec<slint::Image>,
        key: Option<Key>,
    ) {
        self.wallpaper = name;
        self.wallpaper_images = images;
        self.wallpaper_blur = blur;
        self.wallpaper_key = key;
        for (index, background) in self.backgrounds.iter().enumerate() {
            background.set_wallpaper(
                self.wallpaper_images
                    .get(index)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
    }
    fn clear_wallpaper(&mut self) -> Result<(), String> {
        if winarchy_theme::dynamic::is_dynamic(&self.theme) {
            winarchy_theme::dynamic::publish(&self.home, None)?;
            self.wallpaper_palette_dirty = true;
        }
        self.wallpaper_loader.cancel();
        self.wallpaper_pending = None;
        self.show_wallpaper(None, vec![], vec![], None);
        Ok(())
    }
    pub fn take_wallpaper_palette_dirty(&mut self) -> bool {
        std::mem::take(&mut self.wallpaper_palette_dirty)
    }
    fn begin_wallpaper(
        &mut self,
        names: Vec<String>,
        persist: bool,
        fallback: bool,
    ) -> Result<(), String> {
        let keys = self.wallpaper_keys()?;
        let candidates: VecDeque<_> = names
            .iter()
            .filter_map(|name| keys.iter().find(|(n, _)| n == name).cloned())
            .collect();
        let Some((name, key)) = candidates.front() else {
            if fallback {
                self.wallpaper_error = None;
                self.clear_wallpaper()?;
                return Ok(());
            }
            return Err("wallpaper not found in active theme".into());
        };
        if self.wallpaper_key.as_ref() == Some(key) {
            if self.wallpaper_pending.take().is_some() {
                self.wallpaper_loader.cancel();
            }
            if persist {
                Selections::load(&self.home).save_choice(
                    &self.home,
                    &self.theme,
                    Some(name.clone()),
                )?;
            }
            if persist {
                self.wallpaper_error = None;
            }
            self.prefetch_wallpaper();
            return Ok(());
        }
        if self
            .wallpaper_pending
            .as_ref()
            .is_some_and(|p| p.candidates.front() == candidates.front())
        {
            // An explicit click can upgrade a not-yet-loaded automatic choice.
            if persist {
                let pending = self.wallpaper_pending.as_mut().unwrap();
                pending.persist = true;
                pending.fallback = fallback;
                pending.candidates = candidates;
            }
            return Ok(());
        }
        self.wallpaper_error = None;
        self.wallpaper_pending = Some(Pending {
            candidates,
            persist,
            fallback,
            started: Instant::now(),
            selections: crate::files::read_config(&self.home.join("wallpapers.json")),
        });
        self.request_wallpaper();
        Ok(())
    }
    fn request_wallpaper(&mut self) {
        let Some((_, key)) = self
            .wallpaper_pending
            .as_ref()
            .and_then(|p| p.candidates.front())
        else {
            return;
        };
        if let Some(pixels) = self.wallpaper_loader.request(key.clone()) {
            self.finish_wallpaper(pixels, true);
        }
    }
    fn finish_wallpaper(&mut self, pixels: Arc<Prepared>, cached: bool) {
        let Some(pending) = self.wallpaper_pending.take() else {
            return;
        };
        let Some((name, key)) = pending.candidates.front() else {
            return;
        };
        if let Err(e) = crate::wallpaper::commit_choice(
            &self.home,
            &self.theme,
            Some(name.clone()),
            pixels.palette.as_ref(),
            pending.persist,
        ) {
            tracing::warn!(%e, "wallpaper choice not saved");
            self.wallpaper_error = Some(e);
            return;
        }
        let mut images: Vec<slint::Image> = Vec::new();
        let mut blur: Vec<slint::Image> = Vec::new();
        for (index, frame) in pixels.frames.iter().enumerate() {
            if let Some(previous) = pixels.frames[..index]
                .iter()
                .position(|f| Arc::ptr_eq(frame, f))
            {
                images.push(images[previous].clone());
                blur.push(blur[previous].clone());
                continue;
            }
            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &frame.pixels,
                frame.width,
                frame.height,
            );
            images.push(slint::Image::from_rgba8(buffer));
            blur.push(
                crate::wallpaper::blurred(&frame.pixels, frame.width, frame.height)
                    .map(|(pixels, w, h)| {
                        slint::Image::from_rgba8(
                            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                &pixels, w, h,
                            ),
                        )
                    })
                    .unwrap_or_default(),
            );
        }
        self.wallpaper_palette_dirty = winarchy_theme::dynamic::is_dynamic(&self.theme);
        self.show_wallpaper(Some(name.clone()), images, blur, Some(key.clone()));
        self.wallpaper_error = None;
        tracing::info!(
            cached,
            elapsed_ms = pending.started.elapsed().as_millis(),
            "wallpaper applied"
        );
        self.prefetch_wallpaper();
    }
    fn prefetch_wallpaper(&self) {
        let Ok(keys) = self.wallpaper_keys() else {
            return;
        };
        let names: Vec<_> = keys.iter().map(|(name, _)| name.clone()).collect();
        if let Some(next) =
            crate::wallpaper::next_candidates(&names, self.wallpaper.as_deref()).first()
            && let Some((_, key)) = keys.iter().find(|(name, _)| name == next)
        {
            self.wallpaper_loader.prefetch(key.clone());
        }
    }
    /// Called from the existing 10ms UI timer. Only the latest selection may apply.
    pub fn poll_wallpaper(&mut self) {
        let Some((key, result)) = self.wallpaper_loader.take_result() else {
            return;
        };
        let Some(pending) = &self.wallpaper_pending else {
            return;
        };
        if !pending
            .candidates
            .front()
            .is_some_and(|(_, requested)| *requested == key)
        {
            return;
        }
        // An image may have been edited while the worker was decoding it.
        if !self
            .wallpaper_keys()
            .is_ok_and(|keys| keys.iter().any(|(_, current)| *current == key))
        {
            let names = pending.candidates.iter().map(|(n, _)| n.clone()).collect();
            let (persist, fallback) = (pending.persist, pending.fallback);
            self.wallpaper_pending = None;
            if let Err(e) = self.begin_wallpaper(names, persist, fallback) {
                self.wallpaper_error = Some(e);
            }
            return;
        }
        match result {
            Ok(pixels) => self.finish_wallpaper(pixels, false),
            Err(e) => {
                tracing::warn!(%e, "wallpaper preparation failed");
                self.wallpaper_error = Some(e);
                let pending = self.wallpaper_pending.as_mut().unwrap();
                pending.candidates.pop_front();
                if pending.candidates.is_empty() {
                    let fallback = pending.fallback;
                    self.wallpaper_pending = None;
                    if fallback && let Err(error) = self.clear_wallpaper() {
                        self.wallpaper_error = Some(error);
                    }
                } else {
                    self.request_wallpaper();
                }
            }
        }
    }
    pub fn refresh_wallpaper(&mut self) {
        // Image directory notifications must not undo an accepted choice whose
        // pixels are still loading. An external selection edit/theme switch can.
        let continuing = self
            .wallpaper_pending
            .as_ref()
            .filter(|p| {
                p.persist
                    && p.selections == crate::files::read_config(&self.home.join("wallpapers.json"))
                    && p.candidates.front().is_some_and(|(_, key)| {
                        winarchy_theme::pack::wallpaper_dir(&self.home, &self.theme)
                            .is_ok_and(|dir| key.path.parent() == Some(dir.as_path()))
                    })
            })
            .map(|p| {
                (
                    p.candidates.iter().map(|(n, _)| n.clone()).collect(),
                    p.fallback,
                )
            });
        if let Some((names, fallback)) = continuing {
            if let Err(e) = self.begin_wallpaper(names, true, fallback) {
                self.wallpaper_loader.cancel();
                self.wallpaper_pending = None;
                self.wallpaper_error = Some(e);
            }
            return;
        }
        let result = self.wallpaper_names().and_then(|names| {
            let names = Selections::load(&self.home).candidates(&self.theme, &names);
            self.begin_wallpaper(names, false, true)
        });
        if let Err(e) = result {
            tracing::warn!(%e, "wallpapers unavailable");
            self.wallpaper_error = Some(e);
            if let Err(error) = self.clear_wallpaper() {
                self.wallpaper_error = Some(error);
            }
        }
    }
    /// On a cold load, acknowledge the request immediately; errors remain visible
    /// through status without destroying the previously displayed/saved choice.
    pub fn set_wallpaper(&mut self, name: Option<String>) -> Result<(), String> {
        match name {
            Some(name) => self.begin_wallpaper(vec![name], true, false),
            None => {
                crate::wallpaper::commit_choice(&self.home, &self.theme, None, None, true)?;
                self.clear_wallpaper()?;
                self.wallpaper_error = None;
                Ok(())
            }
        }
    }
    pub fn next_wallpaper(&mut self) -> Result<(), String> {
        let names = self.wallpaper_names()?;
        if names.is_empty() {
            return Ok(());
        }
        let current = self.pending_wallpaper().or(self.wallpaper.as_deref());
        self.begin_wallpaper(
            crate::wallpaper::next_candidates(&names, current),
            true,
            false,
        )
    }
}
