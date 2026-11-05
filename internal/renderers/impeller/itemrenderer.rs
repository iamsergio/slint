// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

use i_slint_core::graphics::{Image, ImageInner, SharedImageBuffer, euclid};
use i_slint_core::item_rendering::{
    CachedRenderingData, ItemRenderer, RenderImage, RenderText,
};
use i_slint_core::items::{ImageFit, ImageRendering, ItemRc, Layer, Opacity, RenderingResult};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalLength, LogicalPoint, LogicalRect, LogicalSize, LogicalVector,
    PhysicalPx, ScaleFactor,
};
use i_slint_core::textlayout::sharedparley::{self, GlyphRenderer, fontique};
use i_slint_core::{Brush, Color};

use crate::ffi;
use crate::glyphatlas::{AtlasGlyph, GlyphAtlas, GlyphKey, SUBPIXEL_BIN_COUNT};
use crate::textures::{CachedTexture, ItemTextureCache, TextureCache, TextureCacheKey};

pub struct ImpellerItemRenderer<'a> {
    builder: ffi::ImpellerDisplayListBuilder,
    scale_factor: ScaleFactor,
    window: &'a i_slint_core::api::Window,
    impeller_context: ffi::ImpellerContext,
    glyph_atlas: &'a RefCell<GlyphAtlas>,
    text_layout_cache: &'a sharedparley::TextLayoutCache,
    texture_cache: &'a RefCell<TextureCache>,
    item_texture_cache: &'a ItemTextureCache,
    current_opacity: f32,
    opacity_stack: Vec<f32>,
    /// The sum of the translations applied so far, in physical pixels. Glyphs are snapped to the
    /// pixel grid relative to it.
    origin: ffi::ImpellerPoint,
    origin_stack: Vec<ffi::ImpellerPoint>,
    /// The current clip bounding box, in the current (logical) coordinate system.
    clip: LogicalRect,
    clip_stack: Vec<LogicalRect>,
}

impl<'a> ImpellerItemRenderer<'a> {
    pub fn new(
        builder: ffi::ImpellerDisplayListBuilder,
        scale_factor: ScaleFactor,
        window: &'a i_slint_core::api::Window,
        impeller_context: ffi::ImpellerContext,
        glyph_atlas: &'a RefCell<GlyphAtlas>,
        text_layout_cache: &'a sharedparley::TextLayoutCache,
        texture_cache: &'a RefCell<TextureCache>,
        item_texture_cache: &'a ItemTextureCache,
    ) -> Self {
        let window_size = window.size();
        Self {
            builder,
            scale_factor,
            window,
            impeller_context,
            glyph_atlas,
            text_layout_cache,
            texture_cache,
            item_texture_cache,
            current_opacity: 1.0,
            opacity_stack: Vec::new(),
            origin: ffi::ImpellerPoint { x: 0.0, y: 0.0 },
            origin_stack: Vec::new(),
            clip: LogicalRect::from_size(
                LogicalSize::new(window_size.width as f32, window_size.height as f32)
                    / scale_factor.get(),
            ),
            clip_stack: Vec::new(),
        }
    }

