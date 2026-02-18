// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::ffi::CString;
use std::pin::Pin;

use i_slint_core::graphics::{euclid, Image, ImageInner, SharedImageBuffer};
use i_slint_core::item_rendering::{
    CachedRenderingData, ItemRenderer, PlainOrStyledText, RenderImage, RenderText,
};
use i_slint_core::items::{
    ImageFit, ImageRendering, ItemRc, Layer, Opacity, RenderingResult, TextHorizontalAlignment,
    TextVerticalAlignment,
};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalLength, LogicalPoint, LogicalRect, LogicalSize, LogicalVector,
    PhysicalPx, ScaleFactor,
};
use i_slint_core::{Brush, Color};

use crate::ffi;

fn font_weight_to_impeller(weight: Option<i32>) -> ffi::ImpellerFontWeight {
    match weight.unwrap_or(400) {
        w if w <= 150 => ffi::ImpellerFontWeight::W100,
        w if w <= 250 => ffi::ImpellerFontWeight::W200,
        w if w <= 350 => ffi::ImpellerFontWeight::W300,
        w if w <= 450 => ffi::ImpellerFontWeight::W400,
        w if w <= 550 => ffi::ImpellerFontWeight::W500,
        w if w <= 650 => ffi::ImpellerFontWeight::W600,
        w if w <= 750 => ffi::ImpellerFontWeight::W700,
        w if w <= 850 => ffi::ImpellerFontWeight::W800,
        _ => ffi::ImpellerFontWeight::W900,
    }
}

pub struct ImpellerItemRenderer<'a> {
    builder: ffi::ImpellerDisplayListBuilder,
    scale_factor: ScaleFactor,
    window: &'a i_slint_core::api::Window,
    typography_context: ffi::ImpellerTypographyContext,
    impeller_context: ffi::ImpellerContext,
    current_opacity: f32,
    opacity_stack: Vec<f32>,
}

