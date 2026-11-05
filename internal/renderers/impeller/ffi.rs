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
pub type ImpellerTexture = *mut c_void;
pub type ImpellerColorSource = *mut c_void;
pub type ImpellerColorFilter = *mut c_void;
pub type ImpellerPath = *mut c_void;
pub type ImpellerPathBuilder = *mut c_void;
pub type ImpellerMaskFilter = *mut c_void;

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
#[allow(dead_code)]
pub enum ImpellerBlendMode {
    Clear = 0,
    Source = 1,
    Destination = 2,
    SourceOver = 3,
    DestinationOver = 4,
    SourceIn = 5,
    DestinationIn = 6,
    SourceOut = 7,
    DestinationOut = 8,
    SourceATop = 9,
    DestinationATop = 10,
    Xor = 11,
    Plus = 12,
    Modulate = 13,
    Screen = 14,
    Overlay = 15,
    Darken = 16,
    Lighten = 17,
    ColorDodge = 18,
    ColorBurn = 19,
    HardLight = 20,
    SoftLight = 21,
    Difference = 22,
    Exclusion = 23,
    Multiply = 24,
    Hue = 25,
    Saturation = 26,
    Color = 27,
    Luminosity = 28,
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
pub enum ImpellerTextureSampling {
    NearestNeighbor = 0,
    Linear = 1,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerTileMode {
    Clamp = 0,
    Repeat = 1,
    Mirror = 2,
    Decal = 3,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerFillType {
    NonZero = 0,
    Odd = 1,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerStrokeCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImpellerStrokeJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ImpellerBlurStyle {
    Normal = 0,
    Solid = 1,
    Outer = 2,
    Inner = 3,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerMatrix {
    pub m: [f32; 16],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ImpellerTextureDescriptor {
    pub pixel_format: ImpellerPixelFormat,
    pub size: ImpellerISize,
    pub mip_count: u32,
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

    pub fn ImpellerDisplayListBuilderSaveLayer(
        builder: ImpellerDisplayListBuilder,
        bounds: *const ImpellerRect,
        paint: ImpellerPaint,
        backdrop: *mut c_void,
    );

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

    // Texture
    pub fn ImpellerTextureCreateWithContentsNew(
        context: ImpellerContext,
        descriptor: *const ImpellerTextureDescriptor,
        contents: *const ImpellerMapping,
        contents_on_release_user_data: *mut c_void,
    ) -> ImpellerTexture;

    pub fn ImpellerTextureRetain(texture: ImpellerTexture);
    pub fn ImpellerTextureRelease(texture: ImpellerTexture);

    // Display list builder - draw textures
    pub fn ImpellerDisplayListBuilderDrawTexture(
        builder: ImpellerDisplayListBuilder,
        texture: ImpellerTexture,
        point: *const ImpellerPoint,
        sampling: ImpellerTextureSampling,
        paint: ImpellerPaint,
    );

    // Paint - color source
    pub fn ImpellerPaintSetColorSource(paint: ImpellerPaint, color_source: ImpellerColorSource);

    // Color source - gradients
    pub fn ImpellerColorSourceRelease(color_source: ImpellerColorSource);

    pub fn ImpellerColorSourceCreateLinearGradientNew(
        start_point: *const ImpellerPoint,
        end_point: *const ImpellerPoint,
        stop_count: u32,
        colors: *const ImpellerColor,
        stops: *const f32,
        tile_mode: ImpellerTileMode,
        transformation: *const ImpellerMatrix,
    ) -> ImpellerColorSource;

    pub fn ImpellerColorSourceCreateRadialGradientNew(
        center: *const ImpellerPoint,
        radius: f32,
        stop_count: u32,
        colors: *const ImpellerColor,
        stops: *const f32,
        tile_mode: ImpellerTileMode,
        transformation: *const ImpellerMatrix,
    ) -> ImpellerColorSource;

    pub fn ImpellerColorSourceCreateConicalGradientNew(
        start_center: *const ImpellerPoint,
        start_radius: f32,
        end_center: *const ImpellerPoint,
        end_radius: f32,
        stop_count: u32,
        colors: *const ImpellerColor,
        stops: *const f32,
        tile_mode: ImpellerTileMode,
        transformation: *const ImpellerMatrix,
    ) -> ImpellerColorSource;

    pub fn ImpellerColorSourceCreateSweepGradientNew(
        center: *const ImpellerPoint,
        start: f32,
        end: f32,
        stop_count: u32,
        colors: *const ImpellerColor,
        stops: *const f32,
        tile_mode: ImpellerTileMode,
        transformation: *const ImpellerMatrix,
    ) -> ImpellerColorSource;

    pub fn ImpellerDisplayListBuilderDrawTextureRect(
        builder: ImpellerDisplayListBuilder,
        texture: ImpellerTexture,
        src_rect: *const ImpellerRect,
        dst_rect: *const ImpellerRect,
        sampling: ImpellerTextureSampling,
        paint: ImpellerPaint,
    );

    // Color filter
    pub fn ImpellerColorFilterCreateBlendNew(
        color: *const ImpellerColor,
        blend_mode: ImpellerBlendMode,
    ) -> ImpellerColorFilter;

    pub fn ImpellerColorFilterRelease(color_filter: ImpellerColorFilter);

    pub fn ImpellerPaintSetColorFilter(paint: ImpellerPaint, color_filter: ImpellerColorFilter);

    // Paths
    pub fn ImpellerPathRelease(path: ImpellerPath);
    pub fn ImpellerPathBuilderNew() -> ImpellerPathBuilder;
    pub fn ImpellerPathBuilderRelease(builder: ImpellerPathBuilder);
    pub fn ImpellerPathBuilderMoveTo(builder: ImpellerPathBuilder, location: *const ImpellerPoint);
    pub fn ImpellerPathBuilderLineTo(builder: ImpellerPathBuilder, location: *const ImpellerPoint);
    pub fn ImpellerPathBuilderQuadraticCurveTo(
        builder: ImpellerPathBuilder,
        control_point: *const ImpellerPoint,
        end_point: *const ImpellerPoint,
    );
    pub fn ImpellerPathBuilderCubicCurveTo(
        builder: ImpellerPathBuilder,
        control_point_1: *const ImpellerPoint,
        control_point_2: *const ImpellerPoint,
        end_point: *const ImpellerPoint,
    );
    pub fn ImpellerPathBuilderAddRect(builder: ImpellerPathBuilder, rect: *const ImpellerRect);
    pub fn ImpellerPathBuilderAddRoundedRect(
        builder: ImpellerPathBuilder,
        rect: *const ImpellerRect,
        rounding_radii: *const ImpellerRoundingRadii,
    );
    pub fn ImpellerPathBuilderClose(builder: ImpellerPathBuilder);
    pub fn ImpellerPathBuilderTakePathNew(
        builder: ImpellerPathBuilder,
        fill: ImpellerFillType,
    ) -> ImpellerPath;
    pub fn ImpellerDisplayListBuilderDrawPath(
        builder: ImpellerDisplayListBuilder,
        path: ImpellerPath,
        paint: ImpellerPaint,
    );
    pub fn ImpellerDisplayListBuilderClipRoundedRect(
        builder: ImpellerDisplayListBuilder,
        rect: *const ImpellerRect,
        radii: *const ImpellerRoundingRadii,
        op: ImpellerClipOperation,
    );
    pub fn ImpellerPaintSetStrokeCap(paint: ImpellerPaint, cap: ImpellerStrokeCap);
    pub fn ImpellerPaintSetStrokeJoin(paint: ImpellerPaint, join: ImpellerStrokeJoin);
    pub fn ImpellerPaintSetStrokeMiter(paint: ImpellerPaint, miter: f32);

    // Mask filter
    pub fn ImpellerMaskFilterCreateBlurNew(
        style: ImpellerBlurStyle,
        sigma: f32,
    ) -> ImpellerMaskFilter;
    pub fn ImpellerMaskFilterRelease(mask_filter: ImpellerMaskFilter);
    pub fn ImpellerPaintSetMaskFilter(paint: ImpellerPaint, mask_filter: ImpellerMaskFilter);
}