    pub fn clear_background(&mut self, color: &Color) {
        let window_size = self.window.size();
        let rect = ffi::ImpellerRect {
            x: 0.0,
            y: 0.0,
            width: window_size.width as f32,
            height: window_size.height as f32,
        };

        let impeller_color = ffi::ImpellerColor {
            red: color.red() as f32 / 255.0,
            green: color.green() as f32 / 255.0,
            blue: color.blue() as f32 / 255.0,
            alpha: color.alpha() as f32 / 255.0,
            color_space: ffi::ImpellerColorSpace::SRGB,
        };

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if !paint.is_null() {
                ffi::ImpellerPaintSetColor(paint, &impeller_color as *const _);
                ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);
                ffi::ImpellerDisplayListBuilderDrawRect(self.builder, &rect as *const _, paint);
                ffi::ImpellerPaintRelease(paint);
            }
        }
    }

    fn brush_to_color(&self, brush: &Brush) -> Option<ffi::ImpellerColor> {
        match brush {
            Brush::SolidColor(color) => {
                if color.alpha() == 0 {
                    None
                } else {
                    Some(self.color_to_impeller(color))
                }
            }
            _ => None,
        }
    }

    /// Convert gradient stops to Impeller color/position arrays.
    fn gradient_stops_to_impeller<'s>(
        &self,
        stops: impl Iterator<Item = &'s i_slint_core::graphics::GradientStop>,
    ) -> (Vec<ffi::ImpellerColor>, Vec<f32>) {
        let mut colors = Vec::new();
        let mut positions = Vec::new();
        for stop in stops {
            colors.push(self.color_to_impeller(&stop.color));
            positions.push(stop.position);
        }
        (colors, positions)
    }

    /// Compute linear gradient start/end points for a given angle (degrees) and size.
    /// Equivalent to `i_slint_core::graphics::line_for_angle`.
    fn line_for_angle(
        angle: f32,
        width: f32,
        height: f32,
    ) -> (ffi::ImpellerPoint, ffi::ImpellerPoint) {
        let angle = (angle + 90.0_f32).to_radians();
        let (s, c) = angle.sin_cos();

        let (a, b) = if s.abs() < f32::EPSILON {
            let y = height / 2.0;
            return if c < 0.0 {
                (ffi::ImpellerPoint { x: 0.0, y }, ffi::ImpellerPoint { x: width, y })
            } else {
                (ffi::ImpellerPoint { x: width, y }, ffi::ImpellerPoint { x: 0.0, y })
            };
        } else if c * s < 0.0 {
            let x = (s * width + c * height) * s / 2.0;
            let y = -c * x / s + height;
            (ffi::ImpellerPoint { x, y }, ffi::ImpellerPoint { x: width - x, y: height - y })
        } else {
            let x = (s * width - c * height) * s / 2.0;
            let y = -c * x / s;
            (ffi::ImpellerPoint { x: width - x, y: height - y }, ffi::ImpellerPoint { x, y })
        };

        if s > 0.0 { (a, b) } else { (b, a) }
    }

    /// Set up an ImpellerPaint from a Brush. Returns the paint handle (caller must release),
    /// and an optional color source that must also be released.
    /// Returns None if the brush is fully transparent.
    ///
    /// # Safety
    /// Caller must ensure the returned paint and color source are released.
    unsafe fn setup_paint_for_brush(
        &self,
        brush: &Brush,
        rect: &ffi::ImpellerRect,
    ) -> Option<(ffi::ImpellerPaint, Option<ffi::ImpellerColorSource>)> {
        if brush.is_transparent() {
            return None;
        }

        let paint = unsafe { ffi::ImpellerPaintNew() };
        if paint.is_null() {
            return None;
        }

        match brush {
            Brush::SolidColor(color) => {
                let mut c = self.color_to_impeller(color);
                c.alpha *= self.current_opacity;
                unsafe { ffi::ImpellerPaintSetColor(paint, &c as *const _) };
                Some((paint, None))
            }
            Brush::LinearGradient(gradient) => {
                let (mut colors, positions) = self.gradient_stops_to_impeller(gradient.stops());
                if colors.is_empty() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }
                if self.current_opacity < 1.0 {
                    for c in &mut colors {
                        c.alpha *= self.current_opacity;
                    }
                }

                let (start, end) = Self::line_for_angle(gradient.angle(), rect.width, rect.height);

                let start_point = ffi::ImpellerPoint { x: rect.x + start.x, y: rect.y + start.y };
                let end_point = ffi::ImpellerPoint { x: rect.x + end.x, y: rect.y + end.y };

                let color_source = unsafe {
                    ffi::ImpellerColorSourceCreateLinearGradientNew(
                        &start_point as *const _,
                        &end_point as *const _,
                        colors.len() as u32,
                        colors.as_ptr(),
                        positions.as_ptr(),
                        ffi::ImpellerTileMode::Clamp,
                        std::ptr::null(),
                    )
                };

                if color_source.is_null() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }

                unsafe { ffi::ImpellerPaintSetColorSource(paint, color_source) };
                Some((paint, Some(color_source)))
            }
            Brush::RadialGradient(gradient) => {
                let (mut colors, positions) = self.gradient_stops_to_impeller(gradient.stops());
                if colors.is_empty() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }
                if self.current_opacity < 1.0 {
                    for c in &mut colors {
                        c.alpha *= self.current_opacity;
                    }
                }

                let cx = rect.x + rect.width / 2.0;
                let cy = rect.y + rect.height / 2.0;
                let radius = 0.5 * (rect.width * rect.width + rect.height * rect.height).sqrt();
                let center = ffi::ImpellerPoint { x: cx, y: cy };

                let color_source = unsafe {
                    ffi::ImpellerColorSourceCreateRadialGradientNew(
                        &center as *const _,
                        radius,
                        colors.len() as u32,
                        colors.as_ptr(),
                        positions.as_ptr(),
                        ffi::ImpellerTileMode::Clamp,
                        std::ptr::null(),
                    )
                };

                if color_source.is_null() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }

                unsafe { ffi::ImpellerPaintSetColorSource(paint, color_source) };
                Some((paint, Some(color_source)))
            }
            Brush::ConicGradient(gradient) => {
                let (mut colors, positions) = self.gradient_stops_to_impeller(gradient.stops());
                if colors.is_empty() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }
                if self.current_opacity < 1.0 {
                    for c in &mut colors {
                        c.alpha *= self.current_opacity;
                    }
                }

                let cx = rect.x + rect.width / 2.0;
                let cy = rect.y + rect.height / 2.0;
                let center = ffi::ImpellerPoint { x: cx, y: cy };

                let color_source = unsafe {
                    ffi::ImpellerColorSourceCreateSweepGradientNew(
                        &center as *const _,
                        0.0,
                        360.0,
                        colors.len() as u32,
                        colors.as_ptr(),
                        positions.as_ptr(),
                        ffi::ImpellerTileMode::Clamp,
                        std::ptr::null(),
                    )
                };

                if color_source.is_null() {
                    unsafe { ffi::ImpellerPaintRelease(paint) };
                    return None;
                }

                unsafe { ffi::ImpellerPaintSetColorSource(paint, color_source) };
                Some((paint, Some(color_source)))
            }
            _ => {
                unsafe { ffi::ImpellerPaintRelease(paint) };
                None
            }
        }
    }

    /// Release paint and optional color source.
    unsafe fn release_paint(
        paint: ffi::ImpellerPaint,
        color_source: Option<ffi::ImpellerColorSource>,
    ) {
        if let Some(cs) = color_source {
            if !cs.is_null() {
                unsafe { ffi::ImpellerColorSourceRelease(cs) };
            }
        }
        unsafe { ffi::ImpellerPaintRelease(paint) };
    }

    fn color_to_impeller(&self, color: &Color) -> ffi::ImpellerColor {
        ffi::ImpellerColor {
            red: color.red() as f32 / 255.0,
            green: color.green() as f32 / 255.0,
            blue: color.blue() as f32 / 255.0,
            alpha: color.alpha() as f32 / 255.0,
            color_space: ffi::ImpellerColorSpace::SRGB,
        }
    }

    fn physical_radii(&self, radius: LogicalBorderRadius) -> ffi::ImpellerRoundingRadii {
        let r = radius * self.scale_factor;
        let p = |v: f32| ffi::ImpellerPoint { x: v, y: v };
        ffi::ImpellerRoundingRadii {
            top_left: p(r.top_left),
            bottom_left: p(r.bottom_left),
            top_right: p(r.top_right),
            bottom_right: p(r.bottom_right),
        }
    }

    fn to_physical_rect(&self, rect: LogicalRect) -> ffi::ImpellerRect {
        let physical = rect * self.scale_factor;
        ffi::ImpellerRect {
            x: physical.origin.x,
            y: physical.origin.y,
            width: physical.size.width,
            height: physical.size.height,
        }
    }

    /// Create an Impeller texture from premultiplied RGBA8 pixel data.
    fn create_texture_from_rgba(
        &self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> ffi::ImpellerTexture {
        if self.impeller_context.is_null() {
            return std::ptr::null_mut();
        }

        let descriptor = ffi::ImpellerTextureDescriptor {
            pixel_format: ffi::ImpellerPixelFormat::RGBA8888,
            size: ffi::ImpellerISize { width: width as i64, height: height as i64 },
            mip_count: 1,
        };

        let mapping = ffi::ImpellerMapping {
            data: rgba_data.as_ptr(),
            length: rgba_data.len() as u64,
            on_release: None,
        };

        unsafe {
            ffi::ImpellerTextureCreateWithContentsNew(
                self.impeller_context,
                &descriptor as *const _,
                &mapping as *const _,
                std::ptr::null_mut(),
            )
        }
    }

    /// Render a Slint Image into an Impeller texture, returning the texture and its dimensions.
    ///
    /// `target_size` is used for scalable images (SVGs) to rasterize at the correct resolution.
    /// Pass `None` for non-scalable images or when the target size is unknown.
    fn image_to_texture(
        &self,
        image: &Image,
        target_size: Option<euclid::Size2D<u32, PhysicalPx>>,
    ) -> Option<Rc<CachedTexture>> {
        let image_inner: &ImageInner = image.into();
        let buffer = image_inner.render_to_buffer(target_size)?;
        let (rgba_data, width, height) = match &buffer {
            SharedImageBuffer::RGBA8(buf) => {
                // Impeller blends premultiplied colours.
                let w = buf.width();
                let h = buf.height();
                let premultiplied: Vec<u8> = buf
                    .as_bytes()
                    .chunks_exact(4)
                    .flat_map(|px| {
                        let a = px[3] as u32;
                        let mul = |c: u8| ((c as u32 * a + 127) / 255) as u8;
                        [mul(px[0]), mul(px[1]), mul(px[2]), px[3]]
                    })
                    .collect();
                let texture = self.create_texture_from_rgba(&premultiplied, w, h);
                if texture.is_null() {
                    return None;
                }
                return Some(Rc::new(CachedTexture { texture, width: w, height: h, target_size }));
            }
            SharedImageBuffer::RGBA8Premultiplied(buf) => {
                (buf.as_bytes(), buf.width(), buf.height())
            }
            SharedImageBuffer::RGB8(buf) => {
                // Convert RGB8 to RGBA8
                let w = buf.width();
                let h = buf.height();
                let rgba: Vec<u8> = buf
                    .as_bytes()
                    .chunks_exact(3)
                    .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                    .collect();
                let texture = self.create_texture_from_rgba(&rgba, w, h);
                if texture.is_null() {
                    return None;
                }
                return Some(Rc::new(CachedTexture { texture, width: w, height: h, target_size }));
            }
            _ => return None,
        };
        if width == 0 || height == 0 {
            return None;
        }
        let texture = self.create_texture_from_rgba(rgba_data, width, height);
        if texture.is_null() {
            return None;
        }
        Some(Rc::new(CachedTexture { texture, width, height, target_size }))
    }
}

