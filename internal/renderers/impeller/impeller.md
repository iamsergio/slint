# Impeller Renderer for Slint

## What This Is

A Slint renderer backed by Flutter's [Impeller](https://github.com/flutter/engine/tree/main/impeller) graphics engine, linked via its C API (`libimpeller.so`). Currently supports rectangles, rounded rects, borders, text rendering, and transforms. Images, paths, box shadows, opacity compositing, and text input are not yet implemented.

## Build & Run

```bash
export IMPELLER_SDK_PATH=/pub_data/sources/slint/impeller_install_debug_unopt
export LD_LIBRARY_PATH=$IMPELLER_SDK_PATH/lib:$LD_LIBRARY_PATH
export SLINT_NO_QT=1 # TODO to be removed

# Build
cargo build -p slint-viewer --no-default-features --features backend-winit,renderer-impeller,renderer-skia  

# Run
SLINT_BACKEND=winit-impeller ./target/debug/slint-viewer -- examples/impeller_test.slint


The `IMPELLER_SDK_PATH` env var is required at build time (`build.rs` reads it to link `libimpeller.so`). At runtime, `LD_LIBRARY_PATH` must include the SDK's `lib/` directory.
```

## File Layout

```
internal/renderers/impeller/
  Cargo.toml          # crate: i-slint-renderer-impeller
  build.rs            # links libimpeller.so via IMPELLER_SDK_PATH
  lib.rs              # ImpellerRenderer + RendererSealed impl
  itemrenderer.rs     # ImpellerItemRenderer (ItemRenderer impl)
  ffi.rs              # manual FFI bindings (~50 functions)
  tests.rs            # struct size / constant sanity checks

internal/backends/winit/
  renderer/impeller.rs  # WinitImpellerRenderer (WinitCompatibleRenderer adapter)
  build.rs              # cfg alias: enable_impeller_renderer
  lib.rs                # renderer wiring (factory, fallback, cfg_if)
```

## Feature Flag Chain

```
slint (api crate)        renderer-impeller
  -> backend-selector    renderer-impeller
    -> backend-winit     renderer-impeller -> dep:i-slint-renderer-impeller, dep:glutin, dep:glutin-winit
```

The `cfg_if!` default renderer priority is: skia > femtovg-wgpu > femtovg > **impeller** > software.

## Architecture

### Rendering Pipeline

```
Slint item tree
  -> WindowInner::draw_contents()
    -> render_component_items()
      -> ImpellerItemRenderer methods (records into ImpellerDisplayListBuilder)
        -> finalize into ImpellerDisplayList
          -> ImpellerSurfaceDrawDisplayList (submits to GPU)
            -> glutin swap_buffers
```

### Key Difference from Skia/FemtoVG

Impeller uses a **retained-mode display list** model: each frame creates a `DisplayListBuilder`, records all draw commands, finalizes into an immutable `DisplayList`, then submits it to a surface. Skia provides an immediate-mode `Canvas`; FemtoVG is also immediate-mode.

### OpenGL Context

Uses glutin directly (not glutin-winit) for EGL context creation. Tries GLES 3.0, then GLES 2.0, then default GL. The Impeller context is created by passing glutin's `get_proc_address` as a callback to `ImpellerContextCreateOpenGLESNew`.

## Key Types

### `ImpellerRenderer` (`lib.rs`)

Top-level renderer holding all GPU state:

| Field | Type | Purpose |
|---|---|---|
| `impeller_context` | `RefCell<Option<*mut c_void>>` | Impeller GPU context |
| `typography_context` | `RefCell<Option<*mut c_void>>` | Font registration / text shaping |
| `glutin_context` | `RefCell<Option<PossiblyCurrentContext>>` | OpenGL context |
| `glutin_surface` | `RefCell<Option<Surface<WindowSurface>>>` | EGL surface |
| `size` | `Cell<PhysicalWindowSize>` | Current window size |
| `maybe_window_adapter` | `RefCell<Option<Weak<dyn WindowAdapter>>>` | Back-ref to Slint window |

**Lifecycle:**
- `new()` -> all fields None
- `set_window_handle(window, display, size)` -> initializes glutin + Impeller context + registers fonts
- `render()` -> one frame (see pipeline above)
- `resize(size)` -> updates glutin surface size
- `suspend()` -> releases all GPU resources

**`RendererSealed` impl** delegates text sizing to `sharedparley`. Does not support `set_rendering_notifier` or `take_snapshot`. Reports `supports_transformations() = true`.

### `ImpellerItemRenderer` (`itemrenderer.rs`)

Per-frame drawing visitor, wraps an `ImpellerDisplayListBuilder`.

