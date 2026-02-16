// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

#![doc = include_str!("README.md")]
#![doc(html_logo_url = "https://slint.dev/logo/slint-logo-square-light.svg")]

use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use i_slint_common::sharedfontique;
use i_slint_core::api::{
    PhysicalSize as PhysicalWindowSize, RenderingNotifier, SetRenderingNotifierError,
};
use i_slint_core::item_tree::ItemTreeWeak;
use i_slint_core::lengths::{LogicalLength, LogicalPoint, LogicalRect, LogicalSize, ScaleFactor};
use i_slint_core::platform::PlatformError;
use i_slint_core::textlayout::sharedparley;
use i_slint_core::window::{WindowAdapter, WindowInner};
use i_slint_core::Brush;

mod ffi;
mod itemrenderer;

#[cfg(test)]
mod tests;

use glutin::{
    context::{ContextApi, ContextAttributesBuilder},
    display::GetGlDisplay,
    prelude::*,
    surface::{SurfaceAttributesBuilder, WindowSurface},
};

pub struct ImpellerRenderer {
    maybe_window_adapter: RefCell<Option<Weak<dyn WindowAdapter>>>,
    impeller_context: RefCell<Option<ffi::ImpellerContext>>,
    glutin_context: RefCell<Option<glutin::context::PossiblyCurrentContext>>,
    glutin_surface: RefCell<Option<glutin::surface::Surface<glutin::surface::WindowSurface>>>,
    size: Cell<PhysicalWindowSize>,
}

impl ImpellerRenderer {
    pub fn new() -> Self {
        Self {
            maybe_window_adapter: RefCell::new(None),
            impeller_context: RefCell::new(None),
            glutin_context: RefCell::new(None),
            glutin_surface: RefCell::new(None),
            size: Cell::new(PhysicalWindowSize::default()),
        }
    }

    pub fn set_window_handle(
        &self,
        window_handle: Arc<dyn raw_window_handle::HasWindowHandle + Send + Sync>,
        display_handle: Arc<dyn raw_window_handle::HasDisplayHandle + Send + Sync>,
        size: PhysicalWindowSize,
    ) -> Result<(), PlatformError> {
        let window_handle = window_handle.window_handle().map_err(|e| {
            format!("error obtaining window handle for impeller renderer: {e}")
        })?;
        let display_handle = display_handle.display_handle().map_err(|e| {
            format!("error obtaining display handle for impeller renderer: {e}")
        })?;

        let width = NonZeroU32::new(size.width)
            .ok_or_else(|| "Impeller Renderer: Window width is zero".to_string())?;
        let height = NonZeroU32::new(size.height)
            .ok_or_else(|| "Impeller Renderer: Window height is zero".to_string())?;

        let (glutin_context, glutin_surface) =
            self.init_glutin(window_handle, display_handle, width, height)?;

        glutin_surface.resize(&glutin_context, width, height);

        let impeller_context = unsafe {
            ffi::ImpellerContextCreateOpenGLESNew(
                ffi::IMPELLER_VERSION,
                Self::gl_proc_address_callback,
                &glutin_context as *const _ as *mut std::ffi::c_void,
            )
        };

        if impeller_context.is_null() {
            return Err("Failed to create Impeller context".to_string().into());
        }

        *self.impeller_context.borrow_mut() = Some(impeller_context);
        *self.glutin_context.borrow_mut() = Some(glutin_context);
        *self.glutin_surface.borrow_mut() = Some(glutin_surface);
        self.size.set(size);

        Ok(())
    }