impl ItemRenderer for ImpellerItemRenderer<'_> {
    fn draw_rectangle(
        &mut self,
        rect: Pin<&dyn i_slint_core::item_rendering::RenderRectangle>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let geometry = LogicalRect::from(size);
        if geometry.is_empty() {
            return;
        }

        let impeller_rect = self.to_physical_rect(geometry);
        let brush = rect.background();

        unsafe {
            let Some((paint, color_source)) = self.setup_paint_for_brush(&brush, &impeller_rect)
            else {
                return;
            };
            ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);
            ffi::ImpellerDisplayListBuilderDrawRect(
                self.builder,
                &impeller_rect as *const _,
                paint,
            );
            Self::release_paint(paint, color_source);
        }
    }

    fn draw_border_rectangle(
        &mut self,
        rect: Pin<&dyn i_slint_core::item_rendering::RenderBorderRectangle>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let geometry = LogicalRect::from(size);
        if geometry.is_empty() {
            return;
        }

        let border_radius = rect.border_radius();
        let border_width = rect.border_width();
        let border_color = rect.border_color();

        {
            let impeller_rect = self.to_physical_rect(geometry);
            let background = rect.background();

            unsafe {
                if let Some((paint, color_source)) =
                    self.setup_paint_for_brush(&background, &impeller_rect)
                {
                    ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);

                    if border_radius.is_zero() {
                        ffi::ImpellerDisplayListBuilderDrawRect(
                            self.builder,
                            &impeller_rect as *const _,
                            paint,
                        );
                    } else {
                        let physical_radius = border_radius * self.scale_factor;
                        let radii = ffi::ImpellerRoundingRadii {
                            top_left: ffi::ImpellerPoint {
                                x: physical_radius.top_left,
                                y: physical_radius.top_left,
                            },
                            bottom_left: ffi::ImpellerPoint {
                                x: physical_radius.bottom_left,
                                y: physical_radius.bottom_left,
                            },
                            top_right: ffi::ImpellerPoint {
                                x: physical_radius.top_right,
                                y: physical_radius.top_right,
                            },
                            bottom_right: ffi::ImpellerPoint {
                                x: physical_radius.bottom_right,
                                y: physical_radius.bottom_right,
                            },
                        };
                        ffi::ImpellerDisplayListBuilderDrawRoundedRect(
                            self.builder,
                            &impeller_rect as *const _,
                            &radii as *const _,
                            paint,
                        );
                    }

                    Self::release_paint(paint, color_source);
                }
            }
        }

        if !border_color.is_transparent() && border_width > LogicalLength::new(0.0) {
            let physical_border_width = border_width * self.scale_factor;
            let impeller_rect = self.to_physical_rect(geometry);

            unsafe {
                if let Some((paint, color_source)) =
                    self.setup_paint_for_brush(&border_color, &impeller_rect)
                {
                    ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Stroke);
                    ffi::ImpellerPaintSetStrokeWidth(paint, physical_border_width.get());

                    if border_radius.is_zero() {
                        ffi::ImpellerDisplayListBuilderDrawRect(
                            self.builder,
                            &impeller_rect as *const _,
                            paint,
                        );
                    } else {
                        let physical_radius = border_radius * self.scale_factor;
                        let radii = ffi::ImpellerRoundingRadii {
                            top_left: ffi::ImpellerPoint {
                                x: physical_radius.top_left,
                                y: physical_radius.top_left,
                            },
                            bottom_left: ffi::ImpellerPoint {
                                x: physical_radius.bottom_left,
                                y: physical_radius.bottom_left,
                            },
                            top_right: ffi::ImpellerPoint {
                                x: physical_radius.top_right,
                                y: physical_radius.top_right,
                            },
                            bottom_right: ffi::ImpellerPoint {
                                x: physical_radius.bottom_right,
                                y: physical_radius.bottom_right,
                            },
                        };
                        ffi::ImpellerDisplayListBuilderDrawRoundedRect(
                            self.builder,
                            &impeller_rect as *const _,
                            &radii as *const _,
                            paint,
                        );
                    }

                    Self::release_paint(paint, color_source);
                }
            }
        }
    }

    fn draw_window_background(
        &mut self,
        _rect: Pin<&dyn i_slint_core::item_rendering::RenderRectangle>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
    }

    fn draw_image(
        &mut self,
        image: Pin<&dyn RenderImage>,
        self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let geometry = LogicalRect::from(size);
        if geometry.is_empty() {
            return;
        }

        let source = image.source();
        let sf = self.scale_factor.get();
        let target_w = size.width * sf;
        let target_h = size.height * sf;

        // Compute target size for scalable images (SVGs) to rasterize at the right resolution
        let target_size = {
            let source_size = source.size();
            if source_size.width > 0 && source_size.height > 0 {
                let src_w = source_size.width as f32;
                let src_h = source_size.height as f32;
                let image_fit = image.image_fit();
                let (tw, th) = match image_fit {
                    ImageFit::Fill => (target_w, target_h),
                    ImageFit::Contain | ImageFit::Preserve => {
                        let ratio = (target_w / src_w).min(target_h / src_h);
                        (src_w * ratio, src_h * ratio)
                    }
                    ImageFit::Cover => {
                        let ratio = (target_w / src_w).max(target_h / src_h);
                        (src_w * ratio, src_h * ratio)
                    }
                    _ => (target_w, target_h),
                };
                Some(euclid::Size2D::<u32, PhysicalPx>::new(tw.ceil() as u32, th.ceil() as u32))
            } else {
                None
            }
        };

        // Only SVGs depend on the target size.
        let target_size = if ImageInner::is_svg(<&ImageInner>::from(&source)) { target_size } else { None };

        // The item cache is invalidated when the source changes. The size is not a property, so
        // a texture rasterised for another size is dropped by hand.
        let cached = self.item_texture_cache.get_or_update_cache_entry_unless(
            self_rc,
            || {
                let source = image.source();
                let image_inner: &ImageInner = (&source).into();
                match TextureCacheKey::new(image_inner, target_size) {
                    Some(key) => {
                        let cache = self.texture_cache;
                        cache.borrow_mut().lookup_or_create(key, || {
                            self.image_to_texture(&source, target_size)
                        })
                    }
                    None => self.image_to_texture(&source, target_size),
                }
            },
            |cached| cached.as_ref().is_some_and(|c| c.target_size != target_size),
        );
        let Some(cached) = cached else { return };
        let texture = cached.texture;
        let (src_width, src_height) = (cached.width, cached.height);

        let src_w = src_width as f32;
        let src_h = src_height as f32;

        // Calculate destination rect based on image-fit
        let image_fit = image.image_fit();
        let (dst_x, dst_y, dst_w, dst_h, clip_src) = match image_fit {
            ImageFit::Fill => (0.0, 0.0, target_w, target_h, None),
            ImageFit::Contain | ImageFit::Preserve => {
                let ratio_w = target_w / src_w;
                let ratio_h = target_h / src_h;
                let ratio = ratio_w.min(ratio_h);
                let scaled_w = src_w * ratio;
                let scaled_h = src_h * ratio;
                let dx = (target_w - scaled_w) / 2.0;
                let dy = (target_h - scaled_h) / 2.0;
                (dx, dy, scaled_w, scaled_h, None)
            }
            ImageFit::Cover => {
                let ratio_w = target_w / src_w;
                let ratio_h = target_h / src_h;
                let ratio = ratio_w.max(ratio_h);
                // Crop from center: compute source rect
                let visible_src_w = target_w / ratio;
                let visible_src_h = target_h / ratio;
                let src_x = (src_w - visible_src_w) / 2.0;
                let src_y = (src_h - visible_src_h) / 2.0;
                let src_rect = ffi::ImpellerRect {
                    x: src_x,
                    y: src_y,
                    width: visible_src_w,
                    height: visible_src_h,
                };
                (0.0, 0.0, target_w, target_h, Some(src_rect))
            }
            _ => (0.0, 0.0, target_w, target_h, None),
        };

        let sampling = match image.rendering() {
            ImageRendering::Pixelated => ffi::ImpellerTextureSampling::NearestNeighbor,
            _ => ffi::ImpellerTextureSampling::Linear,
        };

        // Apply colorize if set
        let colorize_brush = image.colorize();
        let mut color_filter: ffi::ImpellerColorFilter = std::ptr::null_mut();

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if !paint.is_null() {
                if self.current_opacity < 1.0 {
                    let color = ffi::ImpellerColor {
                        red: 1.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: self.current_opacity,
                        color_space: ffi::ImpellerColorSpace::SRGB,
                    };
                    ffi::ImpellerPaintSetColor(paint, &color as *const _);
                }

                if !colorize_brush.is_transparent() {
                    if let Some(c) = self.brush_to_color(&colorize_brush) {
                        color_filter = ffi::ImpellerColorFilterCreateBlendNew(
                            &c as *const _,
                            ffi::ImpellerBlendMode::SourceIn,
                        );
                        if !color_filter.is_null() {
                            ffi::ImpellerPaintSetColorFilter(paint, color_filter);
                        }
                    }
                }
            }

            let dst_rect = ffi::ImpellerRect { x: dst_x, y: dst_y, width: dst_w, height: dst_h };

            if let Some(src_rect) = clip_src {
                ffi::ImpellerDisplayListBuilderDrawTextureRect(
                    self.builder,
                    texture,
                    &src_rect as *const _,
                    &dst_rect as *const _,
                    sampling,
                    paint,
                );
            } else {
                let src_rect = ffi::ImpellerRect { x: 0.0, y: 0.0, width: src_w, height: src_h };
                ffi::ImpellerDisplayListBuilderDrawTextureRect(
                    self.builder,
                    texture,
                    &src_rect as *const _,
                    &dst_rect as *const _,
                    sampling,
                    paint,
                );
            }

            if !color_filter.is_null() {
                ffi::ImpellerColorFilterRelease(color_filter);
            }
            if !paint.is_null() {
                ffi::ImpellerPaintRelease(paint);
            }
        }
    }

    fn draw_text(
        &mut self,
        text: Pin<&dyn RenderText>,
        self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        sharedparley::draw_text(self, text, Some(self_rc), size, Some(self.text_layout_cache));
    }

    fn draw_text_input(
        &mut self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        sharedparley::draw_text_input(self, text_input, self_rc, size, self.text_layout_cache);
    }

    fn draw_path(
        &mut self,
        path: Pin<&i_slint_core::items::Path>,
        item_rc: &ItemRc,
        size: LogicalSize,
    ) {
        use i_slint_core::items::{FillRule, LineCap, LineJoin};

        let Some((offset, events)) = path.fitted_path_events(item_rc) else { return };
        let scale = self.scale_factor.get();
        let pt = |p: lyon_path::math::Point| ffi::ImpellerPoint { x: p.x * scale, y: p.y * scale };

        unsafe {
            let builder = ffi::ImpellerPathBuilderNew();
            if builder.is_null() {
                return;
            }
            for event in events.iter() {
                match event {
                    lyon_path::Event::Begin { at } => {
                        ffi::ImpellerPathBuilderMoveTo(builder, &pt(at));
                    }
                    lyon_path::Event::Line { from: _, to } => {
                        ffi::ImpellerPathBuilderLineTo(builder, &pt(to));
                    }
                    lyon_path::Event::Quadratic { from: _, ctrl, to } => {
                        ffi::ImpellerPathBuilderQuadraticCurveTo(builder, &pt(ctrl), &pt(to));
                    }
                    lyon_path::Event::Cubic { from: _, ctrl1, ctrl2, to } => {
                        ffi::ImpellerPathBuilderCubicCurveTo(
                            builder,
                            &pt(ctrl1),
                            &pt(ctrl2),
                            &pt(to),
                        );
                    }
                    lyon_path::Event::End { last: _, first: _, close } => {
                        if close {
                            ffi::ImpellerPathBuilderClose(builder);
                        }
                    }
                }
            }
            let fill_type = match path.fill_rule() {
                FillRule::Evenodd => ffi::ImpellerFillType::Odd,
                _ => ffi::ImpellerFillType::NonZero,
            };
            let impeller_path = ffi::ImpellerPathBuilderTakePathNew(builder, fill_type);
            ffi::ImpellerPathBuilderRelease(builder);
            if impeller_path.is_null() {
                return;
            }

            let physical_offset = offset * self.scale_factor;
            let geometry = self.to_physical_rect(LogicalRect::from(size));
            let brush_rect = ffi::ImpellerRect { x: 0.0, y: 0.0, ..geometry };

            ffi::ImpellerDisplayListBuilderSave(self.builder);
            ffi::ImpellerDisplayListBuilderTranslate(
                self.builder,
                physical_offset.x,
                physical_offset.y,
            );

            if let Some((paint, cs)) = self.setup_paint_for_brush(&path.fill(), &brush_rect) {
                ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);
                ffi::ImpellerDisplayListBuilderDrawPath(self.builder, impeller_path, paint);
                Self::release_paint(paint, cs);
            }
            let stroke_width = (path.stroke_width() * self.scale_factor).get();
            if stroke_width > 0.0 {
                if let Some((paint, cs)) = self.setup_paint_for_brush(&path.stroke(), &brush_rect)
                {
                    ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Stroke);
                    ffi::ImpellerPaintSetStrokeWidth(paint, stroke_width);
                    ffi::ImpellerPaintSetStrokeCap(
                        paint,
                        match path.stroke_line_cap() {
                            LineCap::Round => ffi::ImpellerStrokeCap::Round,
                            LineCap::Square => ffi::ImpellerStrokeCap::Square,
                            _ => ffi::ImpellerStrokeCap::Butt,
                        },
                    );
                    ffi::ImpellerPaintSetStrokeJoin(
                        paint,
                        match path.stroke_line_join() {
                            LineJoin::Round => ffi::ImpellerStrokeJoin::Round,
                            LineJoin::Bevel => ffi::ImpellerStrokeJoin::Bevel,
                            _ => ffi::ImpellerStrokeJoin::Miter,
                        },
                    );
                    ffi::ImpellerPaintSetStrokeMiter(paint, path.stroke_miter_limit());
                    // Dash arrays are not supported: the C API has no path effects.
                    ffi::ImpellerDisplayListBuilderDrawPath(self.builder, impeller_path, paint);
                    Self::release_paint(paint, cs);
                }
            }

            ffi::ImpellerDisplayListBuilderRestore(self.builder);
            ffi::ImpellerPathRelease(impeller_path);
        }
    }

    fn draw_box_shadow(
        &mut self,
        box_shadow: Pin<&i_slint_core::items::BoxShadow>,
        item_rc: &ItemRc,
        _size: LogicalSize,
    ) {
        use i_slint_core::graphics::boxshadow::BoxShadowOptions;
        let Some(opts) = BoxShadowOptions::new(item_rc, box_shadow, self.scale_factor) else {
            return;
        };
        let mut color = self.color_to_impeller(&opts.color);
        color.alpha *= self.current_opacity;
        let sigma = opts.blur_sigma();
        let phys_radii = |r: i_slint_core::lengths::PhysicalBorderRadius| ffi::ImpellerRoundingRadii {
            top_left: ffi::ImpellerPoint { x: r.top_left, y: r.top_left },
            bottom_left: ffi::ImpellerPoint { x: r.bottom_left, y: r.bottom_left },
            top_right: ffi::ImpellerPoint { x: r.top_right, y: r.top_right },
            bottom_right: ffi::ImpellerPoint { x: r.bottom_right, y: r.bottom_right },
        };

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if paint.is_null() {
                return;
            }
            ffi::ImpellerPaintSetColor(paint, &color);
            ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);
            let mask = if sigma > 0.0 {
                ffi::ImpellerMaskFilterCreateBlurNew(ffi::ImpellerBlurStyle::Normal, sigma)
            } else {
                std::ptr::null_mut()
            };
            if !mask.is_null() {
                ffi::ImpellerPaintSetMaskFilter(paint, mask);
            }

            if opts.inset {
                let (w, h) = (opts.width.get(), opts.height.get());
                let spread = opts.spread.get();
                // Clip to the item, then fill everything around the (offset, spread) hole.
                ffi::ImpellerDisplayListBuilderSave(self.builder);
                let item_rect = ffi::ImpellerRect { x: 0.0, y: 0.0, width: w, height: h };
                let radii = phys_radii(opts.radius);
                ffi::ImpellerDisplayListBuilderClipRoundedRect(
                    self.builder,
                    &item_rect,
                    &radii,
                    ffi::ImpellerClipOperation::Intersect,
                );
                let big = opts.blur.get() * 3.0 + 1.0;
                let outer = ffi::ImpellerRect {
                    x: -big - w,
                    y: -big - h,
                    width: w * 3.0 + 2.0 * big,
                    height: h * 3.0 + 2.0 * big,
                };
                let hole = ffi::ImpellerRect {
                    x: opts.offset_x_inset + spread,
                    y: opts.offset_y_inset + spread,
                    width: (w - 2.0 * spread).max(0.0),
                    height: (h - 2.0 * spread).max(0.0),
                };
                let hole_radii = phys_radii(opts.inner_radius());
                let pb = ffi::ImpellerPathBuilderNew();
                if !pb.is_null() {
                    ffi::ImpellerPathBuilderAddRect(pb, &outer);
                    ffi::ImpellerPathBuilderAddRoundedRect(pb, &hole, &hole_radii);
                    let path = ffi::ImpellerPathBuilderTakePathNew(pb, ffi::ImpellerFillType::Odd);
                    ffi::ImpellerPathBuilderRelease(pb);
                    if !path.is_null() {
                        ffi::ImpellerDisplayListBuilderDrawPath(self.builder, path, paint);
                        ffi::ImpellerPathRelease(path);
                    }
                }
                ffi::ImpellerDisplayListBuilderRestore(self.builder);
            } else {
                let shape = opts.shape_size();
                if !shape.is_empty() {
                    let off_x = (box_shadow.offset_x() * self.scale_factor).get();
                    let off_y = (box_shadow.offset_y() * self.scale_factor).get();
                    let rect = ffi::ImpellerRect {
                        x: off_x - opts.spread.get(),
                        y: off_y - opts.spread.get(),
                        width: shape.width,
                        height: shape.height,
                    };
                    let radii = phys_radii(opts.outer_radius());
                    ffi::ImpellerDisplayListBuilderDrawRoundedRect(
                        self.builder,
                        &rect,
                        &radii,
                        paint,
                    );
                }
            }
            if !mask.is_null() {
                ffi::ImpellerMaskFilterRelease(mask);
            }
            ffi::ImpellerPaintRelease(paint);
        }
    }

    fn visit_opacity(
        &mut self,
        opacity_item: Pin<&Opacity>,
        item_rc: &ItemRc,
        _size: LogicalSize,
    ) -> RenderingResult {
        let opacity = opacity_item.opacity();
        if Opacity::need_layer(item_rc, opacity) {
            let window_size = self.window.size();
            let bounds = ffi::ImpellerRect {
                x: 0.0,
                y: 0.0,
                width: window_size.width as f32,
                height: window_size.height as f32,
            };

            unsafe {
                let paint = ffi::ImpellerPaintNew();
                let color = ffi::ImpellerColor {
                    red: 1.0,
                    green: 1.0,
                    blue: 1.0,
                    alpha: opacity,
                    color_space: ffi::ImpellerColorSpace::SRGB,
                };
                ffi::ImpellerPaintSetColor(paint, &color as *const _);
                ffi::ImpellerDisplayListBuilderSaveLayer(
                    self.builder,
                    &bounds as *const _,
                    paint,
                    std::ptr::null_mut(),
                );
                ffi::ImpellerPaintRelease(paint);
            }

            let saved_opacity = self.current_opacity;
            self.current_opacity = 1.0;

            let window_adapter =
                i_slint_core::window::WindowInner::from_pub(self.window).window_adapter();
            i_slint_core::item_rendering::render_item_children(
                self,
                item_rc.item_tree(),
                item_rc.index() as isize,
                &window_adapter,
            );

            self.current_opacity = saved_opacity;
            unsafe { ffi::ImpellerDisplayListBuilderRestore(self.builder) };

            RenderingResult::ContinueRenderingWithoutChildren
        } else {
            self.apply_opacity(opacity);
            RenderingResult::ContinueRenderingChildren
        }
    }

    fn visit_layer(
        &mut self,
        _layer_item: Pin<&Layer>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) -> RenderingResult {
        RenderingResult::ContinueRenderingChildren
    }

    fn combine_clip(&mut self, rect: LogicalRect, radius: LogicalBorderRadius) -> bool {
        let impeller_rect = self.to_physical_rect(rect);
        unsafe {
            if radius.is_zero() {
                ffi::ImpellerDisplayListBuilderClipRect(
                    self.builder,
                    &impeller_rect as *const _,
                    ffi::ImpellerClipOperation::Intersect,
                );
            } else {
                let radii = self.physical_radii(radius);
                ffi::ImpellerDisplayListBuilderClipRoundedRect(
                    self.builder,
                    &impeller_rect as *const _,
                    &radii as *const _,
                    ffi::ImpellerClipOperation::Intersect,
                );
            }
        }
        match self.clip.intersection(&rect) {
            Some(r) => {
                self.clip = r;
                true
            }
            None => {
                self.clip = LogicalRect::default();
                false
            }
        }
    }

    fn get_current_clip(&self) -> LogicalRect {
        self.clip
    }

    fn translate(&mut self, distance: LogicalVector) {
        self.clip.origin -= distance;
        let physical_distance = distance * self.scale_factor;
        self.origin.x += physical_distance.x;
        self.origin.y += physical_distance.y;
        unsafe {
            ffi::ImpellerDisplayListBuilderTranslate(
                self.builder,
                physical_distance.x,
                physical_distance.y,
            );
        }
    }

    fn rotate(&mut self, angle_in_degrees: f32) {
        // The clip in the rotated system is the bounding box of the old one rotated back.
        let (s, c) = (-angle_in_degrees).to_radians().sin_cos();
        let r = self.clip;
        let corners = [
            r.origin,
            r.origin + LogicalVector::new(r.size.width, 0.0),
            r.origin + LogicalVector::new(0.0, r.size.height),
            r.origin + LogicalVector::new(r.size.width, r.size.height),
        ];
        let (mut min_x, mut min_y, mut max_x, mut max_y) =
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in corners {
            let (x, y) = (p.x * c - p.y * s, p.x * s + p.y * c);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        self.clip = LogicalRect::new(
            LogicalPoint::new(min_x, min_y),
            LogicalSize::new(max_x - min_x, max_y - min_y),
        );
        unsafe {
            ffi::ImpellerDisplayListBuilderRotate(self.builder, angle_in_degrees);
        }
    }

    fn scale(&mut self, x_factor: f32, y_factor: f32) {
        if x_factor != 0.0 && y_factor != 0.0 {
            let (x0, x1) = (self.clip.min_x() / x_factor, self.clip.max_x() / x_factor);
            let (y0, y1) = (self.clip.min_y() / y_factor, self.clip.max_y() / y_factor);
            self.clip = LogicalRect::new(
                LogicalPoint::new(x0.min(x1), y0.min(y1)),
                LogicalSize::new((x1 - x0).abs(), (y1 - y0).abs()),
            );
        }
        unsafe {
            ffi::ImpellerDisplayListBuilderScale(self.builder, x_factor, y_factor);
        }
    }

    fn apply_opacity(&mut self, opacity: f32) {
        self.current_opacity *= opacity;
    }

    fn save_state(&mut self) {
        self.opacity_stack.push(self.current_opacity);
        self.origin_stack.push(self.origin);
        self.clip_stack.push(self.clip);
        unsafe {
            ffi::ImpellerDisplayListBuilderSave(self.builder);
        }
    }

    fn restore_state(&mut self) {
        if let Some(opacity) = self.opacity_stack.pop() {
            self.current_opacity = opacity;
        }
        if let Some(clip) = self.clip_stack.pop() {
            self.clip = clip;
        }
        if let Some(origin) = self.origin_stack.pop() {
            self.origin = origin;
        }
        unsafe {
            ffi::ImpellerDisplayListBuilderRestore(self.builder);
        }
    }

    fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }

    fn draw_cached_pixmap(
        &mut self,
        _item_rc: &ItemRc,
        _update_fn: &dyn Fn(&mut dyn FnMut(u32, u32, &[u8])),
    ) {
    }

    fn draw_string(&mut self, string: &str, color: Color) {
        sharedparley::draw_text(
            self,
            std::pin::pin!((i_slint_core::SharedString::from(string), Brush::from(color))),
            None,
            LogicalSize::from_lengths(
                LogicalLength::new(self.window.size().width as f32),
                LogicalLength::new(self.window.size().height as f32),
            ) / self.scale_factor.get(),
            None,
        );
    }

    fn draw_image_direct(&mut self, image: i_slint_core::graphics::Image) {
        let Some(cached) = self.image_to_texture(&image, None) else { return };
        let texture = cached.texture;

        let point = ffi::ImpellerPoint { x: 0.0, y: 0.0 };

        unsafe {
            ffi::ImpellerDisplayListBuilderDrawTexture(
                self.builder,
                texture,
                &point as *const _,
                ffi::ImpellerTextureSampling::Linear,
                std::ptr::null_mut(),
            );
            ffi::ImpellerTextureRelease(texture);
        }
    }

    fn window(&self) -> &i_slint_core::window::WindowInner {
        i_slint_core::window::WindowInner::from_pub(self.window)
    }

    fn as_any(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

impl GlyphRenderer for ImpellerItemRenderer<'_> {
    type PlatformBrush = Color;

    fn platform_text_fill_brush(&mut self, brush: Brush, _size: LogicalSize) -> Option<Color> {
        (!brush.is_transparent()).then(|| brush.color())
    }

    fn platform_brush_for_color(&mut self, color: &Color) -> Option<Color> {
        (color.alpha() != 0).then_some(*color)
    }

    fn platform_text_stroke_brush(
        &mut self,
        _brush: Brush,
        _physical_stroke_width: f32,
        _size: LogicalSize,
    ) -> Option<Color> {
        // Stroked text isn't supported yet.
        None
    }

    fn draw_glyph_run(
        &mut self,
        font: &sharedparley::parley::FontData,
        font_size: sharedparley::PhysicalLength,
        normalized_coords: &[i16],
        synthesis: &fontique::Synthesis,
        brush: Color,
        y_offset: sharedparley::PhysicalLength,
        glyphs_it: &mut dyn Iterator<Item = sharedparley::parley::layout::Glyph>,
    ) {
        let font_size = font_size.get();
        let blob_id = font.data.id();

        // Position every glyph on the pixel grid, with a quantised subpixel x offset.
        let positioned: Vec<(GlyphKey, f32, f32)> = glyphs_it
            .map(|g| {
                let abs_x = self.origin.x + g.x;
                let mut ix = abs_x.floor();
                let mut bin = ((abs_x - ix) * SUBPIXEL_BIN_COUNT as f32).round() as u8;
                if bin >= SUBPIXEL_BIN_COUNT {
                    ix += 1.0;
                    bin = 0;
                }
                let iy = (self.origin.y + g.y + y_offset.get()).round();
                let key = GlyphKey {
                    font_blob_id: blob_id,
                    font_index: font.index,
                    glyph_id: g.id,
                    size_bits: font_size.to_bits(),
                    coords: normalized_coords.to_vec(),
                    subpixel_bin: bin,
                    skew_bits: synthesis.skew().map(f32::to_bits),
                    embolden: synthesis.embolden(),
                };
                (key, ix - self.origin.x, iy - self.origin.y)
            })
            .collect();

        let mut atlas = self.glyph_atlas.borrow_mut();

        if positioned.iter().any(|(key, ..)| atlas.get(key).is_none()) {
            if let Some(font_ref) = atlas.swash_font_ref(font.data.data(), blob_id, font.index) {
                let mut scale_context = self.window().context().swash_scale_context().borrow_mut();
                let mut scaler = scale_context
                    .builder(font_ref)
                    .size(font_size)
                    .normalized_coords(normalized_coords)
                    .build();
                for (key, ..) in &positioned {
                    atlas.insert(key.clone(), &mut scaler, font_size, synthesis);
                }
            }
        }

        let texture = atlas.texture(self.impeller_context);
        if texture.is_null() {
            return;
        }

        let text_color = self.color_to_impeller(&brush);
        let mut mask_paint: Option<(ffi::ImpellerPaint, ffi::ImpellerColorFilter)> = None;
        let mut color_paint: Option<ffi::ImpellerPaint> = None;

        for (key, x, y) in &positioned {
            let Some(Some(AtlasGlyph { x: sx, y: sy, width, height, left, top, is_color })) =
                atlas.get(key)
            else {
                continue;
            };

            let paint = if is_color {
                *color_paint.get_or_insert_with(|| unsafe {
                    let paint = ffi::ImpellerPaintNew();
                    let color = ffi::ImpellerColor {
                        red: 1.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: text_color.alpha * self.current_opacity,
                        color_space: ffi::ImpellerColorSpace::SRGB,
                    };
                    ffi::ImpellerPaintSetColor(paint, &color as *const _);
                    paint
                })
            } else {
                mask_paint
                    .get_or_insert_with(|| unsafe {
                        let paint = ffi::ImpellerPaintNew();
                        // The opacity goes on the paint, the text colour, with its own alpha, in
                        // the filter.
                        let opacity = ffi::ImpellerColor {
                            red: 1.0,
                            green: 1.0,
                            blue: 1.0,
                            alpha: self.current_opacity,
                            color_space: ffi::ImpellerColorSpace::SRGB,
                        };
                        ffi::ImpellerPaintSetColor(paint, &opacity as *const _);
                        let filter = ffi::ImpellerColorFilterCreateBlendNew(
                            &text_color as *const _,
                            ffi::ImpellerBlendMode::SourceIn,
                        );
                        ffi::ImpellerPaintSetColorFilter(paint, filter);
                        (paint, filter)
                    })
                    .0
            };

            let src_rect = ffi::ImpellerRect {
                x: sx as f32,
                y: sy as f32,
                width: width as f32,
                height: height as f32,
            };
            let dst_rect = ffi::ImpellerRect {
                x: x + left as f32,
                y: y + top as f32,
                width: width as f32,
                height: height as f32,
            };
            unsafe {
                ffi::ImpellerDisplayListBuilderDrawTextureRect(
                    self.builder,
                    texture,
                    &src_rect as *const _,
                    &dst_rect as *const _,
                    ffi::ImpellerTextureSampling::Linear,
                    paint,
                );
            }
        }

        unsafe {
            if let Some((paint, filter)) = mask_paint {
                ffi::ImpellerColorFilterRelease(filter);
                ffi::ImpellerPaintRelease(paint);
            }
            if let Some(paint) = color_paint {
                ffi::ImpellerPaintRelease(paint);
            }
        }
    }

    fn fill_rectangle(
        &mut self,
        physical_rect: sharedparley::PhysicalRect,
        brush: Color,
        radius: sharedparley::PhysicalLength,
        border: Option<sharedparley::RectangleBorder<Color>>,
    ) {
        let rect = ffi::ImpellerRect {
            x: physical_rect.min_x(),
            y: physical_rect.min_y(),
            width: physical_rect.width(),
            height: physical_rect.height(),
        };
        let radius = radius.get();
        let radii = ffi::ImpellerRoundingRadii {
            top_left: ffi::ImpellerPoint { x: radius, y: radius },
            bottom_left: ffi::ImpellerPoint { x: radius, y: radius },
            top_right: ffi::ImpellerPoint { x: radius, y: radius },
            bottom_right: ffi::ImpellerPoint { x: radius, y: radius },
        };

        let draw = |color: &Color, style: ffi::ImpellerDrawStyle, stroke_width: f32| unsafe {
            let mut color = self.color_to_impeller(color);
            color.alpha *= self.current_opacity;
            let paint = ffi::ImpellerPaintNew();
            ffi::ImpellerPaintSetColor(paint, &color as *const _);
            ffi::ImpellerPaintSetDrawStyle(paint, style);
            if style == ffi::ImpellerDrawStyle::Stroke {
                ffi::ImpellerPaintSetStrokeWidth(paint, stroke_width);
            }
            if radius <= 0.0 {
                ffi::ImpellerDisplayListBuilderDrawRect(self.builder, &rect as *const _, paint);
            } else {
                ffi::ImpellerDisplayListBuilderDrawRoundedRect(
                    self.builder,
                    &rect as *const _,
                    &radii as *const _,
                    paint,
                );
            }
            ffi::ImpellerPaintRelease(paint);
        };

        draw(&brush, ffi::ImpellerDrawStyle::Fill, 0.0);
        if let Some(sharedparley::RectangleBorder { brush, width }) = border
            && width.get() > 0.0
        {
            draw(&brush, ffi::ImpellerDrawStyle::Stroke, width.get());
        }
    }
}
