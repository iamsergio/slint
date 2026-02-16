// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::rc::Rc;
use std::sync::Arc;

use crate::winitwindowadapter::physical_size_to_slint;
use i_slint_core::platform::PlatformError;
use i_slint_renderer_impeller::ImpellerRenderer;

pub struct WinitImpellerRenderer {
    renderer: ImpellerRenderer,
}

impl WinitImpellerRenderer {
    pub fn new_suspended(
        _shared_backend_data: &Rc<crate::SharedBackendData>,
    ) -> Result<Box<dyn super::WinitCompatibleRenderer>, PlatformError> {
        Ok(Box::new(Self { renderer: ImpellerRenderer::new() }))
    }
}

impl super::WinitCompatibleRenderer for WinitImpellerRenderer {
    fn render(&self, _window: &i_slint_core::api::Window) -> Result<(), PlatformError> {
        self.renderer.render()
    }

    fn as_core_renderer(&self) -> &dyn i_slint_core::renderer::Renderer {
        &self.renderer
    }

    fn suspend(&self) -> Result<(), PlatformError> {
        self.renderer.suspend();
        Ok(())
    }

    fn resume(
        &self,
        active_event_loop: &winit::event_loop::ActiveEventLoop,
        window_attributes: winit::window::WindowAttributes,
    ) -> Result<Arc<winit::window::Window>, PlatformError> {
        let winit_window = Arc::new(active_event_loop.create_window(window_attributes).map_err(
            |winit_os_error| {
                PlatformError::from(format!(
                    "Error creating native window for Impeller rendering: {}",
                    winit_os_error
                ))
            },
        )?);

        let size = winit_window.inner_size();

        self.renderer.set_window_handle(
            winit_window.clone(),
            winit_window.clone(),
            physical_size_to_slint(&size),
        )?;

        Ok(winit_window)
    }
}