    unsafe extern "C" fn gl_proc_address_callback(
        name: *const std::os::raw::c_char,
        user_data: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void {
        unsafe {
            let glutin_context =
                &*(user_data as *const glutin::context::PossiblyCurrentContext);
            let name_cstr = std::ffi::CStr::from_ptr(name);
            glutin_context.display().get_proc_address(name_cstr) as *mut std::ffi::c_void
        }
    }

    fn init_glutin(
        &self,
        window_handle: raw_window_handle::WindowHandle<'_>,
        display_handle: raw_window_handle::DisplayHandle<'_>,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<
        (
            glutin::context::PossiblyCurrentContext,
            glutin::surface::Surface<glutin::surface::WindowSurface>,
        ),
        PlatformError,
    > {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "macos")] {
                let display_api_preference = glutin::display::DisplayApiPreference::Cgl;
            } else if #[cfg(not(target_family = "windows"))] {
                let display_api_preference = glutin::display::DisplayApiPreference::Egl;
            } else {
                let display_api_preference = glutin::display::DisplayApiPreference::EglThenWgl(Some(window_handle.as_raw()));
            }
        }

        let gl_display = unsafe {
            glutin::display::Display::new(display_handle.as_raw(), display_api_preference)
                .map_err(|glutin_error| {
                    format!(
                        "Error creating glutin display for native display {:?}: {}",
                        display_handle.as_raw(),
                        glutin_error
                    )
                })?
        };

        #[cfg(target_os = "macos")]
        let config_template_builder =
            glutin::config::ConfigTemplateBuilder::new().with_transparency(true);

        #[cfg(not(target_os = "macos"))]
        let config_template_builder = glutin::config::ConfigTemplateBuilder::new();

        #[cfg(target_family = "windows")]
        let config_template_builder =
            config_template_builder.compatible_with_native_window(window_handle.as_raw());

        let config_template = config_template_builder.build();

        let config = unsafe {
            gl_display
                .find_configs(config_template)
                .map_err(|e| format!("Could not find valid OpenGL display configurations: {e}"))?
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .ok_or("Unable to find suitable GL config")?
        };

        let gles3_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(glutin::context::Version {
                major: 3,
                minor: 0,
            })))
            .build(Some(window_handle.as_raw()));

        let gles2_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(glutin::context::Version {
                major: 2,
                minor: 0,
            })))
            .build(Some(window_handle.as_raw()));

        let fallback_context_attributes =
            ContextAttributesBuilder::new().build(Some(window_handle.as_raw()));

        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&config, &gles3_context_attributes)
                .or_else(|_| gl_display.create_context(&config, &gles2_context_attributes))
                .or_else(|_| gl_display.create_context(&config, &fallback_context_attributes))
                .map_err(|e| format!("Error creating OpenGL context: {e}"))?
        };

        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window_handle.as_raw(),
            width,
            height,
        );

        let surface = unsafe {
            config
                .display()
                .create_window_surface(&config, &attrs)
                .map_err(|e| format!("Error creating OpenGL window surface: {e}"))?
        };

        let context = not_current_gl_context.make_current(&surface).map_err(
            |glutin_error: glutin::error::Error| -> PlatformError {
                format!(
                    "Impeller Renderer: Failed to make newly created OpenGL context current: {}",
                    glutin_error
                )
                .into()
            },
        )?;

        #[cfg(target_os = "macos")]
        if let raw_window_handle::RawWindowHandle::AppKit(raw_window_handle::AppKitWindowHandle {
            ns_view,
            ..
        }) = window_handle.as_raw()
        {
            let ns_view: &objc2_app_kit::NSView = unsafe { ns_view.cast().as_ref() };
            ns_view.setLayerContentsPlacement(objc2_app_kit::NSViewLayerContentsPlacement::TopLeft);
        }

        if context
            .display()
            .get_proc_address(&std::ffi::CString::new("glCreateShader").unwrap())
            .is_null()
        {
            return Err(
                "Failed to initialize OpenGL driver: Could not locate glCreateShader symbol"
                    .to_string()
                    .into(),
            );
        }

        surface
            .set_swap_interval(
                &context,
                glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
            )
            .ok();

        Ok((context, surface))
    }

    pub fn render(&self) -> Result<(), PlatformError> {
        let glutin_context = self.glutin_context.borrow();
        let Some(ref context) = *glutin_context else {
            return Ok(());
        };

        let impeller_context = self.impeller_context.borrow();
        let Some(impeller_ctx) = *impeller_context else {
            return Ok(());
        };

        context.make_current(self.glutin_surface.borrow().as_ref().unwrap()).map_err(
            |e| -> PlatformError {
                format!("Impeller Renderer: Failed to make context current: {}", e).into()
            },
        )?;

        let size = self.size.get();
        let window_adapter = self.window_adapter()?;
        let window = window_adapter.window();

        let impeller_size = ffi::ImpellerISize {
            width: size.width as i64,
            height: size.height as i64,
        };

        let impeller_surface = unsafe {
            ffi::ImpellerSurfaceCreateWrappedFBONew(
                impeller_ctx,
                0,
                ffi::ImpellerPixelFormat::RGBA8888,
                &impeller_size as *const _,
            )
        };

        if impeller_surface.is_null() {
            return Err("Failed to create Impeller surface".to_string().into());
        }

        let cull_rect = ffi::ImpellerRect {
            x: 0.0,
            y: 0.0,
            width: size.width as f32,
            height: size.height as f32,
        };

        let display_list_builder =
            unsafe { ffi::ImpellerDisplayListBuilderNew(&cull_rect as *const _) };

        if display_list_builder.is_null() {
            unsafe {
                ffi::ImpellerSurfaceRelease(impeller_surface);
            }
            return Err("Failed to create Impeller display list builder".to_string().into());
        }

        let window_inner = WindowInner::from_pub(&window);

        window_inner.draw_contents(|components| {
            let mut item_renderer = itemrenderer::ImpellerItemRenderer::new(
                display_list_builder,
                ScaleFactor::new(window_inner.scale_factor()),
                &window,
            );

            if let Some(window_item_rc) = window_inner.window_item_rc() {
                let window_item =
                    window_item_rc.downcast::<i_slint_core::items::WindowItem>().unwrap();
                if let Brush::SolidColor(clear_color) = window_item.as_pin_ref().background() {
                    item_renderer.clear_background(&clear_color);
                }
            }

            for (component, origin) in components {
                if let Some(component) = ItemTreeWeak::upgrade(component) {
                    i_slint_core::item_rendering::render_component_items(
                        &component,
                        &mut item_renderer,
                        *origin,
                        &window_adapter,
                    );
                }
            }
        });

        let display_list =
            unsafe { ffi::ImpellerDisplayListBuilderCreateDisplayListNew(display_list_builder) };

        unsafe {
            ffi::ImpellerDisplayListBuilderRelease(display_list_builder);
        }

        if display_list.is_null() {
            unsafe {
                ffi::ImpellerSurfaceRelease(impeller_surface);
            }
            return Err("Failed to create Impeller display list".to_string().into());
        }

        let draw_result =
            unsafe { ffi::ImpellerSurfaceDrawDisplayList(impeller_surface, display_list) };

        if !draw_result {
            unsafe {
                ffi::ImpellerDisplayListRelease(display_list);
                ffi::ImpellerSurfaceRelease(impeller_surface);
            }
            return Err("Failed to draw display list to Impeller surface".to_string().into());
        }

        self.glutin_surface.borrow().as_ref().unwrap().swap_buffers(context).map_err(
            |glutin_error| -> PlatformError {
                format!("Impeller Renderer: Error swapping buffers: {}", glutin_error).into()
            },
        )?;

        unsafe {
            ffi::ImpellerDisplayListRelease(display_list);
            ffi::ImpellerSurfaceRelease(impeller_surface);
        }

        Ok(())
    }

    pub fn suspend(&self) {
        if let Some(context) = self.impeller_context.borrow_mut().take() {
            unsafe {
                ffi::ImpellerContextRelease(context);
            }
        }
        *self.glutin_surface.borrow_mut() = None;
        *self.glutin_context.borrow_mut() = None;
    }

    pub fn resize(&self, size: PhysicalWindowSize) -> Result<(), PlatformError> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        let glutin_context = self.glutin_context.borrow();
        let glutin_surface = self.glutin_surface.borrow();

        if let (Some(context), Some(surface)) = (glutin_context.as_ref(), glutin_surface.as_ref()) {
            if let Some((width, height)) = size.width.try_into().ok().zip(size.height.try_into().ok()) {
                surface.resize(context, width, height);
            }
        }

        self.size.set(size);

        Ok(())
    }

    fn window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.maybe_window_adapter.borrow().as_ref().and_then(|w| w.upgrade()).ok_or_else(|| {
            "Renderer must be associated with component before use".to_string().into()
        })
    }
}

