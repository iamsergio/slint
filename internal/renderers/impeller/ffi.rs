// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::ffi::c_void;
use std::os::raw::c_char;

pub const IMPELLER_VERSION: u32 = (1 << 29) | (1 << 22) | (4 << 12) | 0;

pub type ImpellerContext = *mut c_void;
pub type ImpellerSurface = *mut c_void;
pub type ImpellerDisplayListBuilder = *mut c_void;
pub type ImpellerDisplayList = *mut c_void;
pub type ImpellerPaint = *mut c_void;
pub type ImpellerTypographyContext = *mut c_void;
pub type ImpellerParagraph = *mut c_void;
pub type ImpellerParagraphBuilder = *mut c_void;
pub type ImpellerParagraphStyle = *mut c_void;

pub type ImpellerCallback = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerMapping {
    pub data: *const u8,
    pub length: u64,
    pub on_release: ImpellerCallback,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerISize {
    pub width: i64,
    pub height: i64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerPoint {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerRoundingRadii {
    pub top_left: ImpellerPoint,
    pub bottom_left: ImpellerPoint,
    pub top_right: ImpellerPoint,
    pub bottom_right: ImpellerPoint,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerColorSpace {
    SRGB = 0,
    ExtendedSRGB = 1,
    DisplayP3 = 2,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
    pub color_space: ImpellerColorSpace,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerPixelFormat {
    RGBA8888 = 0,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerDrawStyle {
    Fill = 0,
    Stroke = 1,
    StrokeAndFill = 2,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerClipOperation {
    Difference = 0,
    Intersect = 1,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerFontWeight {
    W100 = 0,
    W200 = 1,
    W300 = 2,
    W400 = 3,
    W500 = 4,
    W600 = 5,
    W700 = 6,
    W800 = 7,
    W900 = 8,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerFontStyle {
    Normal = 0,
    Italic = 1,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerTextAlignment {
    Left = 0,
    Right = 1,
    Center = 2,
    Justify = 3,
    Start = 4,
    End = 5,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerTextDirection {
    RTL = 0,
    LTR = 1,
}

pub type ImpellerProcAddressCallback =
    unsafe extern "C" fn(*const c_char, *mut c_void) -> *mut c_void;

unsafe extern "C" {
    pub fn ImpellerGetVersion() -> u32;

    pub fn ImpellerContextCreateOpenGLESNew(
        version: u32,
        gl_proc_address_callback: ImpellerProcAddressCallback,
        gl_proc_address_callback_user_data: *mut c_void,
    ) -> ImpellerContext;

    pub fn ImpellerContextRelease(context: ImpellerContext);

    pub fn ImpellerSurfaceCreateWrappedFBONew(
        context: ImpellerContext,
        fbo: u64,
        format: ImpellerPixelFormat,
        size: *const ImpellerISize,
    ) -> ImpellerSurface;

    pub fn ImpellerSurfaceDrawDisplayList(
        surface: ImpellerSurface,
        display_list: ImpellerDisplayList,
    ) -> bool;

    pub fn ImpellerSurfaceRelease(surface: ImpellerSurface);

    pub fn ImpellerPaintNew() -> ImpellerPaint;

    pub fn ImpellerPaintSetColor(paint: ImpellerPaint, color: *const ImpellerColor);

    pub fn ImpellerPaintSetDrawStyle(paint: ImpellerPaint, style: ImpellerDrawStyle);

    pub fn ImpellerPaintSetStrokeWidth(paint: ImpellerPaint, width: f32);

    pub fn ImpellerPaintRelease(paint: ImpellerPaint);

    pub fn ImpellerDisplayListBuilderNew(
        cull_rect: *const ImpellerRect,
    ) -> ImpellerDisplayListBuilder;

    pub fn ImpellerDisplayListBuilderSave(builder: ImpellerDisplayListBuilder);

    pub fn ImpellerDisplayListBuilderRestore(builder: ImpellerDisplayListBuilder);

    pub fn ImpellerDisplayListBuilderTranslate(
        builder: ImpellerDisplayListBuilder,
        x_translation: f32,
        y_translation: f32,
    );

    pub fn ImpellerDisplayListBuilderScale(
        builder: ImpellerDisplayListBuilder,
        x_scale: f32,
        y_scale: f32,
    );

    pub fn ImpellerDisplayListBuilderRotate(
        builder: ImpellerDisplayListBuilder,
        angle_degrees: f32,
    );

    pub fn ImpellerDisplayListBuilderClipRect(
        builder: ImpellerDisplayListBuilder,
        rect: *const ImpellerRect,
        op: ImpellerClipOperation,
    );

    pub fn ImpellerDisplayListBuilderDrawRect(
        builder: ImpellerDisplayListBuilder,
        rect: *const ImpellerRect,
        paint: ImpellerPaint,
    );

    pub fn ImpellerDisplayListBuilderDrawRoundedRect(
        builder: ImpellerDisplayListBuilder,
        rect: *const ImpellerRect,
        radii: *const ImpellerRoundingRadii,
        paint: ImpellerPaint,
    );

    pub fn ImpellerDisplayListBuilderCreateDisplayListNew(
        builder: ImpellerDisplayListBuilder,
    ) -> ImpellerDisplayList;

    pub fn ImpellerDisplayListBuilderRelease(builder: ImpellerDisplayListBuilder);

    pub fn ImpellerDisplayListRelease(display_list: ImpellerDisplayList);

    // Typography
    pub fn ImpellerTypographyContextNew() -> ImpellerTypographyContext;
    pub fn ImpellerTypographyContextRelease(context: ImpellerTypographyContext);
    pub fn ImpellerTypographyContextRegisterFont(
        context: ImpellerTypographyContext,
        contents: *const ImpellerMapping,
        user_data: *mut c_void,
        family_name_alias: *const c_char,
    ) -> bool;

    // Paragraph style
    pub fn ImpellerParagraphStyleNew() -> ImpellerParagraphStyle;
    pub fn ImpellerParagraphStyleRelease(style: ImpellerParagraphStyle);
    pub fn ImpellerParagraphStyleSetForeground(style: ImpellerParagraphStyle, paint: ImpellerPaint);
    pub fn ImpellerParagraphStyleSetFontSize(style: ImpellerParagraphStyle, size: f32);
    pub fn ImpellerParagraphStyleSetFontFamily(
        style: ImpellerParagraphStyle,
        family: *const c_char,
    );
    pub fn ImpellerParagraphStyleSetFontWeight(
        style: ImpellerParagraphStyle,
        weight: ImpellerFontWeight,
    );
    pub fn ImpellerParagraphStyleSetFontStyle(
        style: ImpellerParagraphStyle,
        font_style: ImpellerFontStyle,
    );
    pub fn ImpellerParagraphStyleSetTextAlignment(
        style: ImpellerParagraphStyle,
        alignment: ImpellerTextAlignment,
    );
    pub fn ImpellerParagraphStyleSetTextDirection(
        style: ImpellerParagraphStyle,
        direction: ImpellerTextDirection,
    );

    // Paragraph builder
    pub fn ImpellerParagraphBuilderNew(
        context: ImpellerTypographyContext,
    ) -> ImpellerParagraphBuilder;
    pub fn ImpellerParagraphBuilderRelease(builder: ImpellerParagraphBuilder);
    pub fn ImpellerParagraphBuilderPushStyle(
        builder: ImpellerParagraphBuilder,
        style: ImpellerParagraphStyle,
    );
    pub fn ImpellerParagraphBuilderPopStyle(builder: ImpellerParagraphBuilder);
    pub fn ImpellerParagraphBuilderAddText(
        builder: ImpellerParagraphBuilder,
        data: *const u8,
        length: u32,
    );
    pub fn ImpellerParagraphBuilderBuildParagraphNew(
        builder: ImpellerParagraphBuilder,
        width: f32,
    ) -> ImpellerParagraph;

    // Paragraph
    pub fn ImpellerParagraphRelease(paragraph: ImpellerParagraph);
    pub fn ImpellerParagraphGetHeight(paragraph: ImpellerParagraph) -> f32;

    // Display list builder - draw paragraph
    pub fn ImpellerDisplayListBuilderDrawParagraph(
        builder: ImpellerDisplayListBuilder,
        paragraph: ImpellerParagraph,
        point: *const ImpellerPoint,
    );
}
