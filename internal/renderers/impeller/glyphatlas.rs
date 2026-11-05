// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! A CPU rasterised glyph atlas, uploaded to Impeller as one texture.
//!
//! The Impeller C API has no way to draw glyph runs, so glyphs are rasterised with swash and
//! packed into an RGBA8 premultiplied bitmap. Mask glyphs are stored as white with the coverage
//! as alpha, so they can be tinted with a blend colour filter. Colour glyphs (emoji) keep their
//! colours.

use std::collections::HashMap;

use i_slint_core::textlayout::sharedparley::fontique;

use crate::ffi;

const ATLAS_WIDTH: u32 = 2048;
const ATLAS_MAX_HEIGHT: u32 = 4096;
/// Empty pixels around every glyph, so linear sampling doesn't pick up the neighbours.
const PADDING: u32 = 1;
pub const SUBPIXEL_BIN_COUNT: u8 = 4;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font_blob_id: u64,
    pub font_index: u32,
    pub glyph_id: u32,
    pub size_bits: u32,
    pub coords: Vec<i16>,
    pub subpixel_bin: u8,
    pub skew_bits: Option<u32>,
    pub embolden: bool,
}

#[derive(Clone, Copy)]
pub struct AtlasGlyph {
    /// Position in the atlas, in pixels.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Offset of the bitmap's top left from the glyph origin (y down).
    pub left: i32,
    pub top: i32,
    pub is_color: bool,
}

pub struct GlyphAtlas {
    glyphs: HashMap<GlyphKey, Option<AtlasGlyph>>,
    swash_fonts: HashMap<(u64, u32), (swash::CacheKey, u32)>,
    pixels: Vec<u8>,
    height: u32,
    shelf_y: u32,
    shelf_height: u32,
    cursor_x: u32,
    dirty: bool,
    texture: ffi::ImpellerTexture,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self {
            glyphs: HashMap::new(),
            swash_fonts: HashMap::new(),
            pixels: Vec::new(),
            height: 0,
            shelf_y: 0,
            shelf_height: 0,
            cursor_x: 0,
            dirty: false,
            texture: std::ptr::null_mut(),
        }
    }
}

impl GlyphAtlas {
    pub fn get(&self, key: &GlyphKey) -> Option<Option<AtlasGlyph>> {
        self.glyphs.get(key).copied()
    }