impl<'a> ImpellerItemRenderer<'a> {
    pub fn new(
        builder: ffi::ImpellerDisplayListBuilder,
        scale_factor: ScaleFactor,
        window: &'a i_slint_core::api::Window,
        typography_context: ffi::ImpellerTypographyContext,
        impeller_context: ffi::ImpellerContext,
    ) -> Self {
        Self { builder, scale_factor, window, typography_context, impeller_context, current_opacity: 1.0, opacity_stack: Vec::new() }
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
    fn line_for_angle(angle: f32, width: f32, height: f32) -> (ffi::ImpellerPoint, ffi::ImpellerPoint) {
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
            (
                ffi::ImpellerPoint { x, y },
                ffi::ImpellerPoint { x: width - x, y: height - y },
            )
        } else {
            let x = (s * width - c * height) * s / 2.0;
            let y = -c * x / s;
            (
                ffi::ImpellerPoint { x: width - x, y: height - y },
                ffi::ImpellerPoint { x, y },
            )
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

                let (start, end) =
                    Self::line_for_angle(gradient.angle(), rect.width, rect.height);

                let start_point =
                    ffi::ImpellerPoint { x: rect.x + start.x, y: rect.y + start.y };
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
                let radius =
                    0.5 * (rect.width * rect.width + rect.height * rect.height).sqrt();
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

    fn to_physical_rect(&self, rect: LogicalRect) -> ffi::ImpellerRect {
        let physical = rect * self.scale_factor;
        ffi::ImpellerRect {
            x: physical.origin.x,
            y: physical.origin.y,
            width: physical.size.width,
            height: physical.size.height,
        }
    }

    /// Create an Impeller texture from RGBA8 pixel data.
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
    ) -> Option<(ffi::ImpellerTexture, u32, u32)> {
        let image_inner: &ImageInner = image.into();
        let buffer = image_inner.render_to_buffer(target_size)?;
        let (rgba_data, width, height) = match &buffer {
            SharedImageBuffer::RGBA8(buf) => (buf.as_bytes(), buf.width(), buf.height()),
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
                return Some((texture, w, h));
            }
        };
        if width == 0 || height == 0 {
            return None;
        }
        let texture = self.create_texture_from_rgba(rgba_data, width, height);
        if texture.is_null() {
            return None;
        }
        Some((texture, width, height))
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
            let Some((paint, color_source)) =
                self.setup_paint_for_brush(&brush, &impeller_rect)
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
        _self_rc: &ItemRc,
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
                Some(euclid::Size2D::<u32, PhysicalPx>::new(
                    tw.ceil() as u32,
                    th.ceil() as u32,
                ))
            } else {
                None
            }
        };

        let (texture, src_width, src_height) = match self.image_to_texture(&source, target_size) {
            Some(t) => t,
            None => return,
        };

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
                let scaled_w = src_w * ratio;
                let scaled_h = src_h * ratio;
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

            let dst_rect =
                ffi::ImpellerRect { x: dst_x, y: dst_y, width: dst_w, height: dst_h };

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
                let src_rect = ffi::ImpellerRect {
                    x: 0.0,
                    y: 0.0,
                    width: src_w,
                    height: src_h,
                };
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
            ffi::ImpellerTextureRelease(texture);
        }
    }

    fn draw_text(
        &mut self,
        text: Pin<&dyn RenderText>,
        self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        if self.typography_context.is_null() {
            return;
        }

        let plain_text = text.text();
        let string: std::borrow::Cow<'_, str> = match &plain_text {
            PlainOrStyledText::Plain(s) => s.as_str().into(),
            PlainOrStyledText::Styled(styled) => {
                i_slint_core::styled_text::get_raw_text(styled)
            }
        };
        if string.is_empty() {
            return;
        }

        let Some(mut color) = self.brush_to_color(&text.color()) else { return };
        color.alpha *= self.current_opacity;
        let (h_align, v_align) = text.alignment();
        let font_request = text.font_request(self_rc);
        let sf = self.scale_factor.get();

        let font_size = font_request
            .pixel_size
            .map(|s| s.get() * sf)
            .unwrap_or(16.0 * sf);

        // Resolve the font family name. When no family is specified, query fontique
        // for the default sans-serif font (matching what Skia/FemtoVG do).
        let resolved_family: Option<String> = if font_request.family.is_some() {
            font_request.family.as_ref().map(|f| f.to_string())
        } else {
            font_request.query_fontique().and_then(|font| {
                let mut collection = i_slint_common::sharedfontique::get_collection();
                collection.family_name(font.family.0).map(|s| s.to_string())
            })
        };

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if paint.is_null() {
                return;
            }
            ffi::ImpellerPaintSetColor(paint, &color as *const _);

            let style = ffi::ImpellerParagraphStyleNew();
            if style.is_null() {
                ffi::ImpellerPaintRelease(paint);
                return;
            }

            ffi::ImpellerParagraphStyleSetForeground(style, paint);
            ffi::ImpellerParagraphStyleSetFontSize(style, font_size);

            if let Some(ref family) = resolved_family {
                if !family.is_empty() {
                    if let Ok(c_family) = CString::new(family.as_str()) {
                        ffi::ImpellerParagraphStyleSetFontFamily(style, c_family.as_ptr());
                    }
                }
            }

            ffi::ImpellerParagraphStyleSetFontWeight(
                style,
                font_weight_to_impeller(font_request.weight),
            );

            if font_request.italic {
                ffi::ImpellerParagraphStyleSetFontStyle(style, ffi::ImpellerFontStyle::Italic);
            }

            let imp_align = match h_align {
                TextHorizontalAlignment::Center => ffi::ImpellerTextAlignment::Center,
                TextHorizontalAlignment::Right => ffi::ImpellerTextAlignment::Right,
                _ => ffi::ImpellerTextAlignment::Left,
            };
            ffi::ImpellerParagraphStyleSetTextAlignment(style, imp_align);

            let para_builder = ffi::ImpellerParagraphBuilderNew(self.typography_context);
            if para_builder.is_null() {
                ffi::ImpellerParagraphStyleRelease(style);
                ffi::ImpellerPaintRelease(paint);
                return;
            }

            ffi::ImpellerParagraphBuilderPushStyle(para_builder, style);

            let text_bytes = string.as_bytes();
            ffi::ImpellerParagraphBuilderAddText(
                para_builder,
                text_bytes.as_ptr(),
                text_bytes.len() as u32,
            );

            ffi::ImpellerParagraphBuilderPopStyle(para_builder);

            let layout_width = size.width * sf;
            let paragraph =
                ffi::ImpellerParagraphBuilderBuildParagraphNew(para_builder, layout_width);

            if !paragraph.is_null() {
                let para_height = ffi::ImpellerParagraphGetHeight(paragraph);
                let physical_height = size.height * sf;

                let y_offset = match v_align {
                    TextVerticalAlignment::Center => (physical_height - para_height) / 2.0,
                    TextVerticalAlignment::Bottom => physical_height - para_height,
                    _ => 0.0,
                };
                let y_offset = y_offset.max(0.0);

                let point = ffi::ImpellerPoint { x: 0.0, y: y_offset };
                ffi::ImpellerDisplayListBuilderDrawParagraph(
                    self.builder,
                    paragraph,
                    &point as *const _,
                );
                ffi::ImpellerParagraphRelease(paragraph);
            }

            ffi::ImpellerParagraphBuilderRelease(para_builder);
            ffi::ImpellerParagraphStyleRelease(style);
            ffi::ImpellerPaintRelease(paint);
        }
    }

    fn draw_text_input(
        &mut self,
        _text_input: Pin<&i_slint_core::items::TextInput>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) {
    }

    fn draw_path(
        &mut self,
        _path: Pin<&i_slint_core::items::Path>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) {
    }

    fn draw_box_shadow(
        &mut self,
        _box_shadow: Pin<&i_slint_core::items::BoxShadow>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) {
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

    fn combine_clip(
        &mut self,
        rect: LogicalRect,
        _radius: LogicalBorderRadius,
        _border_width: LogicalLength,
    ) -> bool {
        let impeller_rect = self.to_physical_rect(rect);
        unsafe {
            ffi::ImpellerDisplayListBuilderClipRect(
                self.builder,
                &impeller_rect as *const _,
                ffi::ImpellerClipOperation::Intersect,
            );
        }
        true
    }

    fn get_current_clip(&self) -> LogicalRect {
        let window_size = self.window.size();
        LogicalRect::new(
            LogicalPoint::default(),
            LogicalSize::new(window_size.width as f32, window_size.height as f32),
        )
    }

    fn translate(&mut self, distance: LogicalVector) {
        let physical_distance = distance * self.scale_factor;
        unsafe {
            ffi::ImpellerDisplayListBuilderTranslate(
                self.builder,
                physical_distance.x,
                physical_distance.y,
            );
        }
    }

    fn rotate(&mut self, angle_in_degrees: f32) {
        unsafe {
            ffi::ImpellerDisplayListBuilderRotate(self.builder, angle_in_degrees);
        }
    }

    fn scale(&mut self, x_factor: f32, y_factor: f32) {
        unsafe {
            ffi::ImpellerDisplayListBuilderScale(self.builder, x_factor, y_factor);
        }
    }

    fn apply_opacity(&mut self, opacity: f32) {
        self.current_opacity *= opacity;
    }

    fn save_state(&mut self) {
        self.opacity_stack.push(self.current_opacity);
        unsafe {
            ffi::ImpellerDisplayListBuilderSave(self.builder);
        }
    }

    fn restore_state(&mut self) {
        if let Some(opacity) = self.opacity_stack.pop() {
            self.current_opacity = opacity;
        }
        unsafe {
            ffi::ImpellerDisplayListBuilderRestore(self.builder);
        }
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    fn draw_cached_pixmap(
        &mut self,
        _item_rc: &ItemRc,
        _update_fn: &dyn Fn(&mut dyn FnMut(u32, u32, &[u8])),
    ) {
    }

    fn draw_string(&mut self, string: &str, color: Color) {
        if self.typography_context.is_null() || string.is_empty() {
            return;
        }

        let sf = self.scale_factor.get();
        let mut impeller_color = self.color_to_impeller(&color);
        impeller_color.alpha *= self.current_opacity;

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if paint.is_null() {
                return;
            }
            ffi::ImpellerPaintSetColor(paint, &impeller_color as *const _);

            let style = ffi::ImpellerParagraphStyleNew();
            if style.is_null() {
                ffi::ImpellerPaintRelease(paint);
                return;
            }

            ffi::ImpellerParagraphStyleSetForeground(style, paint);
            ffi::ImpellerParagraphStyleSetFontSize(style, 16.0 * sf);

            let para_builder = ffi::ImpellerParagraphBuilderNew(self.typography_context);
            if para_builder.is_null() {
                ffi::ImpellerParagraphStyleRelease(style);
                ffi::ImpellerPaintRelease(paint);
                return;
            }

            ffi::ImpellerParagraphBuilderPushStyle(para_builder, style);

            let text_bytes = string.as_bytes();
            ffi::ImpellerParagraphBuilderAddText(
                para_builder,
                text_bytes.as_ptr(),
                text_bytes.len() as u32,
            );

            ffi::ImpellerParagraphBuilderPopStyle(para_builder);

            let paragraph =
                ffi::ImpellerParagraphBuilderBuildParagraphNew(para_builder, f32::MAX);

            if !paragraph.is_null() {
                let point = ffi::ImpellerPoint { x: 0.0, y: 0.0 };
                ffi::ImpellerDisplayListBuilderDrawParagraph(
                    self.builder,
                    paragraph,
                    &point as *const _,
                );
                ffi::ImpellerParagraphRelease(paragraph);
            }

            ffi::ImpellerParagraphBuilderRelease(para_builder);
            ffi::ImpellerParagraphStyleRelease(style);
            ffi::ImpellerPaintRelease(paint);
        }
    }

    fn draw_image_direct(&mut self, image: i_slint_core::graphics::Image) {
        let (texture, _width, _height) = match self.image_to_texture(&image, None) {
            Some(t) => t,
            None => return,
        };

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