**Implemented:**
- `draw_rectangle` - solid color fill
- `draw_border_rectangle` - fill + stroke, supports rounded corners via `ImpellerRoundingRadii`
- `draw_text` - full paragraph layout via Impeller typography (font family, size, weight, style, alignment)
- `draw_string` - debug text at 16px
- `combine_clip` - `ClipRect` with `Intersect`
- `translate`, `rotate`, `scale` - builder transform calls
- `save_state` / `restore_state` - builder save/restore stack

**Stubbed (empty, no-op):**
- `draw_image`, `draw_image_direct`, `draw_cached_pixmap`
- `draw_text_input`
- `draw_path`
- `draw_box_shadow`
- `visit_opacity`, `visit_layer` (return `ContinueRenderingChildren`)
- `apply_opacity`

### FFI Bindings (`ffi.rs`)

Manual bindings (not bindgen). All Impeller objects are opaque `*mut c_void` handles with Retain/Release reference counting.

**Handle types:** `ImpellerContext`, `ImpellerSurface`, `ImpellerDisplayListBuilder`, `ImpellerDisplayList`, `ImpellerPaint`, `ImpellerTypographyContext`, `ImpellerParagraphStyle`, `ImpellerParagraphBuilder`, `ImpellerParagraph`

**Structs:** `ImpellerRect{x,y,width,height: f32}`, `ImpellerISize{width,height: i64}`, `ImpellerPoint{x,y: f32}`, `ImpellerRoundingRadii{top_left,bottom_left,top_right,bottom_right: ImpellerPoint}`, `ImpellerColor{red,green,blue,alpha: f32, color_space}`, `ImpellerMapping{data,length,on_release}` (for font data)

**Version:** `IMPELLER_VERSION = (1<<29)|(1<<22)|(4<<12)|0` (v1.1.4.0)

### `WinitImpellerRenderer` (`winit/renderer/impeller.rs`)

Thin adapter implementing `WinitCompatibleRenderer`. `new_suspended` ignores `SharedBackendData` (no shared context needed). `resume` creates the winit window and calls `set_window_handle`.

## Font Registration

Fonts are registered with Impeller's `ImpellerTypographyContext` at context creation time. The process:

1. Enumerate fonts from fontique's collection (default fonts + default sans-serif)
2. For each font, copy the raw font data into a leaked `Box<[u8]>` (Impeller stores a raw pointer without copying)
3. Call `ImpellerTypographyContextRegisterFont` with the data and family name alias
4. TTC fonts with index > 0 are skipped (Impeller doesn't handle TTC indices)

The font family name is resolved from fontique's collection. When `draw_text` receives no explicit family, it queries fontique for the default sans-serif family and passes that name to `ImpellerParagraphStyleSetFontFamily`.

## Render Loop Detail (`ImpellerRenderer::render()`)

1. `glutin_context.make_current(surface)`
2. Create `ImpellerSurface` wrapping FBO 0 (default framebuffer), RGBA8888
3. Create `ImpellerDisplayListBuilder` with cull rect = full window size
4. Create `ImpellerItemRenderer` wrapping the builder
5. Clear background if window has a solid color background
6. `WindowInner::draw_contents()` traverses item tree, calling `render_component_items()` for each component
7. `ImpellerDisplayListBuilderCreateDisplayListNew(builder)` - finalize
8. `ImpellerSurfaceDrawDisplayList(surface, display_list)` - submit
9. `glutin_surface.swap_buffers()` - present
10. Release display list and surface

## What Needs Implementing

| Feature | Impeller C API needed | Notes |
|---|---|---|
| Images | `ImpellerDisplayListBuilderDrawTexture` or equivalent | Need to create `ImpellerTexture` from pixel data |
| Paths | `ImpellerPathBuilderNew`, `LineTo`, `CubicCurveTo`, etc. | Impeller has a full path builder API |
| Box shadows | No direct API; draw blurred rounded rect | May need `ImpellerPaintSetMaskFilter` or manual approach |
| Opacity | `ImpellerDisplayListBuilderSaveLayer` | Save layer with alpha paint |
| Text input | Reuse `draw_text` logic + cursor/selection rects | Cursor and selection drawing on top of text |
| Gradients | `ImpellerColorSourceCreateLinearGradientNew` etc. | Impeller has linear/radial/conical/sweep gradient APIs |
| Clipping with border radius | `ImpellerDisplayListBuilderClipRoundedRect` | Currently only rectangular clips |
| Snapshots | Read back FBO pixels | `glReadPixels` or Impeller equivalent |

## Impeller C API Header

Located at `impeller_install_debug_unopt/include/impeller.h` (3133 lines). The SDK also includes `impellerc` (shader compiler) and `libimpeller.so`. The SDK is built from Flutter's engine source using `build_impeller.sh`.

## Coordinate System

All drawing in `ImpellerItemRenderer` converts from Slint's logical coordinates to physical pixels by multiplying by `scale_factor`. The display list builder operates in physical pixel space. Transforms (translate, rotate, scale) are applied to the builder's transform stack.