    pub fn swash_font_ref<'a>(&mut self, font: &'a [u8], blob_id: u64, index: u32) -> Option<swash::FontRef<'a>> {
        let (key, offset) = match self.swash_fonts.get(&(blob_id, index)) {
            Some(info) => *info,
            None => {
                let font_ref = swash::FontRef::from_index(font, index as usize)?;
                let info = (font_ref.key, font_ref.offset);
                self.swash_fonts.insert((blob_id, index), info);
                info
            }
        };
        Some(swash::FontRef { data: font, offset, key })
    }

    /// Rasterises a glyph with `scaler` and stores it, unless it's in the atlas already.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        scaler: &mut swash::scale::Scaler<'_>,
        font_size: f32,
        synthesis: &fontique::Synthesis,
    ) {
        if self.glyphs.contains_key(&key) {
            return;
        }

        let transform = synthesis.skew().map(|degrees| {
            swash::zeno::Transform::skew(
                swash::zeno::Angle::from_degrees(degrees),
                swash::zeno::Angle::ZERO,
            )
        });
        let mut render = swash::scale::Render::new(&[
            swash::scale::Source::ColorOutline(0),
            swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit),
            swash::scale::Source::Outline,
        ]);
        render
            .format(swash::zeno::Format::Alpha)
            .offset(swash::zeno::Vector::new(
                key.subpixel_bin as f32 / SUBPIXEL_BIN_COUNT as f32,
                0.0,
            ))
            .transform(transform);
        if key.embolden {
            // Same amount as Skia's fake bold.
            let extra = if font_size <= 9.0 {
                1.0 / 24.0
            } else if font_size >= 36.0 {
                1.0 / 32.0
            } else {
                let t = (font_size - 9.0) / 27.0;
                1.0 / 24.0 + t * (1.0 / 32.0 - 1.0 / 24.0)
            };
            render.embolden(font_size * extra);
        }

        let glyph = render
            .render(scaler, key.glyph_id as u16)
            .filter(|image| image.placement.width > 0 && image.placement.height > 0)
            .and_then(|image| self.pack(&image));
        self.glyphs.insert(key, glyph);
    }

    fn pack(&mut self, image: &swash::scale::image::Image) -> Option<AtlasGlyph> {
        let width = image.placement.width;
        let height = image.placement.height;
        let is_color = matches!(image.content, swash::scale::image::Content::Color);

        let padded_w = width + PADDING * 2;
        let padded_h = height + PADDING * 2;
        if padded_w > ATLAS_WIDTH || padded_h > ATLAS_MAX_HEIGHT {
            return None;
        }

        if self.cursor_x + padded_w > ATLAS_WIDTH {
            self.shelf_y += self.shelf_height;
            self.shelf_height = 0;
            self.cursor_x = 0;
        }
        if self.shelf_y + padded_h > ATLAS_MAX_HEIGHT {
            // Full. Start over: draw commands recorded so far keep the old texture.
            self.glyphs.clear();
            self.pixels.clear();
            self.height = 0;
            self.shelf_y = 0;
            self.shelf_height = 0;
            self.cursor_x = 0;
        }
        self.shelf_height = self.shelf_height.max(padded_h);

        let needed_height = (self.shelf_y + self.shelf_height).next_power_of_two().max(128);
        if needed_height > self.height {
            self.height = needed_height;
            self.pixels.resize((ATLAS_WIDTH * self.height * 4) as usize, 0);
        }

        let x = self.cursor_x + PADDING;
        let y = self.shelf_y + PADDING;
        self.cursor_x += padded_w;

        for row in 0..height {
            for col in 0..width {
                let dst = (((y + row) * ATLAS_WIDTH + x + col) * 4) as usize;
                let pixel = if is_color {
                    let src = ((row * width + col) * 4) as usize;
                    let [r, g, b, a] = [
                        image.data[src] as u32,
                        image.data[src + 1] as u32,
                        image.data[src + 2] as u32,
                        image.data[src + 3] as u32,
                    ];
                    [
                        ((r * a + 127) / 255) as u8,
                        ((g * a + 127) / 255) as u8,
                        ((b * a + 127) / 255) as u8,
                        a as u8,
                    ]
                } else {
                    let a = image.data[(row * width + col) as usize];
                    [a, a, a, a]
                };
                self.pixels[dst..dst + 4].copy_from_slice(&pixel);
            }
        }
        self.dirty = true;

        Some(AtlasGlyph {
            x,
            y,
            width,
            height,
            left: image.placement.left,
            top: -image.placement.top,
            is_color,
        })
    }

    /// Returns the texture holding all glyphs, uploading it first if glyphs were added since the
    /// last upload.
    pub fn texture(&mut self, context: ffi::ImpellerContext) -> ffi::ImpellerTexture {
        if self.dirty || self.texture.is_null() {
            if !self.texture.is_null() {
                // The display list keeps its own reference for what was already drawn.
                unsafe { ffi::ImpellerTextureRelease(self.texture) };
                self.texture = std::ptr::null_mut();
            }
            if self.height > 0 && !context.is_null() {
                let descriptor = ffi::ImpellerTextureDescriptor {
                    pixel_format: ffi::ImpellerPixelFormat::RGBA8888,
                    size: ffi::ImpellerISize {
                        width: ATLAS_WIDTH as i64,
                        height: self.height as i64,
                    },
                    mip_count: 1,
                };
                let mapping = ffi::ImpellerMapping {
                    data: self.pixels.as_ptr(),
                    length: self.pixels.len() as u64,
                    on_release: None,
                };
                self.texture = unsafe {
                    ffi::ImpellerTextureCreateWithContentsNew(
                        context,
                        &descriptor as *const _,
                        &mapping as *const _,
                        std::ptr::null_mut(),
                    )
                };
            }
            self.dirty = false;
        }
        self.texture
    }

    /// Drops everything. Must be called before the Impeller context goes away.
    pub fn clear(&mut self) {
        if !self.texture.is_null() {
            unsafe { ffi::ImpellerTextureRelease(self.texture) };
            self.texture = std::ptr::null_mut();
        }
        self.glyphs.clear();
        self.pixels = Vec::new();
        self.height = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.cursor_x = 0;
        self.dirty = false;
    }
}

impl Drop for GlyphAtlas {
    fn drop(&mut self) {
        if !self.texture.is_null() {
            unsafe { ffi::ImpellerTextureRelease(self.texture) };
        }
    }
}