impl i_slint_core::renderer::RendererSealed for ImpellerRenderer {
    fn text_size(
        &self,
        text_item: Pin<&dyn i_slint_core::item_rendering::RenderString>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        max_width: Option<LogicalLength>,
        text_wrap: i_slint_core::items::TextWrap,
    ) -> LogicalSize {
        sharedparley::text_size(self, text_item, item_rc, max_width, text_wrap)
    }

    fn char_size(
        &self,
        text_item: Pin<&dyn i_slint_core::item_rendering::HasFont>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        ch: char,
    ) -> LogicalSize {
        sharedparley::char_size(text_item, item_rc, ch).unwrap_or_default()
    }

    fn font_metrics(
        &self,
        font_request: i_slint_core::graphics::FontRequest,
    ) -> i_slint_core::items::FontMetrics {
        sharedparley::font_metrics(font_request)
    }

    fn text_input_byte_offset_for_position(
        &self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        pos: LogicalPoint,
    ) -> usize {
        sharedparley::text_input_byte_offset_for_position(self, text_input, item_rc, pos)
    }

    fn text_input_cursor_rect_for_byte_offset(
        &self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        byte_offset: usize,
    ) -> LogicalRect {
        sharedparley::text_input_cursor_rect_for_byte_offset(self, text_input, item_rc, byte_offset)
    }

