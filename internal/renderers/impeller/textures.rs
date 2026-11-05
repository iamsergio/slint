// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::collections::HashMap;
use std::rc::Rc;

use i_slint_core::graphics::{ImageCacheKey, euclid};
use i_slint_core::item_rendering::ItemCache;
use i_slint_core::lengths::PhysicalPx;

use crate::ffi;

pub type TargetSize = Option<euclid::Size2D<u32, PhysicalPx>>;

/// An uploaded Impeller texture, released when the last reference goes away.
pub struct CachedTexture {
    pub texture: ffi::ImpellerTexture,
    pub width: u32,
    pub height: u32,
    /// The size an SVG was rasterised at; `None` for other sources.
    pub target_size: TargetSize,
}

impl Drop for CachedTexture {
    fn drop(&mut self) {
        unsafe { ffi::ImpellerTextureRelease(self.texture) }
    }
}

/// The per-item cache: the texture an image item drew last, invalidated when its properties change.
pub type ItemTextureCache = ItemCache<Option<Rc<CachedTexture>>>;

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct TextureCacheKey {
    source_key: ImageCacheKey,
    target_size_for_scalable_source: TargetSize,
}

impl TextureCacheKey {
    pub fn new(
        resource: &i_slint_core::graphics::ImageInner,
        target_size_for_scalable_source: TargetSize,
    ) -> Option<Self> {
        ImageCacheKey::new(resource)
            .map(|source_key| Self { source_key, target_size_for_scalable_source })
    }
}

/// Shares textures between items showing the same image. Entries nobody else references are
/// drained after each frame.
#[derive(Default)]
pub struct TextureCache(HashMap<TextureCacheKey, Rc<CachedTexture>>);

impl TextureCache {
    pub fn lookup_or_create(
        &mut self,
        key: TextureCacheKey,
        create: impl FnOnce() -> Option<Rc<CachedTexture>>,
    ) -> Option<Rc<CachedTexture>> {
        Some(match self.0.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => e.insert(create()?).clone(),
        })
    }

    pub fn drain(&mut self) {
        self.0.retain(|_, texture| Rc::strong_count(texture) > 1);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
