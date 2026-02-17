// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::ffi::CString;
use std::pin::Pin;

use i_slint_core::item_rendering::{
    CachedRenderingData, ItemRenderer, PlainOrStyledText, RenderImage, RenderText,
};
use i_slint_core::items::{ItemRc, Layer, Opacity, RenderingResult, TextHorizontalAlignment, TextVerticalAlignment};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalLength, LogicalPoint, LogicalRect, LogicalSize, LogicalVector,
    ScaleFactor,
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
}

impl<'a> ImpellerItemRenderer<'a> {
    pub fn new(
        builder: ffi::ImpellerDisplayListBuilder,
        scale_factor: ScaleFactor,
        window: &'a i_slint_core::api::Window,
        typography_context: ffi::ImpellerTypographyContext,
    ) -> Self {
        Self { builder, scale_factor, window, typography_context }
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
                    Some(ffi::ImpellerColor {
                        red: color.red() as f32 / 255.0,
                        green: color.green() as f32 / 255.0,
                        blue: color.blue() as f32 / 255.0,
                        alpha: color.alpha() as f32 / 255.0,
                        color_space: ffi::ImpellerColorSpace::SRGB,
                    })
                }
            }
            _ => None,
        }
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

        let Some(color) = self.brush_to_color(&rect.background()) else { return };

        let impeller_rect = self.to_physical_rect(geometry);

        unsafe {
            let paint = ffi::ImpellerPaintNew();
            if !paint.is_null() {
                ffi::ImpellerPaintSetColor(paint, &color as *const _);
                ffi::ImpellerPaintSetDrawStyle(paint, ffi::ImpellerDrawStyle::Fill);
                ffi::ImpellerDisplayListBuilderDrawRect(
                    self.builder,
                    &impeller_rect as *const _,
                    paint,
                );
                ffi::ImpellerPaintRelease(paint);
            }
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

        if let Some(background_color) = self.brush_to_color(&rect.background()) {
            let impeller_rect = self.to_physical_rect(geometry);

            unsafe {
                let paint = ffi::ImpellerPaintNew();
                if !paint.is_null() {
                    ffi::ImpellerPaintSetColor(paint, &background_color as *const _);
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

                    ffi::ImpellerPaintRelease(paint);
                }
            }
        }

        if !border_color.is_transparent() && border_width > LogicalLength::new(0.0) {
            if let Some(stroke_color) = self.brush_to_color(&border_color) {
                let physical_border_width = border_width * self.scale_factor;
                let impeller_rect = self.to_physical_rect(geometry);

                unsafe {
                    let paint = ffi::ImpellerPaintNew();
                    if !paint.is_null() {
                        ffi::ImpellerPaintSetColor(paint, &stroke_color as *const _);
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

                        ffi::ImpellerPaintRelease(paint);
                    }
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
        _image: Pin<&dyn RenderImage>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
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

        let Some(color) = self.brush_to_color(&text.color()) else { return };
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
        _opacity_item: Pin<&Opacity>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) -> RenderingResult {
        RenderingResult::ContinueRenderingChildren
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

    fn apply_opacity(&mut self, _opacity: f32) {
    }

    fn save_state(&mut self) {
        unsafe {
            ffi::ImpellerDisplayListBuilderSave(self.builder);
        }
    }

    fn restore_state(&mut self) {
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
        let impeller_color = self.color_to_impeller(&color);

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

    fn draw_image_direct(&mut self, _image: i_slint_core::graphics::Image) {
    }

    fn window(&self) -> &i_slint_core::window::WindowInner {
        i_slint_core::window::WindowInner::from_pub(self.window)
    }

    fn as_any(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}