    fn register_font_from_memory(
        &self,
        data: &'static [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        sharedfontique::get_collection().register_fonts(data.to_vec().into(), None);
        Ok(())
    }

    fn register_font_from_path(
        &self,
        path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let requested_path = path.canonicalize().unwrap_or_else(|_| path.into());
        let contents = std::fs::read(requested_path)?;
        sharedfontique::get_collection().register_fonts(contents.into(), None);
        Ok(())
    }

    fn set_rendering_notifier(
        &self,
        _callback: Box<dyn RenderingNotifier>,
    ) -> std::result::Result<(), SetRenderingNotifierError> {
        Err(SetRenderingNotifierError::Unsupported)
    }

    fn default_font_size(&self) -> LogicalLength {
        sharedparley::DEFAULT_FONT_SIZE
    }

    fn free_graphics_resources(
        &self,
        _component: i_slint_core::item_tree::ItemTreeRef,
        _items: &mut dyn Iterator<Item = Pin<i_slint_core::items::ItemRef<'_>>>,
    ) -> Result<(), PlatformError> {
        Ok(())
    }

    fn set_window_adapter(&self, window_adapter: &Rc<dyn WindowAdapter>) {
        *self.maybe_window_adapter.borrow_mut() = Some(Rc::downgrade(window_adapter));
    }

    fn window_adapter(&self) -> Option<Rc<dyn WindowAdapter>> {
        self.maybe_window_adapter
            .borrow()
            .as_ref()
            .and_then(|window_adapter| window_adapter.upgrade())
    }

    fn resize(&self, size: PhysicalWindowSize) -> Result<(), PlatformError> {
        self.resize(size)
    }

    fn take_snapshot(
        &self,
    ) -> Result<
        i_slint_core::graphics::SharedPixelBuffer<i_slint_core::graphics::Rgba8Pixel>,
        PlatformError,
    > {
        Err("Taking snapshots is not yet implemented for the Impeller renderer".to_string().into())
    }

    fn mark_dirty_region(&self, _region: i_slint_core::partial_renderer::DirtyRegion) {}

    fn supports_transformations(&self) -> bool {
        true
    }
}

impl Default for ImpellerRenderer {
    fn default() -> Self {
        Self::new()
    }
}
