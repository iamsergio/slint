# Impeller Renderer for Slint

## What This Is

A Slint renderer backed by Flutter's [Impeller](https://github.com/flutter/engine/tree/main/impeller) graphics engine, linked via its C API (`libimpeller.so`). Currently supports rectangles, rounded rects, borders, text (sharedparley layout, swash glyph atlas), text input, transforms, images (all ImageFit modes, SVG via resvg rasterization), gradients (linear, radial, conic/sweep), opacity compositing, clip tracking (rounded clips), paths and box shadows.

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
  glyphatlas.rs       # swash-rasterized glyph atlas (R8 textures, subpixel bins)
  ffi.rs              # manual FFI bindings (~80 functions)
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
| `glutin_context` | `RefCell<Option<PossiblyCurrentContext>>` | OpenGL context |
| `glutin_surface` | `RefCell<Option<Surface<WindowSurface>>>` | EGL surface |
| `size` | `Cell<PhysicalWindowSize>` | Current window size |
| `maybe_window_adapter` | `RefCell<Option<Weak<dyn WindowAdapter>>>` | Back-ref to Slint window |

**Lifecycle:**
- `new()` -> all fields None
- `set_window_handle(window, display, size)` -> initializes glutin + Impeller context
- `render()` -> one frame (see pipeline above)
- `resize(size)` -> updates glutin surface size
- `suspend()` -> releases all GPU resources

**`RendererSealed` impl** delegates text sizing to `sharedparley`. Does not support `set_rendering_notifier` or `take_snapshot`. Reports `supports_transformations() = true`.

### `ImpellerItemRenderer` (`itemrenderer.rs`)

Per-frame drawing visitor, wraps an `ImpellerDisplayListBuilder`.

**Implemented:**
- `draw_rectangle` - solid/gradient fill
- `draw_border_rectangle` - fill + stroke, rounded corners via `ImpellerRoundingRadii`
- `draw_text` / `draw_text_input` / `draw_string` - layout by `sharedparley` (the `GlyphRenderer` trait is implemented on the item renderer); glyphs are rasterized with swash into `GlyphAtlas` (`glyphatlas.rs`) and drawn as textured rects, snapped to the pixel grid relative to the accumulated translation `origin`; colour glyphs are supported, fills/stroke by solid colour
- `draw_image` - texture from `image_to_texture()`, cached as in FemtoVG: a per-item `ItemCache` (invalidated with the item's properties, released in `free_graphics_resources`) over a shared `TextureCache` keyed by `ImageCacheKey` and the SVG target size (`textures.rs`), drained after each frame and cleared in `suspend()`; ImageFit (Fill, Contain, Preserve, Cover), ImageRendering via `DrawTextureRect`
- `draw_image_direct` - texture at origin
- `setup_paint_for_brush` - SolidColor, LinearGradient, RadialGradient, ConicGradient (sweep)
- `combine_clip` - `ClipRect`, or `ClipRoundedRect` when the radius is non-zero (the border width is already folded in by core's `visit_clip` via `clip_content_box`)
- Clip tracking: `clip` is a `LogicalRect` in the current coordinate system; `combine_clip` intersects (returns false when empty), `translate` shifts it, `scale` divides it, `rotate` replaces it with the bounding box of the back-rotated rect (conservative); `save_state`/`restore_state` push/pop it. `get_current_clip` returns it so core can cull off-screen items
- `draw_path` - `ImpellerPathBuilder` from `fitted_path_events` (move/line/quad/cubic/close), fill with the item's fill rule (`NonZero`/`Odd`), stroke with width, cap, join and miter limit; drawn after a builder translate by the fit offset
- `draw_box_shadow` - non-inset: blurred rounded rect (`ImpellerMaskFilterCreateBlurNew`, Normal, sigma = blur/2) at offset, grown by spread; inset: clipped to the item, an even-odd path (big rect minus offset/spread rounded hole) filled with the same blur. Shadow shape is the item's rounded rect; a non-opaque or bordered source is not reproduced
- `translate`, `rotate`, `scale`, `save_state` / `restore_state` - builder transform stack
- `visit_opacity` - `SaveLayer` with alpha, or inline `apply_opacity` when no layer is needed

**Partial (missing sub-features):**
- `draw_image` - `colorize`, `source_clip`, `alignment`, `tiling` properties not handled
- `draw_path` - `stroke-dash-array` / `stroke-dash-offset` are ignored (no path effect in the C API); no viewbox-aware gradients
- `draw_box_shadow` - shadow is always a solid rounded rect, not the item's real fill/border

**Stubbed (empty, no-op):**
- `draw_cached_pixmap`
- `visit_layer` (returns `ContinueRenderingChildren`)

### FFI Bindings (`ffi.rs`)

Manual bindings (not bindgen). All Impeller objects are opaque `*mut c_void` handles with Retain/Release reference counting.

**Handle types:** `ImpellerContext`, `ImpellerSurface`, `ImpellerDisplayListBuilder`, `ImpellerDisplayList`, `ImpellerPaint`, `ImpellerTexture`, `ImpellerColorSource`, `ImpellerColorFilter`, `ImpellerPath`, `ImpellerPathBuilder`, `ImpellerMaskFilter`

**Structs:** `ImpellerRect{x,y,width,height: f32}`, `ImpellerISize{width,height: i64}`, `ImpellerPoint{x,y: f32}`, `ImpellerRoundingRadii{top_left,bottom_left,top_right,bottom_right: ImpellerPoint}`, `ImpellerColor{red,green,blue,alpha: f32, color_space}`, `ImpellerMapping{data,length,on_release}` (texture data), `ImpellerTextureDescriptor{pixel_format, size, mip_count}`

**Version:** `IMPELLER_VERSION = (1<<29)|(1<<22)|(4<<12)|0` (v1.1.4.0)

### `WinitImpellerRenderer` (`winit/renderer/impeller.rs`)

Thin adapter implementing `WinitCompatibleRenderer`. `new_suspended` ignores `SharedBackendData` (no shared context needed). `resume` creates the winit window and calls `set_window_handle`.

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

| Feature | Notes |
|---|---|
| Image colorize, source_clip, tiling | `image.colorize()` never read |
| Path dashes | no API in the C header |
| Layers (`cache-rendering-hint`) | `visit_layer` is a pass-through |
| Backdrop blur, accurate shadows for translucent/bordered items | |

## Impeller C API Header

Located at `impeller_install_debug_unopt/include/impeller.h` (3133 lines). The SDK also includes `impellerc` (shader compiler) and `libimpeller.so`. The SDK is built from Flutter's engine source using `build_impeller.sh`.

## Coordinate System

All drawing in `ImpellerItemRenderer` converts from Slint's logical coordinates to physical pixels by multiplying by `scale_factor`. The display list builder operates in physical pixel space. Transforms (translate, rotate, scale) are applied to the builder's transform stack.
